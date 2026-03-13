//! Log Descriptors (Zero and Data)

use crate::error::{Result, VhdxError};
use crate::log::{DATA_DESCRIPTOR_SIGNATURE, ZERO_DESCRIPTOR_SIGNATURE};
use byteorder::{ByteOrder, LittleEndian};

/// Zero Descriptor (32 bytes)
#[derive(Debug, Clone)]
pub struct ZeroDescriptor {
    pub signature: [u8; 4],
    pub zero_length: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
}

impl ZeroDescriptor {
    /// Size of descriptor
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != ZERO_DESCRIPTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(ZERO_DESCRIPTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let zero_length = LittleEndian::read_u64(&data[8..16]);
        let file_offset = LittleEndian::read_u64(&data[16..24]);
        let sequence_number = LittleEndian::read_u64(&data[24..32]);

        // Validate zero_length (must be multiple of 4KB)
        if zero_length == 0 || zero_length % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        // Validate file_offset (must be multiple of 4KB)
        if file_offset % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        Ok(ZeroDescriptor {
            signature,
            zero_length,
            file_offset,
            sequence_number,
        })
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number == header_seq
    }
}

/// Data Descriptor (32 bytes)
#[derive(Debug, Clone)]
pub struct DataDescriptor {
    pub signature: [u8; 4],
    pub trailing_bytes: [u8; 4],
    pub leading_bytes: [u8; 8],
    pub file_offset: u64,
    pub sequence_number: u64,
}

impl DataDescriptor {
    /// Size of descriptor
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != DATA_DESCRIPTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(DATA_DESCRIPTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let mut trailing_bytes = [0u8; 4];
        trailing_bytes.copy_from_slice(&data[4..8]);

        let mut leading_bytes = [0u8; 8];
        leading_bytes.copy_from_slice(&data[8..16]);

        let file_offset = LittleEndian::read_u64(&data[16..24]);
        let sequence_number = LittleEndian::read_u64(&data[24..32]);

        // Validate file_offset (must be multiple of 4KB)
        if file_offset % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        Ok(DataDescriptor {
            signature,
            trailing_bytes,
            leading_bytes,
            file_offset,
            sequence_number,
        })
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number == header_seq
    }
}
