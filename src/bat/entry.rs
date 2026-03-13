//! BAT Entry structure and bit operations

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

use super::states::PayloadBlockState;

/// BAT Entry (64 bits)
///
/// Bit layout according to MS-VHDX Section 2.5.1:
/// - Bits 0-2: State (3 bits)
/// - Bits 3-19: Reserved (17 bits) - must be zero
/// - Bits 20-63: FileOffsetMB (44 bits) - file offset in MB
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
}
