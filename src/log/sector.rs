//! Data Sector structure

use crate::error::{Result, VhdxError};
use crate::log::DATA_SECTOR_SIGNATURE;
use byteorder::LittleEndian;

/// Data Sector (4KB)
#[derive(Debug, Clone)]
pub struct DataSector {
    pub signature: [u8; 4],
    pub sequence_high: u32,
    pub data: [u8; 4084],
    pub sequence_low: u32,
}

impl DataSector {
    /// Size of sector
    pub const SIZE: usize = 4096;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != DATA_SECTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(DATA_SECTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let sequence_high = LittleEndian::read_u32(&data[4..8]);

        let mut sector_data = [0u8; 4084];
        sector_data.copy_from_slice(&data[8..4092]);

        let sequence_low = LittleEndian::read_u32(&data[4092..4096]);

        Ok(DataSector {
            signature,
            sequence_high,
            data: sector_data,
            sequence_low,
        })
    }

    /// Get full sequence number
    pub fn sequence_number(&self) -> u64 {
        ((self.sequence_high as u64) << 32) | (self.sequence_low as u64)
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number() == header_seq
    }

    /// Reconstruct full 4KB sector data
    /// Combines leading bytes (from descriptor) + data + trailing bytes (from descriptor)
    pub fn reconstruct_sector(
        &self,
        descriptor: &crate::log::descriptor::DataDescriptor,
    ) -> [u8; 4096] {
        let mut full_data = [0u8; 4096];

        // Leading bytes (first 8 bytes)
        full_data[0..8].copy_from_slice(&descriptor.leading_bytes);

        // Data (bytes 8-4091)
        full_data[8..4092].copy_from_slice(&self.data);

        // Trailing bytes (last 4 bytes)
        full_data[4092..4096].copy_from_slice(&descriptor.trailing_bytes);

        full_data
    }
}
