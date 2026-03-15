//! Parent Locator metadata item
//!
//! Contains information for locating parent disk (used by differencing disks).

use crate::error::{Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use uuid::Uuid;

/// Parent Locator GUID: A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
pub const PARENT_LOCATOR_GUID: crate::common::guid::Guid =
    crate::common::guid::Guid(Uuid::from_bytes_le([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB,
        0x0C,
    ]));

/// Parent Locator Entry
#[derive(Debug, Clone)]
pub struct ParentLocatorEntry {
    pub key_offset: u32,
    pub value_offset: u32,
    pub key_length: u16,
    pub value_length: u16,
}

impl ParentLocatorEntry {
    /// Size of entry
    pub const SIZE: usize = 12;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::FileTooSmall("file size is insufficient".to_string()));
        }

        let key_offset = LittleEndian::read_u32(&data[0..4]);
        let value_offset = LittleEndian::read_u32(&data[4..8]);
        let key_length = LittleEndian::read_u16(&data[8..10]);
        let value_length = LittleEndian::read_u16(&data[10..12]);

        Ok(ParentLocatorEntry {
            key_offset,
            value_offset,
            key_length,
            value_length,
        })
    }
}

/// Parent Locator metadata item
#[derive(Debug, Clone)]
pub struct ParentLocator {
    pub key_count: u32,
    pub entries: Vec<ParentLocatorEntry>,
    pub key_values: Vec<(String, String)>, // Key-value pairs
}

impl ParentLocator {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(Error::InvalidMetadata("ParentLocator too small".to_string(),));
        }

        let key_count = LittleEndian::read_u32(&data[4..8]);

        // Parse entries
        let mut entries = Vec::with_capacity(key_count as usize);
        let entries_start = 16;

        for i in 0..key_count as usize {
            let entry_offset = entries_start + i * ParentLocatorEntry::SIZE;
            if entry_offset + ParentLocatorEntry::SIZE > data.len() {
                return Err(Error::InvalidMetadata("ParentLocator entry extends beyond data".to_string(),));
            }
            let entry = ParentLocatorEntry::from_bytes(&data[entry_offset..])?;
            entries.push(entry);
        }

        // Parse key-value strings (UTF-16 LE)
        let mut key_values = Vec::with_capacity(key_count as usize);
        for entry in &entries {
            let key = read_utf16_string(data, entry.key_offset, entry.key_length)?;
            let value = read_utf16_string(data, entry.value_offset, entry.value_length)?;
            key_values.push((key, value));
        }

        Ok(ParentLocator {
            key_count,
            entries,
            key_values,
        })
    }

    /// Get value by key
    pub fn get(&self, key: &str) -> Option<&String> {
        self.key_values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Get parent path (try relative, then absolute, then URI)
    pub fn parent_path(&self) -> Option<&String> {
        self.get("relative_path")
            .or_else(|| self.get("absolute_win32_path"))
            .or_else(|| self.get("absolute_uri"))
    }

    /// Get parent linkage GUID
    pub fn parent_linkage(&self) -> Option<&String> {
        self.get("parent_linkage")
    }

    /// Check if parent_linkage2 exists (MUST NOT exist per MS-VHDX spec Section 2.2.4)
    pub fn parent_linkage2(&self) -> Option<&String> {
        self.get("parent_linkage2")
    }
}

/// Read UTF-16 LE string from bytes
fn read_utf16_string(data: &[u8], offset: u32, length: u16) -> Result<String> {
    let start = offset as usize;
    let len = length as usize * 2; // UTF-16 is 2 bytes per char

    if start + len > data.len() {
        return Err(Error::InvalidMetadata("String extends beyond data".to_string(),));
    }

    let mut chars = Vec::with_capacity(length as usize);
    for i in (start..start + len).step_by(2) {
        if i + 1 < data.len() {
            let ch = LittleEndian::read_u16(&data[i..i + 2]);
            if ch == 0 {
                break;
            }
            chars.push(ch);
        }
    }

    String::from_utf16(&chars)
        .map_err(|_| Error::InvalidMetadata("Invalid UTF-16 string".to_string()))
}
