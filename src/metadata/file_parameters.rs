//! File Parameters metadata item
//!
//! Contains block size and parent disk information.

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};
use uuid::Uuid;

/// File Parameters GUID: CAA16737-FA36-4D43-B3B6-33F0AA44E76B
pub const FILE_PARAMETERS_GUID: crate::common::guid::Guid =
    crate::common::guid::Guid(Uuid::from_bytes_le([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7,
        0x6B,
    ]));

/// File Parameters metadata item
#[derive(Debug, Clone)]
pub struct FileParameters {
    pub block_size: u32,
    pub leave_block_allocated: bool,
    pub has_parent: bool,
}

impl FileParameters {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::InvalidMetadata(
                "FileParameters too small".to_string(),
            ));
        }

        let block_size = LittleEndian::read_u32(&data[0..4]);
        let flags = LittleEndian::read_u32(&data[4..8]);
        let leave_block_allocated = flags & 0x1 != 0;
        let has_parent = flags & 0x2 != 0;

        // Validate block size (1MB to 256MB, must be 1MB multiple)
        if block_size < 1024 * 1024 || block_size > 256 * 1024 * 1024 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Invalid block size: {}",
                block_size
            )));
        }

        if block_size % (1024 * 1024) != 0 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Block size {} not 1MB aligned",
                block_size
            )));
        }

        Ok(FileParameters {
            block_size,
            leave_block_allocated,
            has_parent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_parameters() {
        let mut data = vec![0u8; 16];
        LittleEndian::write_u32(&mut data[0..4], 1024 * 1024); // 1MB block size
        LittleEndian::write_u32(&mut data[4..8], 0); // No parent

        let params = FileParameters::from_bytes(&data).unwrap();
        assert_eq!(params.block_size, 1024 * 1024);
        assert!(!params.has_parent);
        assert!(!params.leave_block_allocated);
    }

    #[test]
    fn test_file_parameters_with_parent() {
        let mut data = vec![0u8; 16];
        LittleEndian::write_u32(&mut data[0..4], 32 * 1024 * 1024); // 32MB block size
        LittleEndian::write_u32(&mut data[4..8], 0x2); // Has parent (bit 1)

        let params = FileParameters::from_bytes(&data).unwrap();
        assert_eq!(params.block_size, 32 * 1024 * 1024);
        assert!(params.has_parent);
    }
}
