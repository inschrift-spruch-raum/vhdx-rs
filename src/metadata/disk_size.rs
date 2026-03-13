//! Virtual Disk Size metadata item
//!
//! Contains the virtual size of the disk.

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

/// Virtual Disk Size GUID: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
pub const VIRTUAL_DISK_SIZE_GUID: crate::guid::Guid = crate::guid::Guid([
    0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4, 0xB8,
]);

/// Virtual Disk Size metadata item
#[derive(Debug, Clone)]
pub struct VirtualDiskSize {
    pub size: u64,
}

impl VirtualDiskSize {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::InvalidMetadata(
                "VirtualDiskSize too small".to_string(),
            ));
        }

        let size = LittleEndian::read_u64(&data[0..8]);

        if size == 0 {
            return Err(VhdxError::InvalidMetadata(
                "Virtual disk size cannot be zero".to_string(),
            ));
        }

        Ok(VirtualDiskSize { size })
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
}
