//! Sector Size metadata items
//!
//! Contains logical and physical sector size information.

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};
use uuid::Uuid;

/// Logical Sector Size GUID: 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
pub const LOGICAL_SECTOR_SIZE_GUID: crate::common::guid::Guid =
    crate::common::guid::Guid(Uuid::from_bytes_le([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB,
        0x5F,
    ]));

/// Physical Sector Size GUID: CDA348C7-445D-4471-9CC9-E9885251C556
pub const PHYSICAL_SECTOR_SIZE_GUID: crate::common::guid::Guid =
    crate::common::guid::Guid(Uuid::from_bytes_le([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5,
        0x56,
    ]));

/// Sector Size metadata item
#[derive(Debug, Clone)]
pub struct SectorSize {
    pub size: u32,
}

/// Logical Sector Size type alias
pub type LogicalSectorSize = SectorSize;

/// Physical Sector Size type alias
pub type PhysicalSectorSize = SectorSize;

impl SectorSize {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(VhdxError::InvalidMetadata(
                "SectorSize too small".to_string(),
            ));
        }

        let size = LittleEndian::read_u32(&data[0..4]);

        // Only 512 or 4096 are valid
        if size != 512 && size != 4096 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Invalid sector size: {}. Must be 512 or 4096",
                size
            )));
        }

        Ok(SectorSize { size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_size_512() {
        let mut data = vec![0u8; 4];
        LittleEndian::write_u32(&mut data[0..4], 512);

        let sector = SectorSize::from_bytes(&data).unwrap();
        assert_eq!(sector.size, 512);
    }

    #[test]
    fn test_sector_size_4096() {
        let mut data = vec![0u8; 4];
        LittleEndian::write_u32(&mut data[0..4], 4096);

        let sector = SectorSize::from_bytes(&data).unwrap();
        assert_eq!(sector.size, 4096);
    }

    #[test]
    fn test_sector_size_invalid() {
        let mut data = vec![0u8; 4];
        LittleEndian::write_u32(&mut data[0..4], 1024); // Invalid size

        assert!(SectorSize::from_bytes(&data).is_err());
    }
}
