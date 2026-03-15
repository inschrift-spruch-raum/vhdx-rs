//! File Parameters metadata item
//!
//! Contains block size and parent disk information.

use crate::error::{Error, Result};
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
            return Err(Error::InvalidMetadata("FileParameters too small".to_string(),));
        }

        let block_size = LittleEndian::read_u32(&data[0..4]);
        let flags = LittleEndian::read_u32(&data[4..8]);
        let leave_block_allocated = flags & 0x1 != 0;
        let has_parent = flags & 0x2 != 0;

        // Validate block size per MS-VHDX Section 2.2.2:
        // Must be power of 2, between 1MB and 256MB inclusive
        const MIN_BLOCK_SIZE: u32 = 1024 * 1024; // 1MB
        const MAX_BLOCK_SIZE: u32 = 256 * 1024 * 1024; // 256MB

        if !(MIN_BLOCK_SIZE..=MAX_BLOCK_SIZE).contains(&block_size) {
            return Err(Error::InvalidBlockSize(block_size));
        }

        // Check power of 2: only one bit set
        if block_size & (block_size - 1) != 0 {
            return Err(Error::InvalidBlockSize(block_size));
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

    #[test]
    fn test_valid_block_sizes() {
        // Test all valid powers of 2 from 1MB to 256MB
        let valid_sizes = vec![
            1024 * 1024,       // 1MB
            2 * 1024 * 1024,   // 2MB
            4 * 1024 * 1024,   // 4MB
            8 * 1024 * 1024,   // 8MB
            16 * 1024 * 1024,  // 16MB
            32 * 1024 * 1024,  // 32MB
            64 * 1024 * 1024,  // 64MB
            128 * 1024 * 1024, // 128MB
            256 * 1024 * 1024, // 256MB
        ];

        for block_size in valid_sizes {
            let mut data = vec![0u8; 16];
            LittleEndian::write_u32(&mut data[0..4], block_size);
            LittleEndian::write_u32(&mut data[4..8], 0);

            let result = FileParameters::from_bytes(&data);
            assert!(
                result.is_ok(),
                "Block size {} (power of 2) should be valid",
                block_size
            );
        }
    }

    #[test]
    fn test_invalid_block_size_non_power_of_2() {
        // Test non-power-of-2 values that should be rejected
        let invalid_sizes = vec![
            3 * 1024 * 1024,   // 3MB
            5 * 1024 * 1024,   // 5MB
            6 * 1024 * 1024,   // 6MB
            7 * 1024 * 1024,   // 7MB
            9 * 1024 * 1024,   // 9MB
            100 * 1024 * 1024, // 100MB
        ];

        for block_size in invalid_sizes {
            let mut data = vec![0u8; 16];
            LittleEndian::write_u32(&mut data[0..4], block_size);
            LittleEndian::write_u32(&mut data[4..8], 0);

            let result = FileParameters::from_bytes(&data);
            assert!(
                result.is_err(),
                "Block size {} (not power of 2) should be rejected",
                block_size
            );
            match result {
                Err(Error::InvalidBlockSize(size)) => {
                    assert_eq!(size, block_size);
                }
                _ => panic!("Expected InvalidBlockSize error"),
            }
        }
    }

    #[test]
    fn test_invalid_block_size_below_min() {
        // Test values below 1MB minimum
        let invalid_sizes = vec![
            512 * 1024, // 512KB
            100 * 1024, // 100KB
            1,          // 1 byte
            1024,       // 1KB
        ];

        for block_size in invalid_sizes {
            let mut data = vec![0u8; 16];
            LittleEndian::write_u32(&mut data[0..4], block_size);
            LittleEndian::write_u32(&mut data[4..8], 0);

            let result = FileParameters::from_bytes(&data);
            assert!(
                result.is_err(),
                "Block size {} (below 1MB) should be rejected",
                block_size
            );
            match result {
                Err(Error::InvalidBlockSize(size)) => {
                    assert_eq!(size, block_size);
                }
                _ => panic!("Expected InvalidBlockSize error"),
            }
        }
    }

    #[test]
    fn test_invalid_block_size_above_max() {
        // Test values above 256MB maximum
        let invalid_sizes = vec![
            512 * 1024 * 1024,  // 512MB
            1024 * 1024 * 1024, // 1GB
            257 * 1024 * 1024,  // 257MB
            u32::MAX,
        ];

        for block_size in invalid_sizes {
            let mut data = vec![0u8; 16];
            LittleEndian::write_u32(&mut data[0..4], block_size);
            LittleEndian::write_u32(&mut data[4..8], 0);

            let result = FileParameters::from_bytes(&data);
            assert!(
                result.is_err(),
                "Block size {} (above 256MB) should be rejected",
                block_size
            );
            match result {
                Err(Error::InvalidBlockSize(size)) => {
                    assert_eq!(size, block_size);
                }
                _ => panic!("Expected InvalidBlockSize error"),
            }
        }
    }
}
