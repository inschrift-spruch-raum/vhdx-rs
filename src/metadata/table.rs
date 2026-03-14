//! Metadata Table structures
//!
//! Contains the metadata table header and entry definitions.

use crate::common::guid::Guid;
use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

/// Metadata Table signature: "metadata"
pub const METADATA_SIGNATURE: &[u8] = b"metadata";

/// Metadata Table Header
#[derive(Debug, Clone)]
pub struct MetadataTableHeader {
    pub signature: [u8; 8],
    pub entry_count: u16,
}

impl MetadataTableHeader {
    /// Size of header (32 bytes per MS-VHDX spec)
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::FileTooSmall);
        }

        let mut signature = [0u8; 8];
        signature.copy_from_slice(&data[0..8]);

        if &signature != METADATA_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(METADATA_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let entry_count = LittleEndian::read_u16(&data[10..12]);

        Ok(MetadataTableHeader {
            signature,
            entry_count,
        })
    }
}

/// Metadata Table Entry
#[derive(Debug, Clone)]
pub struct MetadataTableEntry {
    pub item_id: Guid,
    pub offset: u32,
    pub length: u32,
    pub is_user: bool,
    pub is_virtual_disk: bool,
    pub is_required: bool,
}

impl MetadataTableEntry {
    /// Size of entry
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::FileTooSmall);
        }

        let mut item_id = [0u8; 16];
        item_id.copy_from_slice(&data[0..16]);
        let item_id = Guid::from_bytes(item_id);

        let offset = LittleEndian::read_u32(&data[16..20]);
        let length = LittleEndian::read_u32(&data[20..24]);
        let flags = LittleEndian::read_u32(&data[24..28]);

        let is_user = flags & 0x1 != 0;
        let is_virtual_disk = flags & 0x2 != 0;
        let is_required = flags & 0x4 != 0;

        // Validate reserved bits (bits 3-31 must be 0)
        if flags & 0xFFFFFFF8 != 0 {
            return Err(VhdxError::InvalidMetadata(
                "Reserved bits in metadata table entry flags must be 0".to_string(),
            ));
        }

        Ok(MetadataTableEntry {
            item_id,
            offset,
            length,
            is_user,
            is_virtual_disk,
            is_required,
        })
    }

    /// Check if this is a system metadata item
    pub fn is_system(&self) -> bool {
        !self.is_user
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_entry_data(flags: u32) -> [u8; 32] {
        let mut data = [0u8; 32];
        // Set dummy item_id (bytes 0-15)
        data[0..16].copy_from_slice(&[0x01; 16]);
        // Set offset (bytes 16-19)
        LittleEndian::write_u32(&mut data[16..20], 0x1000);
        // Set length (bytes 20-23)
        LittleEndian::write_u32(&mut data[20..24], 0x100);
        // Set flags (bytes 24-27)
        LittleEndian::write_u32(&mut data[24..28], flags);
        data
    }

    #[test]
    fn test_is_required_true() {
        // flags = 0x00000004 (only IsRequired set)
        let data = create_entry_data(0x00000004);
        let entry = MetadataTableEntry::from_bytes(&data).unwrap();
        assert!(entry.is_required);
        assert!(!entry.is_user);
        assert!(!entry.is_virtual_disk);
    }

    #[test]
    fn test_is_required_false() {
        // flags = 0x00000000 (all flags false)
        let data = create_entry_data(0x00000000);
        let entry = MetadataTableEntry::from_bytes(&data).unwrap();
        assert!(!entry.is_required);
        assert!(!entry.is_user);
        assert!(!entry.is_virtual_disk);
    }

    #[test]
    fn test_reserved_bits_error() {
        // flags = 0xFFFFFFF8 (bits 3-31 set) - should return error
        let data = create_entry_data(0xFFFFFFF8);
        let result = MetadataTableEntry::from_bytes(&data);
        assert!(result.is_err());
        match result.unwrap_err() {
            VhdxError::InvalidMetadata(msg) => {
                assert!(msg.contains("Reserved bits"));
            }
            _ => panic!("Expected InvalidMetadata error"),
        }
    }

    #[test]
    fn test_all_flags_combinations() {
        // Test various flag combinations
        let test_cases = [
            (0x0, false, false, false),
            (0x1, false, false, true), // is_user
            (0x2, false, true, false), // is_virtual_disk
            (0x3, false, true, true),  // is_user + is_virtual_disk
            (0x4, true, false, false), // is_required
            (0x5, true, false, true),  // is_required + is_user
            (0x6, true, true, false),  // is_required + is_virtual_disk
            (0x7, true, true, true),   // all flags set
        ];

        for (flags, expected_required, expected_virtual, expected_user) in test_cases {
            let data = create_entry_data(flags);
            let entry = MetadataTableEntry::from_bytes(&data).unwrap();
            assert_eq!(entry.is_required, expected_required, "flags={:#x}", flags);
            assert_eq!(
                entry.is_virtual_disk, expected_virtual,
                "flags={:#x}",
                flags
            );
            assert_eq!(entry.is_user, expected_user, "flags={:#x}", flags);
        }
    }

    #[test]
    fn test_high_reserved_bits_error() {
        // Test that high reserved bits (16-31) cause error
        let data = create_entry_data(0xFFFF0000);
        let result = MetadataTableEntry::from_bytes(&data);
        assert!(result.is_err());
        match result.unwrap_err() {
            VhdxError::InvalidMetadata(msg) => {
                assert!(msg.contains("Reserved bits"));
            }
            _ => panic!("Expected InvalidMetadata error"),
        }
    }
}
