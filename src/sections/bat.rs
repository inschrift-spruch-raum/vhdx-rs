//! Block Allocation Table (BAT) implementation
//!
//! The BAT maps virtual disk blocks to physical file locations.
//! Each entry is 64 bits containing:
//! - Bits 0-2: State (3 bits)
//! - Bits 3-19: Reserved (17 bits)
//! - Bits 20-63: File Offset in MB (44 bits)

use crate::common::constants::{BAT_ENTRY_SIZE, CHUNK_RATIO_CONSTANT, MB};
use crate::error::{Error, Result};

/// BAT Section
pub struct Bat {
    raw_data: Vec<u8>,
    entry_count: usize,
}

impl Bat {
    /// Create a new BAT from raw data
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidFile` if the provided data is too small for the entry count
    /// or if `entry_count` exceeds `usize::MAX`
    pub fn new(data: Vec<u8>, entry_count: u64) -> Result<Self> {
        let entry_count: usize = entry_count.try_into().map_err(|_| {
            Error::InvalidFile(format!("entry_count {entry_count} exceeds usize::MAX"))
        })?;
        let expected_size = entry_count
            .checked_mul(BAT_ENTRY_SIZE)
            .ok_or_else(|| Error::InvalidFile("BAT size overflow".to_string()))?;
        if data.len() < expected_size {
            return Err(Error::InvalidFile(format!(
                "BAT data too small: expected at least {} bytes, got {}",
                expected_size,
                data.len()
            )));
        }
        Ok(Self {
            raw_data: data,
            entry_count,
        })
    }

    /// Return the complete BAT raw bytes
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// Get a BAT entry by index
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<BatEntry> {
        if index >= self.entry_count {
            return None;
        }
        let offset = index * BAT_ENTRY_SIZE;
        let raw_value = u64::from_le_bytes([
            self.raw_data[offset],
            self.raw_data[offset + 1],
            self.raw_data[offset + 2],
            self.raw_data[offset + 3],
            self.raw_data[offset + 4],
            self.raw_data[offset + 5],
            self.raw_data[offset + 6],
            self.raw_data[offset + 7],
        ]);
        // Skip entries with reserved states (4, 5) - return None for invalid entries
        BatEntry::from_raw(raw_value).ok()
    }

    /// Get all BAT entries
    #[must_use]
    pub const fn entries(&self) -> BatEntryIter<'_> {
        BatEntryIter {
            bat: self,
            current: 0,
            end: self.entry_count,
        }
    }

    /// Get the number of entries
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entry_count
    }

    /// Check if BAT is empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Calculate chunk ratio: (2^23 * `LogicalSectorSize`) / `BlockSize`
    #[must_use]
    pub fn calculate_chunk_ratio(logical_sector_size: u32, block_size: u32) -> u32 {
        let result =
            (CHUNK_RATIO_CONSTANT * u64::from(logical_sector_size)) / u64::from(block_size);
        result.try_into().unwrap_or(u32::MAX)
    }

    /// Calculate the number of payload blocks from virtual disk size
    #[must_use]
    pub fn calculate_payload_blocks(virtual_disk_size: u64, block_size: u32) -> u64 {
        virtual_disk_size.div_ceil(u64::from(block_size))
    }

    /// Calculate the number of sector bitmap blocks
    #[must_use]
    pub fn calculate_sector_bitmap_blocks(payload_blocks: u64, chunk_ratio: u32) -> u64 {
        payload_blocks.div_ceil(u64::from(chunk_ratio))
    }

    /// Calculate total BAT entries (payload + sector bitmap)
    #[must_use]
    pub fn calculate_total_entries(
        virtual_disk_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> u64 {
        let payload_blocks = Self::calculate_payload_blocks(virtual_disk_size, block_size);
        let chunk_ratio = Self::calculate_chunk_ratio(logical_sector_size, block_size);
        let sector_bitmap_blocks =
            Self::calculate_sector_bitmap_blocks(payload_blocks, chunk_ratio);
        payload_blocks + sector_bitmap_blocks
    }
}

/// Iterator over BAT entries
pub struct BatEntryIter<'a> {
    bat: &'a Bat,
    current: usize,
    end: usize,
}

impl Iterator for BatEntryIter<'_> {
    type Item = (usize, BatEntry);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }
        let entry = self.bat.entry(self.current)?;
        let index = self.current;
        self.current += 1;
        Some((index, entry))
    }
}

/// BAT Entry
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatEntry {
    /// Entry state
    pub state: BatState,
    /// File offset in MB
    pub file_offset_mb: u64,
}

impl BatEntry {
    /// Create from raw 64-bit value
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidBlockState` if the state bits represent a reserved state (4 or 5)
    pub fn from_raw(raw: u64) -> std::result::Result<Self, Error> {
        let state_bits = (raw & 0x7) as u8;
        let offset_mb = raw >> 20;

        let state = BatState::from_bits(state_bits)?;

        Ok(Self {
            state,
            file_offset_mb: offset_mb,
        })
    }

    /// Convert to raw 64-bit value
    #[must_use]
    pub fn raw(&self) -> u64 {
        let state_bits = u64::from(self.state.to_bits());
        (self.file_offset_mb << 20) | state_bits
    }

    /// Get the file offset in bytes
    #[must_use]
    pub const fn file_offset(&self) -> u64 {
        self.file_offset_mb * MB
    }

    /// Create a new entry
    #[must_use]
    pub const fn new(state: BatState, file_offset_mb: u64) -> Self {
        Self {
            state,
            file_offset_mb,
        }
    }
}

/// BAT State - determines the type and state of the entry
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatState {
    /// Payload block state
    Payload(PayloadBlockState),
    /// Sector bitmap block state
    SectorBitmap(SectorBitmapState),
}

impl BatState {
    /// Parse from 3-bit value
    ///
    /// Per MS-VHDX spec, states 4 and 5 are reserved and should be treated as errors
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidBlockState` if the bits represent a reserved state (4 or 5)
    pub const fn from_bits(bits: u8) -> std::result::Result<Self, Error> {
        match bits {
            0 | 1 | 2 | 3 | 6 | 7 => Ok(Self::Payload(PayloadBlockState::from_bits(bits))),
            _ => Err(Error::InvalidBlockState(bits)),
        }
    }

    /// Convert to 3-bit state value
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::Payload(state) => state.to_bits(),
            Self::SectorBitmap(state) => state.to_bits(),
        }
    }
}

/// Payload Block State
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadBlockState {
    /// Block is not present (not allocated)
    NotPresent = 0,
    /// Block content is undefined
    Undefined = 1,
    /// Block content is all zeros
    Zero = 2,
    /// Block has been unmapped (TRIM/UNMAP)
    Unmapped = 3,
    /// Block is fully present and allocated
    FullyPresent = 6,
    /// Block is partially present (differencing disks)
    PartiallyPresent = 7,
}

impl PayloadBlockState {
    /// Parse from 3-bit value
    ///
    /// Note: States 4 and 5 are reserved per MS-VHDX spec and should not be used.
    /// They are handled as errors at the `BatState` level.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Undefined,
            2 => Self::Zero,
            3 => Self::Unmapped,
            6 => Self::FullyPresent,
            7 => Self::PartiallyPresent,
            _ => Self::NotPresent, // 0 and invalid states (4, 5, 8+)
        }
    }

    /// Convert to 3-bit value
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::NotPresent => 0,
            Self::Undefined => 1,
            Self::Zero => 2,
            Self::Unmapped => 3,
            Self::FullyPresent => 6,
            Self::PartiallyPresent => 7,
        }
    }

    /// Check if the block is allocated
    #[must_use]
    pub const fn is_allocated(&self) -> bool {
        matches!(self, Self::FullyPresent | Self::PartiallyPresent)
    }

    /// Check if the block needs to be read from file
    #[must_use]
    pub const fn needs_read(&self) -> bool {
        matches!(
            self,
            Self::FullyPresent | Self::PartiallyPresent | Self::Undefined
        )
    }
}

/// Sector Bitmap Block State (for differencing disks)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorBitmapState {
    /// Sector bitmap block is not present
    NotPresent = 0,
    /// Sector bitmap block is present
    Present = 6,
}

impl SectorBitmapState {
    /// Parse from 3-bit value
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            6 => Self::Present,
            _ => Self::NotPresent, // 0 and invalid states
        }
    }

    /// Convert to 3-bit value
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::NotPresent => 0,
            Self::Present => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bat_entry_from_raw() {
        // State = 6 (FullyPresent), Offset = 1 MB
        let raw = (1u64 << 20) | 6u64;
        let entry = BatEntry::from_raw(raw).unwrap();

        assert_eq!(entry.file_offset_mb, 1);
        assert_eq!(entry.file_offset(), MB);
        assert!(matches!(
            entry.state,
            BatState::Payload(PayloadBlockState::FullyPresent)
        ));
    }

    #[test]
    fn test_bat_entry_to_raw() {
        let entry = BatEntry::new(BatState::Payload(PayloadBlockState::FullyPresent), 1);
        let raw = entry.raw();
        assert_eq!(raw & 0x7, 6); // State bits
        assert_eq!(raw >> 20, 1); // Offset MB
    }

    #[test]
    fn test_payload_block_states() {
        assert_eq!(PayloadBlockState::NotPresent.to_bits(), 0);
        assert_eq!(PayloadBlockState::FullyPresent.to_bits(), 6);
        assert_eq!(PayloadBlockState::PartiallyPresent.to_bits(), 7);

        assert!(PayloadBlockState::FullyPresent.is_allocated());
        assert!(PayloadBlockState::PartiallyPresent.is_allocated());
        assert!(!PayloadBlockState::NotPresent.is_allocated());
        assert!(!PayloadBlockState::Zero.is_allocated());
    }

    #[test]
    fn test_chunk_ratio_calculation() {
        // ChunkRatio = (2^23 * 512) / (32 * 1024 * 1024)
        // = (8388608 * 512) / 33554432
        // = 128
        let ratio = Bat::calculate_chunk_ratio(512, 32 * 1024 * 1024);
        assert_eq!(ratio, 128);
    }

    #[test]
    fn test_calculate_payload_blocks() {
        // 10 GB disk with 32 MB blocks
        let blocks = Bat::calculate_payload_blocks(10 * 1024 * MB, 32 * 1024 * 1024);
        assert_eq!(blocks, 320);
    }
}
