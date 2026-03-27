//! Metadata Section implementation
//!
//! The Metadata Section contains:
//! - Metadata Table (64 KB fixed): Entry directory
//! - Metadata Items (variable): `FileParameters`, `VirtualDiskSize`, etc.

use crate::common::constants::{METADATA_TABLE_SIZE, metadata_guids};
use crate::error::{Error, Result};
use crate::types::Guid;

/// Metadata Section
pub struct Metadata {
    raw_data: Vec<u8>,
}

impl Metadata {
    /// Create from raw data
    pub fn new(data: Vec<u8>) -> Result<Self> {
        if data.len() < METADATA_TABLE_SIZE {
            return Err(Error::InvalidMetadata(format!(
                "Metadata section must be at least {} bytes, got {}",
                METADATA_TABLE_SIZE,
                data.len()
            )));
        }
        Ok(Self { raw_data: data })
    }

    /// Return the complete raw bytes
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// Access the Metadata Table
    pub fn table(&self) -> MetadataTable<'_> {
        MetadataTable::new(&self.raw_data[..METADATA_TABLE_SIZE])
    }

    /// Access the Metadata Items
    pub const fn items(&self) -> MetadataItems<'_> {
        MetadataItems::new(self)
    }
}

/// Metadata Table (64 KB fixed)
pub struct MetadataTable<'a> {
    data: &'a [u8],
}

impl<'a> MetadataTable<'a> {
    /// Create from raw data
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Return raw bytes
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get the table header
    #[must_use]
    pub fn header(&self) -> TableHeader<'_> {
        TableHeader::new(&self.data[0..32])
    }

    /// Get an entry by Item ID
    #[must_use]
    pub fn entry(&self, item_id: &Guid) -> Option<TableEntry<'_>> {
        self.entries().into_iter().find(|e| e.item_id() == *item_id)
    }

    /// Get all entries
    #[must_use]
    pub fn entries(&self) -> Vec<TableEntry<'_>> {
        let count = self.header().entry_count();
        (0..count).filter_map(|i| self.entry_by_index(i)).collect()
    }

    /// Get entry by index
    fn entry_by_index(&self, index: u16) -> Option<TableEntry<'_>> {
        let header = self.header();
        if index >= header.entry_count() {
            return None;
        }
        let offset = 32 + index as usize * 32;
        if offset + 32 > self.data.len() {
            return None;
        }
        TableEntry::new(&self.data[offset..offset + 32]).ok()
    }
}

/// Table Header (32 bytes)
pub struct TableHeader<'a> {
    data: &'a [u8],
}

impl<'a> TableHeader<'a> {
    /// Create from raw data
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Return raw bytes
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get signature (should be "metadata")
    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.data[0..8]
    }

    /// Get entry count
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 12 bytes.
    #[must_use]
    pub fn entry_count(&self) -> u16 {
        u16::from_le_bytes(self.data[10..12].try_into().unwrap())
    }
}

/// Table Entry (32 bytes)
pub struct TableEntry<'a> {
    data: &'a [u8],
}

impl<'a> TableEntry<'a> {
    /// Create from raw data
    ///
    /// # Errors
    ///
    /// Returns an error if the data is not exactly 32 bytes.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != 32 {
            return Err(Error::InvalidMetadata("Entry must be 32 bytes".to_string()));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get Item ID (GUID)
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 16 bytes.
    #[must_use]
    pub fn item_id(&self) -> Guid {
        Guid::from_bytes(self.data[0..16].try_into().unwrap())
    }

    /// Get offset relative to metadata region start
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 20 bytes.
    #[must_use]
    pub fn offset(&self) -> u32 {
        u32::from_le_bytes(self.data[16..20].try_into().unwrap())
    }

    /// Get entry length
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 24 bytes.
    #[must_use]
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[20..24].try_into().unwrap())
    }

    /// Get flags
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 28 bytes.
    #[must_use]
    pub fn flags(&self) -> EntryFlags {
        EntryFlags(u32::from_le_bytes(self.data[24..28].try_into().unwrap()))
    }
}

/// Entry Flags
#[derive(Clone, Copy, Debug)]
pub struct EntryFlags(pub u32);

impl EntryFlags {
    /// Is user metadata (bit 31)
    #[must_use]
    pub const fn is_user(&self) -> bool {
        (self.0 & 0x8000_0000) != 0
    }

    /// Is virtual disk metadata (bit 30)
    #[must_use]
    pub const fn is_virtual_disk(&self) -> bool {
        (self.0 & 0x4000_0000) != 0
    }

    /// Is required (bit 29)
    #[must_use]
    pub const fn is_required(&self) -> bool {
        (self.0 & 0x2000_0000) != 0
    }
}

/// Metadata Items accessor
pub struct MetadataItems<'a> {
    metadata: &'a Metadata,
}

impl<'a> MetadataItems<'a> {
    /// Create from metadata section
    #[must_use]
    pub const fn new(metadata: &'a Metadata) -> Self {
        Self { metadata }
    }

    /// Get raw data for an item
    fn get_item_data(&self, guid: &Guid) -> Option<&'a [u8]> {
        let table = self.metadata.table();
        let entry = table.entry(guid)?;
        let offset = entry.offset() as usize;
        let length = entry.length() as usize;
        self.metadata.raw_data.get(offset..offset + length)
    }

    /// Get File Parameters
    #[must_use]
    pub fn file_parameters(&self) -> Option<FileParameters> {
        let data = self.get_item_data(&metadata_guids::FILE_PARAMETERS)?;
        if data.len() < 8 {
            return None;
        }
        Some(FileParameters::from_bytes(data))
    }

    /// Get virtual disk size
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 8 bytes.
    #[must_use]
    pub fn virtual_disk_size(&self) -> Option<u64> {
        let data = self.get_item_data(&metadata_guids::VIRTUAL_DISK_SIZE)?;
        if data.len() < 8 {
            return None;
        }
        Some(u64::from_le_bytes(data[0..8].try_into().unwrap()))
    }

    /// Get virtual disk ID
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 16 bytes.
    #[must_use]
    pub fn virtual_disk_id(&self) -> Option<Guid> {
        let data = self.get_item_data(&metadata_guids::VIRTUAL_DISK_ID)?;
        if data.len() < 16 {
            return None;
        }
        Some(Guid::from_bytes(data[0..16].try_into().unwrap()))
    }

    /// Get logical sector size
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 4 bytes.
    #[must_use]
    pub fn logical_sector_size(&self) -> Option<u32> {
        let data = self.get_item_data(&metadata_guids::LOGICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes(data[0..4].try_into().unwrap()))
    }

    /// Get physical sector size
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 4 bytes.
    #[must_use]
    pub fn physical_sector_size(&self) -> Option<u32> {
        let data = self.get_item_data(&metadata_guids::PHYSICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes(data[0..4].try_into().unwrap()))
    }

    /// Get parent locator (for differencing disks)
    #[must_use]
    pub fn parent_locator(&self) -> Option<ParentLocator<'_>> {
        let data = self.get_item_data(&metadata_guids::PARENT_LOCATOR)?;
        ParentLocator::new(data).ok()
    }
}

/// File Parameters (8 bytes)
#[derive(Clone, Copy, Debug)]
pub struct FileParameters {
    block_size: u32,
    flags: u32,
}

impl FileParameters {
    /// Parse from bytes
    ///
    /// # Panics
    ///
    /// Panics if the data is less than 8 bytes.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            block_size: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            flags: u32::from_le_bytes(data[4..8].try_into().unwrap()),
        }
    }

    /// Get block size
    #[must_use]
    pub const fn block_size(&self) -> u32 {
        self.block_size
    }

    /// Check if blocks should remain allocated (fixed disk)
    #[must_use]
    pub const fn leave_block_allocated(&self) -> bool {
        (self.flags & 0x01) != 0
    }

    /// Check if has parent (differencing disk)
    #[must_use]
    pub const fn has_parent(&self) -> bool {
        (self.flags & 0x02) != 0
    }

    /// Get raw flags
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// Parent Locator (for differencing disks)
pub struct ParentLocator<'a> {
    data: &'a [u8],
}

impl<'a> ParentLocator<'a> {
    /// Create from raw data
    ///
    /// # Errors
    ///
    /// Returns an error if the data is less than 20 bytes.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 20 {
            return Err(Error::InvalidMetadata(
                "Parent Locator too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get header
    #[must_use]
    pub fn header(&self) -> LocatorHeader<'_> {
        LocatorHeader::new(&self.data[0..20])
    }

    /// Get entry by index
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<KeyValueEntry> {
        let header = self.header();
        if index >= header.key_value_count() as usize {
            return None;
        }
        let offset = 20 + index * 12;
        if offset + 12 > self.data.len() {
            return None;
        }
        KeyValueEntry::new(&self.data[offset..offset + 12]).ok()
    }

    /// Get all entries
    #[must_use]
    pub fn entries(&self) -> Vec<KeyValueEntry> {
        let count = self.header().key_value_count();
        (0..count).filter_map(|i| self.entry(i as usize)).collect()
    }

    /// Get the key-value data region
    #[must_use]
    pub fn key_value_data(&self) -> &[u8] {
        // Key-value data starts after all entries
        let header = self.header();
        let entries_size = 20 + header.key_value_count() as usize * 12;
        if entries_size > self.data.len() {
            return &[];
        }
        &self.data[entries_size..]
    }
}

/// Locator Header (20 bytes)
pub struct LocatorHeader<'a> {
    data: &'a [u8],
}

impl<'a> LocatorHeader<'a> {
    /// Create from raw data
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Return raw bytes
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get locator type GUID
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 16 bytes (should not happen with valid metadata).
    #[must_use]
    pub fn locator_type(&self) -> Guid {
        Guid::from_bytes(self.data[0..16].try_into().unwrap())
    }

    /// Get key-value count
    ///
    /// # Panics
    ///
    /// Panics if the data slice is not at least 20 bytes (should not happen with valid metadata).
    #[must_use]
    pub fn key_value_count(&self) -> u16 {
        u16::from_le_bytes(self.data[18..20].try_into().unwrap())
    }
}

/// Key-Value Entry (12 bytes)
#[derive(Clone, Copy, Debug)]
pub struct KeyValueEntry {
    key_offset: u32,
    value_offset: u32,
    key_length: u16,
    value_length: u16,
}

impl KeyValueEntry {
    /// Create from raw data
    ///
    /// # Errors
    ///
    /// Returns an error if the data is not exactly 12 bytes.
    ///
    /// # Panics
    ///
    /// Panics if the byte slice conversion fails (should not happen with valid input).
    pub fn new(data: &[u8]) -> Result<Self> {
        if data.len() != 12 {
            return Err(Error::InvalidMetadata(
                "Key-Value Entry must be 12 bytes".to_string(),
            ));
        }
        Ok(Self {
            key_offset: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            value_offset: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            key_length: u16::from_le_bytes(data[8..10].try_into().unwrap()),
            value_length: u16::from_le_bytes(data[10..12].try_into().unwrap()),
        })
    }

    /// Get raw bytes representation
    #[must_use]
    pub fn raw(&self) -> [u8; 12] {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&self.key_offset.to_le_bytes());
        data[4..8].copy_from_slice(&self.value_offset.to_le_bytes());
        data[8..10].copy_from_slice(&self.key_length.to_le_bytes());
        data[10..12].copy_from_slice(&self.value_length.to_le_bytes());
        data
    }

    /// Get key string from key-value data
    #[must_use]
    pub fn key(&self, data: &[u8]) -> Option<String> {
        let start = self.key_offset as usize;
        let end = start + self.key_length as usize;
        let key_data = data.get(start..end)?;

        // Decode UTF-16LE
        let utf16: Vec<u16> = key_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&utf16))
    }

    /// Get value string from key-value data
    #[must_use]
    pub fn value(&self, data: &[u8]) -> Option<String> {
        let start = self.value_offset as usize;
        let end = start + self.value_length as usize;
        let value_data = data.get(start..end)?;

        // Decode UTF-16LE
        let utf16: Vec<u16> = value_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&utf16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_flags() {
        let flags = EntryFlags(0xE000_0000); // All three bits set
        assert!(flags.is_user());
        assert!(flags.is_virtual_disk());
        assert!(flags.is_required());

        let flags = EntryFlags(0);
        assert!(!flags.is_user());
        assert!(!flags.is_virtual_disk());
        assert!(!flags.is_required());
    }

    #[test]
    fn test_file_parameters() {
        let data = [0x00, 0x00, 0x00, 0x02, 0x03, 0x00, 0x00, 0x00]; // 32MB, flags=3
        let fp = FileParameters::from_bytes(&data);
        assert_eq!(fp.block_size(), 0x0200_0000);
        assert!(fp.leave_block_allocated());
        assert!(fp.has_parent());
    }

    #[test]
    fn test_key_value_entry() {
        // Create a simple key-value pair
        let mut kv_data = vec![0u8; 100];
        // Key at offset 0: "parent_linkage"
        let key = "parent_linkage";
        for (i, c) in key.encode_utf16().enumerate() {
            kv_data[i * 2..i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        // Value at offset 32: "parent.vhdx"
        let value = "parent.vhdx";
        for (i, c) in value.encode_utf16().enumerate() {
            kv_data[32 + i * 2..32 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }

        let entry = KeyValueEntry {
            key_offset: 0,
            value_offset: 32,
            key_length: u16::try_from(key.len() * 2).unwrap_or(0),
            value_length: u16::try_from(value.len() * 2).unwrap_or(0),
        };

        assert_eq!(entry.key(&kv_data).unwrap(), key);
        assert_eq!(entry.value(&kv_data).unwrap(), value);
    }
}
