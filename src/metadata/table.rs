//! Metadata Table structures
//!
//! Contains the metadata table header and entry definitions.

use crate::error::{Result, VhdxError};
use crate::guid::Guid;
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

        Ok(MetadataTableEntry {
            item_id,
            offset,
            length,
            is_user,
            is_virtual_disk,
        })
    }

    /// Check if this is a system metadata item
    pub fn is_system(&self) -> bool {
        !self.is_user
    }
}
