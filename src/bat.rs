//! VHDX Block Allocation Table (BAT) structures and operations
//!
//! The BAT is a redirection table that translates virtual disk offsets
//! to file offsets for payload blocks and sector bitmap blocks.

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

/// BAT Entry state for payload blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadBlockState {
    /// Block is not present (unallocated)
    NotPresent = 0,
    /// Block content is undefined
    Undefined = 1,
    /// Block should be read as all zeros
    Zero = 2,
    /// Block was unmapped via TRIM/UNMAP
    Unmapped = 3,
    /// Block is fully present in this file
    FullyPresent = 6,
    /// Block is partially present (differencing disk only)
    PartiallyPresent = 7,
}

impl PayloadBlockState {
    /// Parse from 3-bit value
    pub fn from_bits(bits: u8) -> Result<Self> {
        match bits {
            0 => Ok(PayloadBlockState::NotPresent),
            1 => Ok(PayloadBlockState::Undefined),
            2 => Ok(PayloadBlockState::Zero),
            3 => Ok(PayloadBlockState::Unmapped),
            6 => Ok(PayloadBlockState::FullyPresent),
            7 => Ok(PayloadBlockState::PartiallyPresent),
            _ => Err(VhdxError::InvalidBatEntry),
        }
    }

    /// Convert to bits
    pub fn to_bits(self) -> u8 {
        self as u8
    }

    /// Check if data should be read from this file
    pub fn is_present(&self) -> bool {
        matches!(
            self,
            PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent
        )
    }

    /// Check if block should return zeros
    pub fn is_zero(&self) -> bool {
        matches!(
            self,
            PayloadBlockState::Zero | PayloadBlockState::NotPresent
        )
    }
}

/// BAT Entry state for sector bitmap blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorBitmapState {
    /// Block is not present (unallocated)
    NotPresent = 0,
    /// Block is present
    Present = 6,
}

impl SectorBitmapState {
    /// Parse from 3-bit value
    pub fn from_bits(bits: u8) -> Result<Self> {
        match bits {
            0 => Ok(SectorBitmapState::NotPresent),
            6 => Ok(SectorBitmapState::Present),
            _ => Err(VhdxError::InvalidBatEntry),
        }
    }

    /// Convert to bits
    pub fn to_bits(self) -> u8 {
        self as u8
    }
}

/// BAT Entry (64 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatEntry {
    /// State of the block
    pub state: PayloadBlockState,
    /// File offset in MB (44 bits, 0 if block not present)
    pub file_offset_mb: u64,
    /// Raw entry value
    pub raw: u64,
}

impl BatEntry {
    /// Parse from raw 64-bit value
    pub fn from_raw(raw: u64) -> Result<Self> {
        let state_bits = (raw & 0x7) as u8;
        let state = PayloadBlockState::from_bits(state_bits)?;
        let file_offset_mb = (raw >> 20) & 0xFFFFFFFFFFF; // 44 bits

        Ok(BatEntry {
            state,
            file_offset_mb,
            raw,
        })
    }

    /// Create new entry
    pub fn new(state: PayloadBlockState, file_offset_mb: u64) -> Self {
        let raw = ((file_offset_mb & 0xFFFFFFFFFFF) << 20) | (state.to_bits() as u64);
        BatEntry {
            state,
            file_offset_mb,
            raw,
        }
    }

    /// Get file offset in bytes
    pub fn file_offset(&self) -> Option<u64> {
        if self.file_offset_mb == 0 && !self.state.is_present() {
            None
        } else {
            Some(self.file_offset_mb * 1024 * 1024)
        }
    }

    /// Serialize to raw value
    pub fn to_raw(&self) -> u64 {
        self.raw
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        LittleEndian::write_u64(&mut bytes, self.raw);
        bytes
    }
}

/// Block Allocation Table
#[derive(Debug, Clone)]
pub struct Bat {
    /// BAT entries (includes both payload and sector bitmap entries)
    pub entries: Vec<BatEntry>,
    /// Virtual disk size in bytes
    pub virtual_disk_size: u64,
    /// Block size in bytes
    pub block_size: u64,
    /// Logical sector size in bytes
    pub logical_sector_size: u32,
    /// Number of payload blocks
    pub num_payload_blocks: u64,
    /// Number of sector bitmap blocks
    pub num_sector_bitmap_blocks: u64,
    /// Chunk ratio (payload blocks per sector bitmap)
    pub chunk_ratio: u64,
    /// Chunk size in bytes
    pub chunk_size: u64,
}

impl Bat {
    /// Calculate chunk ratio
    ///
    /// Formula: ChunkSize = 2^23 * LogicalSectorSize
    ///          ChunkRatio = ChunkSize / BlockSize
    pub fn calculate_chunk_ratio(block_size: u64, logical_sector_size: u32) -> u64 {
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;
        chunk_size / block_size
    }

    /// Calculate number of payload blocks
    pub fn calculate_num_payload_blocks(virtual_disk_size: u64, block_size: u64) -> u64 {
        (virtual_disk_size + block_size - 1) / block_size
    }

    /// Calculate number of sector bitmap blocks
    pub fn calculate_num_sector_bitmap_blocks(num_payload_blocks: u64, chunk_ratio: u64) -> u64 {
        (num_payload_blocks + chunk_ratio - 1) / chunk_ratio
    }

    /// Create new BAT from raw data
    pub fn from_bytes(
        data: &[u8],
        virtual_disk_size: u64,
        block_size: u64,
        logical_sector_size: u32,
    ) -> Result<Self> {
        // Calculate derived values
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;
        let chunk_ratio = chunk_size / block_size;
        let num_payload_blocks = Self::calculate_num_payload_blocks(virtual_disk_size, block_size);
        let num_sector_bitmap_blocks =
            Self::calculate_num_sector_bitmap_blocks(num_payload_blocks, chunk_ratio);

        // Calculate expected number of entries
        let expected_entries = num_payload_blocks + num_sector_bitmap_blocks;

        // Parse entries
        let mut entries = Vec::with_capacity(expected_entries as usize);
        for i in 0..expected_entries {
            let offset = i as usize * 8;
            if offset + 8 > data.len() {
                return Err(VhdxError::InvalidBatEntry);
            }
            let raw = LittleEndian::read_u64(&data[offset..offset + 8]);
            let entry = BatEntry::from_raw(raw)?;
            entries.push(entry);
        }

        Ok(Bat {
            entries,
            virtual_disk_size,
            block_size,
            logical_sector_size,
            num_payload_blocks,
            num_sector_bitmap_blocks,
            chunk_ratio,
            chunk_size,
        })
    }

    /// Calculate BAT index for a payload block
    ///
    /// Formula: bat_index = chunk_index * (chunk_ratio + 1) + block_in_chunk
    pub fn payload_bat_index(&self, block_idx: u64) -> Option<usize> {
        if block_idx >= self.num_payload_blocks {
            return None;
        }

        let chunk_idx = block_idx / self.chunk_ratio;
        let block_in_chunk = block_idx % self.chunk_ratio;
        let bat_idx = chunk_idx * (self.chunk_ratio + 1) + block_in_chunk;

        Some(bat_idx as usize)
    }

    /// Calculate BAT index for a sector bitmap block
    ///
    /// Formula: bat_index = chunk_idx * (chunk_ratio + 1) + chunk_ratio
    pub fn sector_bitmap_bat_index(&self, chunk_idx: u64) -> Option<usize> {
        if chunk_idx >= self.num_sector_bitmap_blocks {
            return None;
        }

        let bat_idx = chunk_idx * (self.chunk_ratio + 1) + self.chunk_ratio;
        Some(bat_idx as usize)
    }

    /// Get entry for a payload block
    pub fn get_payload_entry(&self, block_idx: u64) -> Option<&BatEntry> {
        self.payload_bat_index(block_idx)
            .and_then(|idx| self.entries.get(idx))
    }

    /// Get entry for a sector bitmap block
    pub fn get_sector_bitmap_entry(&self, chunk_idx: u64) -> Option<&BatEntry> {
        self.sector_bitmap_bat_index(chunk_idx)
            .and_then(|idx| self.entries.get(idx))
    }

    /// Calculate chunk index from virtual offset
    pub fn chunk_index_from_offset(&self, virtual_offset: u64) -> u64 {
        virtual_offset / self.chunk_size
    }

    /// Calculate block index from virtual offset
    pub fn block_index_from_offset(&self, virtual_offset: u64) -> u64 {
        virtual_offset / self.block_size
    }

    /// Calculate offset within block from virtual offset
    pub fn offset_in_block(&self, virtual_offset: u64) -> u64 {
        virtual_offset % self.block_size
    }

    /// Translate virtual offset to file offset
    ///
    /// Returns None if block is not present (should read zeros)
    pub fn translate(&self, virtual_offset: u64) -> Result<Option<u64>> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let block_idx = self.block_index_from_offset(virtual_offset);
        let offset_in_block = self.offset_in_block(virtual_offset);

        let entry = self
            .get_payload_entry(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;

        match entry.state {
            PayloadBlockState::FullyPresent => {
                let file_offset = entry.file_offset_mb * 1024 * 1024;
                Ok(Some(file_offset + offset_in_block))
            }
            PayloadBlockState::PartiallyPresent => {
                // Differencing disk - need to check sector bitmap
                // For now, return None to indicate parent lookup needed
                Ok(None)
            }
            PayloadBlockState::Zero
            | PayloadBlockState::NotPresent
            | PayloadBlockState::Unmapped => Ok(None),
            PayloadBlockState::Undefined => Err(VhdxError::InvalidBatEntry),
        }
    }

    /// Get total BAT size in bytes
    pub fn bat_size_bytes(&self) -> u64 {
        self.entries.len() as u64 * 8
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![0u8; self.entries.len() * 8];
        for (i, entry) in self.entries.iter().enumerate() {
            let offset = i * 8;
            LittleEndian::write_u64(&mut data[offset..offset + 8], entry.raw);
        }
        data
    }

    /// Update an entry
    pub fn update_entry(&mut self, index: usize, entry: BatEntry) -> Result<()> {
        if index >= self.entries.len() {
            return Err(VhdxError::InvalidBatEntry);
        }
        self.entries[index] = entry;
        Ok(())
    }

    /// Update a payload block entry
    pub fn update_payload_entry(&mut self, block_idx: u64, entry: BatEntry) -> Result<()> {
        let index = self
            .payload_bat_index(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        self.update_entry(index, entry)
    }
}

/// Sector Bitmap operations
pub struct SectorBitmap;

impl SectorBitmap {
    /// Check if a sector is present in the current file
    ///
    /// For differencing disks, returns true if data should be read from
    /// this file, false if it should be read from parent.
    pub fn is_sector_present(bitmap: &[u8], sector_idx: u64) -> bool {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx >= bitmap.len() {
            return false;
        }

        (bitmap[byte_idx] >> bit_idx) & 1 == 1
    }

    /// Set a sector as present
    pub fn set_sector_present(bitmap: &mut [u8], sector_idx: u64) {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx < bitmap.len() {
            bitmap[byte_idx] |= 1 << bit_idx;
        }
    }

    /// Clear a sector (mark as not present)
    pub fn clear_sector(bitmap: &mut [u8], sector_idx: u64) {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx < bitmap.len() {
            bitmap[byte_idx] &= !(1 << bit_idx);
        }
    }

    /// Calculate sector index within chunk from virtual offset
    pub fn sector_index_in_chunk(
        virtual_offset: u64,
        logical_sector_size: u32,
        chunk_size: u64,
    ) -> u64 {
        let offset_in_chunk = virtual_offset % chunk_size;
        offset_in_chunk / logical_sector_size as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bat_entry() {
        // Create entry with state = FullyPresent, offset = 1MB
        let entry = BatEntry::new(PayloadBlockState::FullyPresent, 1);
        assert_eq!(entry.state, PayloadBlockState::FullyPresent);
        assert_eq!(entry.file_offset_mb, 1);
        assert_eq!(entry.file_offset(), Some(1024 * 1024));

        // Parse back from raw
        let entry2 = BatEntry::from_raw(entry.raw).unwrap();
        assert_eq!(entry.state, entry2.state);
        assert_eq!(entry.file_offset_mb, entry2.file_offset_mb);
    }

    #[test]
    fn test_chunk_calculation() {
        // 1MB block size, 512 byte sectors
        let block_size = 1024 * 1024;
        let logical_sector_size = 512;

        let chunk_ratio = Bat::calculate_chunk_ratio(block_size, logical_sector_size);
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        assert_eq!(chunk_size, 4 * 1024 * 1024 * 1024); // 4GB
        assert_eq!(chunk_ratio, 4096); // 4096 blocks per chunk
    }

    #[test]
    fn test_bat_index_calculation() {
        // 1MB blocks, 512 byte sectors
        let bat = Bat {
            entries: vec![],
            virtual_disk_size: 100 * 1024 * 1024 * 1024, // 100GB
            block_size: 1024 * 1024,                     // 1MB
            logical_sector_size: 512,
            num_payload_blocks: 100 * 1024, // 100GB / 1MB
            num_sector_bitmap_blocks: 25,   // 100K / 4096
            chunk_ratio: 4096,
            chunk_size: 4 * 1024 * 1024 * 1024, // 4GB
        };

        // Block 0 should be at index 0
        assert_eq!(bat.payload_bat_index(0), Some(0));

        // Block 4095 should be at index 4095
        assert_eq!(bat.payload_bat_index(4095), Some(4095));

        // Block 4096 should be at index 4097 (after sector bitmap)
        assert_eq!(bat.payload_bat_index(4096), Some(4097));

        // Sector bitmap 0 should be at index 4096
        assert_eq!(bat.sector_bitmap_bat_index(0), Some(4096));
    }

    #[test]
    fn test_sector_bitmap() {
        let mut bitmap = vec![0u8; 1024 * 1024]; // 1MB bitmap

        // Set sector 0
        SectorBitmap::set_sector_present(&mut bitmap, 0);
        assert!(SectorBitmap::is_sector_present(&bitmap, 0));
        assert!(!SectorBitmap::is_sector_present(&bitmap, 1));

        // Set sector 1000
        SectorBitmap::set_sector_present(&mut bitmap, 1000);
        assert!(SectorBitmap::is_sector_present(&bitmap, 1000));

        // Clear sector 0
        SectorBitmap::clear_sector(&mut bitmap, 0);
        assert!(!SectorBitmap::is_sector_present(&bitmap, 0));
    }
}
