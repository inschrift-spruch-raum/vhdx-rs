//! Virtual Disk Size metadata item
//!
//! Contains the virtual size of the disk.

use crate::error::{Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use uuid::Uuid;

/// Virtual Disk Size GUID: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
pub const VIRTUAL_DISK_SIZE_GUID: crate::common::guid::Guid =
    crate::common::guid::Guid(Uuid::from_bytes_le([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4,
        0xB8,
    ]));

/// Maximum disk size per MS-VHDX specification Section 2.6.2.3: 64 TB
const MAX_DISK_SIZE: u64 = 64 * 1024 * 1024 * 1024 * 1024; // 64TB

/// Virtual Disk Size metadata item
#[derive(Debug, Clone)]
pub struct VirtualDiskSize {
    pub size: u64,
}

impl VirtualDiskSize {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(Error::InvalidMetadata("VirtualDiskSize too small".to_string(),));
        }

        let size = LittleEndian::read_u64(&data[0..8]);

        if size == 0 {
            return Err(Error::InvalidMetadata("Virtual disk size cannot be zero".to_string(),));
        }

        Ok(VirtualDiskSize { size })
    }

    /// Validate disk size per MS-VHDX specification Section 2.6.2.3
    ///
    /// Requirements:
    /// 1. Size must be >= logical sector size (minimum valid size)
    /// 2. Size must be <= 64TB (maximum per spec)
    /// 3. Size must be a multiple of logical sector size (sector-aligned)
    pub fn validate(&self, logical_sector_size: u32) -> Result<()> {
        let sector_size = logical_sector_size as u64;

        // Check minimum: must be at least one sector
        if self.size < sector_size {
            return Err(Error::InvalidDiskSize {
                size: self.size,
                min: sector_size,
                max: MAX_DISK_SIZE,
            });
        }

        // Check maximum: must not exceed 64TB
        if self.size > MAX_DISK_SIZE {
            return Err(Error::InvalidDiskSize {
                size: self.size,
                min: sector_size,
                max: MAX_DISK_SIZE,
            });
        }

        // Check alignment: must be multiple of logical sector size
        if !self.size.is_multiple_of(sector_size) {
            return Err(Error::InvalidDiskSize {
                size: self.size,
                min: sector_size,
                max: MAX_DISK_SIZE,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_disk_size() {
        let mut data = vec![0u8; 8];
        LittleEndian::write_u64(&mut data[0..8], 10 * 1024 * 1024 * 1024); // 10GB

        let size = VirtualDiskSize::from_bytes(&data).unwrap();
        assert_eq!(size.size, 10 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_virtual_disk_size_zero_fails() {
        let data = vec![0u8; 8];

        assert!(VirtualDiskSize::from_bytes(&data).is_err());
    }

    #[test]
    fn test_valid_disk_size_64tb() {
        // 64TB should pass validation
        let disk_size = VirtualDiskSize {
            size: MAX_DISK_SIZE,
        };
        let result = disk_size.validate(512);
        assert!(
            result.is_ok(),
            "64TB should pass validation with 512-byte sectors"
        );
    }

    #[test]
    fn test_invalid_disk_size_above_max() {
        // 64TB + 1 byte should be rejected
        let disk_size = VirtualDiskSize {
            size: MAX_DISK_SIZE + 1,
        };
        let result = disk_size.validate(512);
        assert!(result.is_err(), "64TB+1 should be rejected");
        match result.unwrap_err() {
            Error::InvalidDiskSize { size, min, max } => {
                assert_eq!(size, MAX_DISK_SIZE + 1);
                assert_eq!(min, 512);
                assert_eq!(max, MAX_DISK_SIZE);
            }
            e => panic!("Expected InvalidDiskSize error, got {:?}", e),
        }
    }

    #[test]
    fn test_valid_disk_size_minimum_512() {
        // Minimum size (one 512-byte sector) should pass
        let disk_size = VirtualDiskSize { size: 512 };
        let result = disk_size.validate(512);
        assert!(
            result.is_ok(),
            "512 bytes should pass validation with 512-byte sectors"
        );
    }

    #[test]
    fn test_valid_disk_size_minimum_4096() {
        // Minimum size (one 4096-byte sector) should pass with 4096-byte sectors
        let disk_size = VirtualDiskSize { size: 4096 };
        let result = disk_size.validate(4096);
        assert!(
            result.is_ok(),
            "4096 bytes should pass validation with 4096-byte sectors"
        );
    }

    #[test]
    fn test_invalid_disk_size_below_min() {
        // Size smaller than sector size should be rejected
        let disk_size = VirtualDiskSize { size: 256 };
        let result = disk_size.validate(512);
        assert!(
            result.is_err(),
            "256 bytes should be rejected with 512-byte sectors"
        );
        match result.unwrap_err() {
            Error::InvalidDiskSize { size, min, .. } => {
                assert_eq!(size, 256);
                assert_eq!(min, 512);
            }
            e => panic!("Expected InvalidDiskSize error, got {:?}", e),
        }
    }

    #[test]
    fn test_invalid_disk_size_unaligned_512() {
        // Unaligned size (not multiple of 512) should be rejected
        let disk_size = VirtualDiskSize { size: 1000 }; // 1000 is not divisible by 512
        let result = disk_size.validate(512);
        assert!(
            result.is_err(),
            "1000 bytes should be rejected (not 512-aligned)"
        );
    }

    #[test]
    fn test_invalid_disk_size_unaligned_4096() {
        // Unaligned size (not multiple of 4096) should be rejected
        let disk_size = VirtualDiskSize { size: 5000 }; // 5000 is not divisible by 4096
        let result = disk_size.validate(4096);
        assert!(
            result.is_err(),
            "5000 bytes should be rejected (not 4096-aligned)"
        );
    }

    #[test]
    fn test_valid_disk_size_aligned() {
        // Aligned size should pass
        let disk_size = VirtualDiskSize { size: 1024 * 1024 }; // 1MB, aligned to 512
        let result = disk_size.validate(512);
        assert!(result.is_ok(), "1MB should pass validation (512-aligned)");
    }

    #[test]
    fn test_invalid_disk_size_zero_validation() {
        // Zero size should fail validation
        let disk_size = VirtualDiskSize { size: 0 };
        let result = disk_size.validate(512);
        assert!(result.is_err(), "0 bytes should be rejected");
    }

    #[test]
    fn test_valid_disk_size_1gb() {
        // 1GB should pass validation
        let disk_size = VirtualDiskSize {
            size: 1024 * 1024 * 1024,
        };
        let result = disk_size.validate(512);
        assert!(result.is_ok(), "1GB should pass validation");
    }
}
