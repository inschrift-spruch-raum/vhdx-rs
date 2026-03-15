//! Virtual Disk ID metadata item
//!
//! Contains the unique GUID identifier for the virtual disk.

use crate::common::guid::Guid;
use crate::error::{Error, Result};
use uuid::Uuid;

/// Virtual Disk ID GUID: BECA12AB-B2E6-4523-93EF-C309E000C746
pub const VIRTUAL_DISK_ID_GUID: Guid = Guid(Uuid::from_bytes_le([
    0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7, 0x46,
]));

/// Virtual Disk ID metadata item
#[derive(Debug, Clone)]
pub struct VirtualDiskId {
    pub guid: Guid,
}

impl VirtualDiskId {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(Error::InvalidMetadata("VirtualDiskId too small".to_string(),));
        }

        let mut guid = [0u8; 16];
        guid.copy_from_slice(&data[0..16]);

        Ok(VirtualDiskId {
            guid: Guid::from_bytes(guid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_disk_id() {
        let data = vec![
            0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00,
            0xC7, 0x46,
        ];

        let disk_id = VirtualDiskId::from_bytes(&data).unwrap();
        assert_eq!(disk_id.guid, VIRTUAL_DISK_ID_GUID);
    }
}
