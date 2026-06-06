use std::path::PathBuf;

use bitvec::prelude::*;

use crate::constants::{
    KV_ENTRY_SIZE, LOCATOR_HEADER_SIZE, METADATA_SIGNATURE, METADATA_TABLE_SIZE, TABLE_ENTRY_SIZE,
    TABLE_HEADER_SIZE,
};
use crate::error::{Error, Result, SignaturePosition};
use crate::types::Guid;
pub use crate::types::StandardItems;

// ---------------------------------------------------------------------------
// Metadata (top-level wrapper)
// ---------------------------------------------------------------------------

/// Wrapper around the entire metadata region buffer.
///
/// Layout: 64 KB metadata table followed by variable-length metadata items.
#[derive(Clone, Copy)]
pub struct Metadata<'a> {
    data: &'a [u8],
}

impl<'a> Metadata<'a> {
    /// Create a new `Metadata` view over the metadata region bytes.
    ///
    /// The buffer must be at least 64 KB (the fixed table size).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidMetadata`] if the buffer is smaller than 64 KB.
    pub(crate) fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < METADATA_TABLE_SIZE as usize {
            return Err(Error::InvalidMetadata(format!(
                "metadata region too small: {} bytes, need at least {METADATA_TABLE_SIZE}",
                data.len()
            )));
        }
        Ok(Self { data })
    }

    /// Access the 64 KB metadata table.
    #[must_use]
    pub fn table(&self) -> MetadataTable<'a> {
        MetadataTable {
            data: &self.data[..METADATA_TABLE_SIZE as usize],
        }
    }

    /// Access metadata items (region after the 64 KB table).
    #[must_use]
    pub fn items(&self) -> MetadataItems<'a> {
        MetadataItems {
            table: self.table(),
            items_data: self.data,
        }
    }
}

// ---------------------------------------------------------------------------
// MetadataTable
// ---------------------------------------------------------------------------

/// The fixed 64 KB metadata table: a 32-byte header followed by 32-byte entries.
pub struct MetadataTable<'a> {
    data: &'a [u8],
}

impl<'a> MetadataTable<'a> {
    /// Access the table header.
    #[must_use]
    pub fn header(&self) -> TableHeader<'a> {
        TableHeader {
            data: &self.data[..TABLE_HEADER_SIZE as usize],
        }
    }

    /// Look up a table entry by GUID.
    ///
    /// Returns `Err(Error::MetadataNotFound)` if no entry matches.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MetadataNotFound`] when the GUID is not present.
    pub fn entry(&self, item_id: &Guid) -> Result<TableEntry<'a>> {
        for e in self.entries() {
            if e.item_id() == *item_id {
                return Ok(e);
            }
        }
        Err(Error::MetadataNotFound { guid: *item_id })
    }

    /// Iterate over all table entries (zero-copy views).
    pub fn entries(&self) -> impl Iterator<Item = TableEntry<'a>> + 'a {
        let count = self.header().entry_count() as usize;
        let data = self.data;
        (0..count).map(move |i| {
            let start = TABLE_HEADER_SIZE as usize + i * TABLE_ENTRY_SIZE as usize;
            TableEntry {
                data: &data[start..start + TABLE_ENTRY_SIZE as usize],
            }
        })
    }
}

// ---------------------------------------------------------------------------
// TableHeader
// ---------------------------------------------------------------------------

/// 32-byte metadata table header.
pub struct TableHeader<'a> {
    data: &'a [u8],
}

impl<'a> TableHeader<'a> {
    /// Signature: 8 bytes, must be "metadata".
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 8 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 8] {
        self.data[..8]
            .try_into()
            .expect("header has 8 signature bytes")
    }

    /// Reserved: 2 bytes (must be 0).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 10 bytes.
    #[must_use]
    pub fn reserved(&self) -> &'a [u8; 2] {
        self.data[8..10]
            .try_into()
            .expect("header has 2 reserved bytes")
    }

    /// Number of table entries (must be <= 2047).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 12 bytes.
    #[must_use]
    pub fn entry_count(&self) -> u16 {
        u16::from_le_bytes(self.data[10..12].try_into().unwrap())
    }

    /// Reserved2: 20 bytes (must be 0).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 32 bytes.
    #[must_use]
    pub fn reserved2(&self) -> &'a [u8; 20] {
        self.data[12..32]
            .try_into()
            .expect("header has 20 reserved2 bytes")
    }

    /// Check that the signature matches "metadata".
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSignature`] if the signature does not match.
    pub(crate) fn validate_signature(&self) -> Result<()> {
        let signature = *self.signature();
        if signature.view_bits::<Lsb0>() != *METADATA_SIGNATURE {
            return Err(Error::InvalidSignature {
                position: SignaturePosition::MetadataTable,
                expected: METADATA_SIGNATURE.into_inner().to_le_bytes(),
                found: signature,
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TableEntry
// ---------------------------------------------------------------------------

/// 32-byte metadata table entry.
pub struct TableEntry<'a> {
    data: &'a [u8],
}

impl TableEntry<'_> {
    /// Item ID (16-byte GUID).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 16 bytes.
    #[must_use]
    pub fn item_id(&self) -> Guid {
        let bytes: [u8; 16] = self.data[..16].try_into().expect("entry has 16 guid bytes");
        Guid::from_bytes(bytes)
    }

    /// Byte offset of the metadata item (relative to start of metadata region).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 20 bytes.
    #[must_use]
    pub fn offset(&self) -> u32 {
        u32::from_le_bytes(self.data[16..20].try_into().unwrap())
    }

    /// Length of the metadata item in bytes.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 24 bytes.
    #[must_use]
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[20..24].try_into().unwrap())
    }

    /// Raw flags bits (4 bytes).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 28 bytes.
    #[must_use]
    pub fn flags_bits(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    /// Reserved field (4 bytes).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 32 bytes.
    #[must_use]
    pub fn reserved(&self) -> u32 {
        u32::from_le_bytes(self.data[28..32].try_into().unwrap())
    }

    /// Parsed flags.
    #[must_use]
    pub fn flags(&self) -> EntryFlags<'_> {
        EntryFlags {
            data: &self.data[24..28],
        }
    }
}

// ---------------------------------------------------------------------------
// EntryFlags
// ---------------------------------------------------------------------------

/// Bitfield flags for a metadata table entry.
///
/// Per MS-VHDX §2.6.1.2 diagram: A=IsUser(bit0), B=IsVirtualDisk(bit1),
/// C=IsRequired(bit2), bits 3-31 Reserved and MUST be 0.
#[derive(Clone, Copy, Debug)]
pub struct EntryFlags<'a> {
    pub(super) data: &'a [u8],
}

impl EntryFlags<'_> {
    /// Create an `EntryFlags` view from a 4-byte flags slice.
    #[cfg(test)]
    pub(crate) fn new(data: &[u8]) -> EntryFlags<'_> {
        EntryFlags { data }
    }

    /// `IsUser` (bit 0): user metadata vs system metadata.
    #[must_use]
    pub fn is_user(&self) -> bool {
        self.data.view_bits::<Lsb0>()[0]
    }

    /// `IsVirtualDisk` (bit 1): virtual disk metadata vs file metadata.
    #[must_use]
    pub fn is_virtual_disk(&self) -> bool {
        self.data.view_bits::<Lsb0>()[1]
    }

    /// `IsRequired` (bit 2): implementation must understand this item.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.data.view_bits::<Lsb0>()[2]
    }

    /// Whether any reserved bits (3-31) are set.
    pub(crate) fn has_reserved_bits(&self) -> bool {
        self.data.view_bits::<Lsb0>()[3..=31].any()
    }
}

// ---------------------------------------------------------------------------
// MetadataItems
// ---------------------------------------------------------------------------

/// Accessor for metadata items by well-known GUID.
pub struct MetadataItems<'a> {
    table: MetadataTable<'a>,
    items_data: &'a [u8],
}

impl<'a> MetadataItems<'a> {
    /// Resolve the item data slice for a given GUID.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MetadataRequiredMissing`] if the GUID is not found in
    /// the table or if the offset + length overflows or exceeds the metadata
    /// region bounds.
    fn item_data(&self, guid: &Guid) -> Result<&'a [u8]> {
        let Ok(entry) = self.table.entry(guid) else {
            return Err(Error::MetadataRequiredMissing { guid: *guid });
        };
        let offset = entry.offset() as usize;
        let length = entry.length() as usize;
        if length == 0 {
            // Present but empty
            return Ok(&[]);
        }
        let end = offset
            .checked_add(length)
            .ok_or(Error::MetadataRequiredMissing { guid: *guid })?;
        if end > self.items_data.len() {
            return Err(Error::MetadataRequiredMissing { guid: *guid });
        }
        Ok(&self.items_data[offset..end])
    }

    /// File Parameters metadata item.
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or has an invalid extent.
    pub fn file_parameters(&self) -> Result<FileParameters<'a>> {
        let data = self.item_data(&StandardItems::FILE_PARAMETERS)?;
        // FileParameters is 8 bytes; tolerate shorter (empty) items
        Ok(FileParameters { data })
    }

    /// Virtual disk size in bytes (8 bytes, little-endian u64).
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or shorter than 8 bytes.
    ///
    /// # Panics
    ///
    /// Panics only if the internal length check is violated before converting
    /// the 8-byte slice.
    pub fn virtual_disk_size(&self) -> Result<u64> {
        let data = self.item_data(&StandardItems::VIRTUAL_DISK_SIZE)?;
        if data.len() < 8 {
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::VIRTUAL_DISK_SIZE,
            });
        }
        Ok(u64::from_le_bytes(data[..8].try_into().unwrap()))
    }

    /// Virtual disk identifier (16-byte GUID).
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or shorter than 16 bytes.
    pub fn virtual_disk_id(&self) -> Result<Guid> {
        let data = self.item_data(&StandardItems::VIRTUAL_DISK_ID)?;
        if data.len() < 16 {
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::VIRTUAL_DISK_ID,
            });
        }
        let bytes: [u8; 16] =
            data[..16]
                .try_into()
                .map_err(|_| Error::MetadataRequiredMissing {
                    guid: StandardItems::VIRTUAL_DISK_ID,
                })?;
        Ok(Guid::from_bytes(bytes))
    }

    /// Logical sector size in bytes (4 bytes, little-endian u32).
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or shorter than 4 bytes.
    ///
    /// # Panics
    ///
    /// Panics only if the internal length check is violated before converting
    /// the 4-byte slice.
    pub fn logical_sector_size(&self) -> Result<u32> {
        let data = self.item_data(&StandardItems::LOGICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::LOGICAL_SECTOR_SIZE,
            });
        }
        Ok(u32::from_le_bytes(data[..4].try_into().unwrap()))
    }

    /// Physical sector size in bytes (4 bytes, little-endian u32).
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or shorter than 4 bytes.
    ///
    /// # Panics
    ///
    /// Panics only if the internal length check is violated before converting
    /// the 4-byte slice.
    pub fn physical_sector_size(&self) -> Result<u32> {
        let data = self.item_data(&StandardItems::PHYSICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::PHYSICAL_SECTOR_SIZE,
            });
        }
        Ok(u32::from_le_bytes(data[..4].try_into().unwrap()))
    }

    /// Parent locator (differencing disks).
    ///
    /// # Errors
    ///
    /// Returns an error if the item is missing or has an invalid extent.
    pub fn parent_locator(&self) -> Result<ParentLocator<'a>> {
        let data = self.item_data(&StandardItems::PARENT_LOCATOR)?;
        Ok(ParentLocator { data })
    }
}

// ---------------------------------------------------------------------------
// FileParameters
// ---------------------------------------------------------------------------

/// File Parameters metadata item (8 bytes).
///
/// Layout per MS-VHDX §2.6.2.1:
/// ```text
///  Bytes 0-3: BlockSize (u32 LE)
///  Bytes 4-7: BitFields (u32 LE)
///    Bit 0: LeaveBlockAllocated
///    Bit 1: HasParent
///    Bits 2-31: Reserved (MUST be 0)
/// ```
pub struct FileParameters<'a> {
    data: &'a [u8],
}

/// Bit index offsets within the 8-byte `FileParameters` view.
///
/// The full `data` is 8 bytes (64 bits) viewed as `Lsb0`:
/// - `[ 0..32]` → `BlockSize`          (first u32, bytes 0-3)
/// - `[32    ]` → `LeaveBlockAllocated` (bit 0 of `BitFields`, second u32)
/// - `[33    ]` → `HasParent`          (bit 1 of `BitFields`, second u32)
/// - `[34..64]` → Reserved           (bits 2-31 of `BitFields`, second u32)
const FP_BLOCK_SIZE: std::ops::Range<usize> = 0..32;
const FP_BITFIELDS: std::ops::Range<usize> = 32..64;
const FP_LEAVE_BLOCK_ALLOCATED: usize = 32;
const FP_HAS_PARENT: usize = 33;

impl FileParameters<'_> {
    /// Block size in bytes (first u32 per MS-VHDX §2.6.2.1).
    #[must_use]
    pub fn block_size(&self) -> u32 {
        if self.data.len() < 8 {
            return 0;
        }
        self.data.view_bits::<Lsb0>()[FP_BLOCK_SIZE].load_le::<u32>()
    }

    /// Raw bitfields word (second u32 per MS-VHDX §2.6.2.1).
    pub(crate) fn flags(&self) -> u32 {
        if self.data.len() < 8 {
            return 0;
        }
        self.data.view_bits::<Lsb0>()[FP_BITFIELDS].load_le::<u32>()
    }

    /// Whether blocks should remain allocated (fixed disk) — bit 0 of `BitFields`.
    #[must_use]
    pub fn leave_block_allocated(&self) -> bool {
        if self.data.len() < 8 {
            return false;
        }
        self.data.view_bits::<Lsb0>()[FP_LEAVE_BLOCK_ALLOCATED]
    }

    /// Whether this file has a parent (differencing disk) — bit 1 of `BitFields`.
    #[must_use]
    pub fn has_parent(&self) -> bool {
        if self.data.len() < 8 {
            return false;
        }
        self.data.view_bits::<Lsb0>()[FP_HAS_PARENT]
    }

    /// Whether any reserved bits (bits 2-31 of `BitFields`) are set.
    ///
    /// Per MS-VHDX §2.6.2.1, bits 2-31 MUST be 0.
    pub(crate) fn has_reserved_bits_set(&self) -> bool {
        if self.data.len() < 8 {
            return false;
        }
        self.data.view_bits::<Lsb0>()[34..64].any()
    }
}

// ---------------------------------------------------------------------------
// ParentLocator
// ---------------------------------------------------------------------------

/// Parent Locator metadata item for differencing disks.
///
/// Layout: 20-byte header + N × 12-byte key-value entry table + key/value data.
pub struct ParentLocator<'a> {
    data: &'a [u8],
}

impl<'a> ParentLocator<'a> {
    /// Access the 20-byte locator header.
    #[must_use]
    pub fn header(&self) -> LocatorHeader<'a> {
        LocatorHeader {
            data: &self.data[..(LOCATOR_HEADER_SIZE as usize).min(self.data.len())],
        }
    }

    /// Get a key-value entry by index.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of range or entry bytes are truncated.
    pub fn entry(&self, index: usize) -> Result<KeyValueEntry<'a>> {
        let count = self.header().key_value_count() as usize;
        if index >= count {
            return Err(Error::InvalidParameter(format!(
                "parent locator entry index {index} out of range (count={count})"
            )));
        }
        let start = LOCATOR_HEADER_SIZE as usize + index * KV_ENTRY_SIZE as usize;
        let end = start + KV_ENTRY_SIZE as usize;
        if end > self.data.len() {
            return Err(Error::InvalidParentLocator(
                "parent locator data too short for entries".into(),
            ));
        }
        Ok(KeyValueEntry {
            data: &self.data[start..end],
        })
    }

    /// Iterate over all key-value entries (zero-copy).
    pub fn entries(&self) -> impl Iterator<Item = KeyValueEntry<'a>> + 'a {
        let count = self.header().key_value_count() as usize;
        let data = self.data;
        (0..count).filter_map(move |i| {
            let start = LOCATOR_HEADER_SIZE as usize + i * KV_ENTRY_SIZE as usize;
            let end = start + KV_ENTRY_SIZE as usize;
            if end <= data.len() {
                Some(KeyValueEntry {
                    data: &data[start..end],
                })
            } else {
                None
            }
        })
    }

    /// The raw parent locator item data (including the 20-byte header and entry table).
    ///
    /// Offsets in [`KeyValueEntry`] are relative to the start of this data.
    #[must_use]
    pub fn key_value_data(&self) -> &'a [u8] {
        self.data
    }

    /// Resolve the parent path, trying in order:
    /// 1. `relative_path`
    /// 2. `volume_path`
    /// 3. `absolute_win32_path`
    ///
    /// Note: UTF-16LE decoding requires allocation, so this returns owned `PathBuf`.
    ///
    /// # Errors
    ///
    /// Returns an error if no usable locator key is found or no referenced path
    /// exists.
    ///
    /// May also return [`Error::InvalidParentLocator`] if key-value decoding
    /// fails, or [`Error::ParentNotFound`] if no accessible parent path is found.
    pub fn resolve_parent_path(&self) -> Result<PathBuf> {
        let keys = ["relative_path", "volume_path", "absolute_win32_path"];
        let mut attempted = (None::<PathBuf>, None::<PathBuf>, None::<PathBuf>);

        for (ki, key_str) in keys.iter().enumerate() {
            for kv in self.entries() {
                let Ok(key) = kv.key(self.data) else {
                    continue;
                };
                if key == *key_str {
                    let value = kv.value(self.data)?;
                    let path = PathBuf::from(value);
                    // Record the attempted path
                    match ki {
                        0 => attempted.0 = Some(path.clone()),
                        1 => attempted.1 = Some(path.clone()),
                        2 => attempted.2 = Some(path.clone()),
                        _ => {}
                    }
                    // Check accessibility
                    if std::fs::metadata(&path).is_ok() {
                        return Ok(path);
                    }
                    break; // Found this key, move to next key
                }
            }
        }

        Err(Error::ParentNotFound)
    }
}

// ---------------------------------------------------------------------------
// LocatorHeader
// ---------------------------------------------------------------------------

/// 20-byte parent locator header.
pub struct LocatorHeader<'a> {
    data: &'a [u8],
}

impl LocatorHeader<'_> {
    /// Locator type GUID (16 bytes).
    ///
    /// # Panics
    ///
    /// Panics only if an internal 16-byte guard is violated.
    #[must_use]
    pub fn locator_type(&self) -> Guid {
        let bytes: [u8; 16] = if self.data.len() >= 16 {
            self.data[..16].try_into().expect("16 bytes")
        } else {
            [0u8; 16]
        };
        Guid::from_bytes(bytes)
    }

    /// Reserved (2 bytes, must be 0).
    ///
    /// # Panics
    ///
    /// Panics only if an internal 2-byte guard is violated.
    #[must_use]
    pub fn reserved(&self) -> u16 {
        if self.data.len() >= 18 {
            u16::from_le_bytes(self.data[16..18].try_into().unwrap())
        } else {
            0
        }
    }

    /// Number of key-value entries.
    ///
    /// # Panics
    ///
    /// Panics only if an internal 2-byte guard is violated.
    #[must_use]
    pub fn key_value_count(&self) -> u16 {
        if self.data.len() >= 20 {
            u16::from_le_bytes(self.data[18..20].try_into().unwrap())
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// KeyValueEntry
// ---------------------------------------------------------------------------

/// 12-byte key-value entry in a parent locator.
pub struct KeyValueEntry<'a> {
    data: &'a [u8],
}

impl KeyValueEntry<'_> {
    /// Key offset within the parent locator item.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 4 bytes.
    #[must_use]
    pub fn key_offset(&self) -> u32 {
        u32::from_le_bytes(self.data[..4].try_into().unwrap())
    }

    /// Value offset within the parent locator item.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 8 bytes.
    #[must_use]
    pub fn value_offset(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Key length in bytes.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 10 bytes.
    #[must_use]
    pub fn key_length(&self) -> u16 {
        u16::from_le_bytes(self.data[8..10].try_into().unwrap())
    }

    /// Value length in bytes.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 12 bytes.
    #[must_use]
    pub fn value_length(&self) -> u16 {
        u16::from_le_bytes(self.data[10..12].try_into().unwrap())
    }

    /// Decode the key string (UTF-16LE) from the locator data.
    ///
    /// # Errors
    ///
    /// Returns an error if offset/length are invalid or UTF-16 decoding fails.
    pub fn key(&self, data: &[u8]) -> Result<String> {
        decode_utf16le(data, self.key_offset() as usize, self.key_length() as usize)
    }

    /// Decode the value string (UTF-16LE) from the locator data.
    ///
    /// # Errors
    ///
    /// Returns an error if offset/length are invalid or UTF-16 decoding fails.
    pub fn value(&self, data: &[u8]) -> Result<String> {
        decode_utf16le(
            data,
            self.value_offset() as usize,
            self.value_length() as usize,
        )
    }
}

/// Decode a UTF-16LE string from a byte slice at the given offset and byte-length.
pub(crate) fn decode_utf16le(data: &[u8], offset: usize, byte_len: usize) -> Result<String> {
    let end = offset
        .checked_add(byte_len)
        .ok_or_else(|| Error::InvalidParentLocator("key/value offset+length overflow".into()))?;
    if end > data.len() {
        return Err(Error::InvalidParentLocator(format!(
            "key/value data out of bounds: offset={offset}, len={byte_len}, data_len={}",
            data.len()
        )));
    }
    if !byte_len.is_multiple_of(2) {
        return Err(Error::InvalidParentLocator(
            "UTF-16LE string has odd byte length".into(),
        ));
    }
    let units: Vec<u16> = data[offset..end]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16(&units)
        .map_err(|e| Error::InvalidParentLocator(format!("invalid UTF-16LE string: {e}")))
}
