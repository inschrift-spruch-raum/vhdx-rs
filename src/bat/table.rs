//! Block Allocation Table (BAT) structure

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

use super::entry::BatEntry;
use super::states::PayloadBlockState;

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
    /// File offset of BAT region (for updates)
    pub bat_file_offset: u64,
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
        virtual_disk_size.div_ceil(block_size)
    }

    /// Calculate number of sector bitmap blocks
    pub fn calculate_num_sector_bitmap_blocks(num_payload_blocks: u64, chunk_ratio: u64) -> u64 {
        num_payload_blocks.div_ceil(chunk_ratio)
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
            bat_file_offset: 0, // Must be set by caller after creation
        })
    }

    /// Set BAT file offset
    pub fn set_bat_file_offset(&mut self, offset: u64) {
        self.bat_file_offset = offset;
    }

    /// Get file offset of a BAT entry
    pub fn get_bat_entry_file_offset(&self, bat_index: usize) -> u64 {
        self.bat_file_offset + (bat_index as u64 * 8)
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

    /// Calculate chunk index from block index
    pub fn chunk_index_from_block(&self, block_idx: u64) -> u64 {
        block_idx / self.chunk_ratio
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

#[cfg(test)]
mod tests {
    use super::*;

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
            bat_file_offset: 1024 * 1024,       // 1MB
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
}
