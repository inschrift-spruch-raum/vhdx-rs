use std::path::PathBuf;

use bitvec::prelude::*;

use crate::error::{Error, Result, SignaturePosition};
use crate::types::Guid;
pub use crate::types::StandardItems;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Metadata table fixed size: 64 KB.
const METADATA_TABLE_SIZE: usize = 64 * 1024;

/// Table header size: 32 bytes.
const TABLE_HEADER_SIZE: usize = 32;

/// Table entry size: 32 bytes.
const TABLE_ENTRY_SIZE: usize = 32;

/// Locator header size: 20 bytes.
const LOCATOR_HEADER_SIZE: usize = 20;

/// Key-value entry size: 12 bytes.
const KV_ENTRY_SIZE: usize = 12;

/// Expected metadata table signature: "metadata" in ASCII.
const SIGNATURE: &[u8; 8] = b"metadata";

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
        if data.len() < METADATA_TABLE_SIZE {
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
            data: &self.data[..METADATA_TABLE_SIZE],
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
            data: &self.data[..TABLE_HEADER_SIZE],
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
            let start = TABLE_HEADER_SIZE + i * TABLE_ENTRY_SIZE;
            TableEntry {
                data: &data[start..start + TABLE_ENTRY_SIZE],
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
        if signature != *SIGNATURE {
            return Err(Error::InvalidSignature {
                position: SignaturePosition::MetadataTable,
                expected: *SIGNATURE,
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
    pub fn flags(&self) -> EntryFlags {
        EntryFlags(self.flags_bits())
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
pub struct EntryFlags(pub u32);

impl EntryFlags {
    /// View the flags as a `BitSlice` with `Lsb0` ordering.
    fn bitslice(&self) -> &BitSlice<u32, Lsb0> {
        self.0.view_bits::<Lsb0>()
    }

    /// `IsUser` (bit 0): user metadata vs system metadata.
    #[must_use]
    pub fn is_user(&self) -> bool {
        self.bitslice()[0]
    }

    /// `IsVirtualDisk` (bit 1): virtual disk metadata vs file metadata.
    #[must_use]
    pub fn is_virtual_disk(&self) -> bool {
        self.bitslice()[1]
    }

    /// `IsRequired` (bit 2): implementation must understand this item.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.bitslice()[2]
    }

    /// Whether any reserved bits (3-31) are set.
    pub(crate) fn has_reserved_bits(self) -> bool {
        self.bitslice()[3..=31].any()
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
            data: &self.data[..LOCATOR_HEADER_SIZE.min(self.data.len())],
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
        let start = LOCATOR_HEADER_SIZE + index * KV_ENTRY_SIZE;
        let end = start + KV_ENTRY_SIZE;
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
            let start = LOCATOR_HEADER_SIZE + i * KV_ENTRY_SIZE;
            let end = start + KV_ENTRY_SIZE;
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
fn decode_utf16le(data: &[u8], offset: usize, byte_len: usize) -> Result<String> {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid metadata region for testing.
    fn build_test_metadata() -> Vec<u8> {
        let mut buf = vec![0u8; METADATA_TABLE_SIZE + 4096];

        // -- Table Header (32 bytes) --
        buf[0..8].copy_from_slice(b"metadata"); // signature
        // reserved (2 bytes): 0
        // entry_count (2 bytes): 6
        buf[10..12].copy_from_slice(&6u16.to_le_bytes());
        // reserved2 (20 bytes): 0

        let mut off = TABLE_HEADER_SIZE;

        // Helper to write an entry
        let mut write_entry = |guid: &Guid, item_offset: u32, length: u32, flags: u32| {
            buf[off..off + 16].copy_from_slice(&guid.to_bytes());
            buf[off + 16..off + 20].copy_from_slice(&item_offset.to_le_bytes());
            buf[off + 20..off + 24].copy_from_slice(&length.to_le_bytes());
            buf[off + 24..off + 28].copy_from_slice(&flags.to_le_bytes());
            // reserved (4 bytes): 0
            off += TABLE_ENTRY_SIZE;
        };

        // Entry 0: File Parameters (at offset 64KB, length 8)
        write_entry(
            &StandardItems::FILE_PARAMETERS,
            u32::try_from(METADATA_TABLE_SIZE).expect("metadata table size fits u32"),
            8,
            0x0000_0004, // is_virtual_disk=0, is_required=1 (bit 2)
        );

        // Entry 1: Virtual Disk Size (at 64KB+8, length 8)
        write_entry(
            &StandardItems::VIRTUAL_DISK_SIZE,
            u32::try_from(METADATA_TABLE_SIZE + 8).expect("metadata offset fits u32"),
            8,
            0x0000_0006, // is_virtual_disk + is_required
        );

        // Entry 2: Virtual Disk ID (at 64KB+24, length 16)
        write_entry(
            &StandardItems::VIRTUAL_DISK_ID,
            u32::try_from(METADATA_TABLE_SIZE + 24).expect("metadata offset fits u32"),
            16,
            0x0000_0006,
        );

        // Entry 3: Logical Sector Size (at 64KB+40, length 4)
        write_entry(
            &StandardItems::LOGICAL_SECTOR_SIZE,
            u32::try_from(METADATA_TABLE_SIZE + 40).expect("metadata offset fits u32"),
            4,
            0x0000_0006,
        );

        // Entry 4: Physical Sector Size (at 64KB+48, length 4)
        write_entry(
            &StandardItems::PHYSICAL_SECTOR_SIZE,
            u32::try_from(METADATA_TABLE_SIZE + 48).expect("metadata offset fits u32"),
            4,
            0x0000_0006,
        );

        // Entry 5: Parent Locator (empty, offset=0, length=0)
        write_entry(&StandardItems::PARENT_LOCATOR, 0, 0, 0x0000_0004);

        // -- Metadata Items --
        let items_base = METADATA_TABLE_SIZE;

        // File Parameters per MS-VHDX §2.6.2.1: block_size first, flags second
        let fp_block = (32 * 1024 * 1024u32).to_le_bytes();
        let fp_flags = 0u32.to_le_bytes();
        buf[items_base..items_base + 4].copy_from_slice(&fp_block);
        buf[items_base + 4..items_base + 8].copy_from_slice(&fp_flags);

        // Virtual Disk Size: 10 GB
        let disk_size = (10u64 * 1024 * 1024 * 1024).to_le_bytes();
        buf[items_base + 8..items_base + 16].copy_from_slice(&disk_size);

        // Virtual Disk ID: all zeros GUID
        // (already zeroed)

        // Logical Sector Size: 4096
        buf[items_base + 40..items_base + 44].copy_from_slice(&4096u32.to_le_bytes());

        // Physical Sector Size: 4096
        buf[items_base + 48..items_base + 52].copy_from_slice(&4096u32.to_le_bytes());

        buf
    }

    #[test]
    fn metadata_signature_valid() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        let header = meta.table().header();
        assert_eq!(header.signature(), b"metadata");
        header.validate_signature().unwrap();
    }

    #[test]
    fn metadata_entry_count() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        assert_eq!(meta.table().header().entry_count(), 6);
    }

    #[test]
    fn metadata_entries_iterator_count() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        assert_eq!(meta.table().entries().count(), 6);
    }

    #[test]
    fn metadata_entry_lookup_found() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        let entry = meta.table().entry(&StandardItems::FILE_PARAMETERS).unwrap();
        assert_eq!(
            entry.offset(),
            u32::try_from(METADATA_TABLE_SIZE).expect("metadata table size fits u32")
        );
        assert_eq!(entry.length(), 8);
    }

    #[test]
    fn metadata_entry_lookup_not_found() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        let unknown = Guid::from_bytes([0xFF; 16]);
        let result = meta.table().entry(&unknown);
        assert!(result.is_err());
    }

    #[test]
    fn file_parameters_dynamic_disk() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        let fp = meta.items().file_parameters().unwrap();
        assert_eq!(fp.block_size(), 32 * 1024 * 1024);
        assert!(!fp.leave_block_allocated());
        assert!(!fp.has_parent());
    }

    #[test]
    fn virtual_disk_size() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        assert_eq!(
            meta.items().virtual_disk_size().unwrap(),
            10 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn logical_sector_size() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        assert_eq!(meta.items().logical_sector_size().unwrap(), 4096);
    }

    #[test]
    fn physical_sector_size() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        assert_eq!(meta.items().physical_sector_size().unwrap(), 4096);
    }

    #[test]
    fn entry_flags() {
        // Per MS-VHDX §2.6.1.2 diagram: A=IsUser(bit0), B=IsVirtualDisk(bit1),
        // C=IsRequired(bit2).
        let flags = EntryFlags(0x0000_0007); // bits 0 + 1 + 2 = all three set
        assert!(flags.is_user());
        assert!(flags.is_virtual_disk());
        assert!(flags.is_required());

        let flags = EntryFlags(0x0000_0000);
        assert!(!flags.is_user());
        assert!(!flags.is_virtual_disk());
        assert!(!flags.is_required());

        // Reserved bits 3-31: detection
        let flags = EntryFlags(0x0000_0008); // bit 3 set
        assert!(flags.has_reserved_bits());

        let flags = EntryFlags(0xFFFF_FFF8); // all reserved bits set
        assert!(flags.has_reserved_bits());

        let flags = EntryFlags(0x0000_0007); // only valid bits
        assert!(!flags.has_reserved_bits());
    }

    #[test]
    fn parent_locator_empty() {
        let buf = build_test_metadata();
        let meta = Metadata::new(&buf).unwrap();
        // Parent locator has length=0, so item_data returns Some(&[])
        let locator = meta.items().parent_locator().unwrap();
        // Header data is 0 bytes - accessing it would panic, but key_value_count will be 0
        // since data.len() < LOCATOR_HEADER_SIZE
        assert_eq!(locator.entries().count(), 0);
    }

    #[test]
    fn utf16le_decoding() {
        // "hello" in UTF-16LE
        let data: Vec<u8> = "hello".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let result = decode_utf16le(&data, 0, data.len()).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn parent_locator_with_entries() {
        // Build a parent locator with one key-value pair: relative_path -> "Cargo.toml"
        // Use a real file so std::fs::metadata() succeeds in resolve_parent_path()
        let key = "relative_path";
        let value = "Cargo.toml";
        let key_utf16: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let value_utf16: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();

        // Header (20) + 1 KV entry (12) + key data + value data
        let kv_data_start = LOCATOR_HEADER_SIZE + KV_ENTRY_SIZE;
        let total_len = kv_data_start + key_utf16.len() + value_utf16.len();

        let mut buf = vec![0u8; METADATA_TABLE_SIZE + total_len];

        // Table header
        buf[0..8].copy_from_slice(b"metadata");
        buf[10..12].copy_from_slice(&1u16.to_le_bytes()); // 1 entry

        // Entry 0: Parent Locator
        let entry_off = TABLE_HEADER_SIZE;
        buf[entry_off..entry_off + 16].copy_from_slice(&StandardItems::PARENT_LOCATOR.to_bytes());
        buf[entry_off + 16..entry_off + 20].copy_from_slice(
            &u32::try_from(METADATA_TABLE_SIZE)
                .expect("metadata table size fits u32")
                .to_le_bytes(),
        );
        buf[entry_off + 20..entry_off + 24].copy_from_slice(
            &u32::try_from(total_len)
                .expect("total length fits u32")
                .to_le_bytes(),
        );
        buf[entry_off + 24..entry_off + 28].copy_from_slice(&0x0000_0004u32.to_le_bytes());

        // Locator data
        let base = METADATA_TABLE_SIZE;
        // Locator header
        buf[base..base + 16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
        buf[base + 16..base + 18].copy_from_slice(&0u16.to_le_bytes()); // reserved
        buf[base + 18..base + 20].copy_from_slice(&1u16.to_le_bytes()); // 1 kv entry

        // KV entry: key at kv_data_start, value at kv_data_start + key_len
        let kv_entry_off = base + LOCATOR_HEADER_SIZE;
        buf[kv_entry_off..kv_entry_off + 4].copy_from_slice(
            &u32::try_from(kv_data_start)
                .expect("key/value data start fits u32")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 4..kv_entry_off + 8].copy_from_slice(
            &u32::try_from(kv_data_start + key_utf16.len())
                .expect("value offset fits u32")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 8..kv_entry_off + 10].copy_from_slice(
            &u16::try_from(key_utf16.len())
                .expect("key length fits u16")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 10..kv_entry_off + 12].copy_from_slice(
            &u16::try_from(value_utf16.len())
                .expect("value length fits u16")
                .to_le_bytes(),
        );

        // Key and value data
        let key_off = base + kv_data_start;
        buf[key_off..key_off + key_utf16.len()].copy_from_slice(&key_utf16);
        let val_off = key_off + key_utf16.len();
        buf[val_off..val_off + value_utf16.len()].copy_from_slice(&value_utf16);

        // Parse
        let meta = Metadata::new(&buf).unwrap();
        let locator = meta.items().parent_locator().unwrap();

        assert_eq!(locator.header().key_value_count(), 1);

        let kv_entries: Vec<_> = locator.entries().collect();
        assert_eq!(kv_entries.len(), 1);

        let kv = &kv_entries[0];
        assert_eq!(kv.key(locator.key_value_data()).unwrap(), "relative_path");
        assert_eq!(kv.value(locator.key_value_data()).unwrap(), "Cargo.toml");

        // resolve_parent_path should return the path (Cargo.toml exists on disk)
        let path = locator.resolve_parent_path().unwrap();
        assert_eq!(path.to_str().unwrap(), "Cargo.toml");
    }

    #[test]
    fn metadata_region_too_small() {
        let buf = vec![0u8; 100];
        assert!(Metadata::new(&buf).is_err());
    }

    /// Helper: build a parent locator buffer with arbitrary key-value pairs.
    /// Each pair is (&str, &str) for (key, value).
    fn build_locator_buf(entries: &[(&str, &str)]) -> Vec<u8> {
        let count = entries.len();
        // Encode all keys and values as UTF-16LE
        let encoded: Vec<(Vec<u8>, Vec<u8>)> = entries
            .iter()
            .map(|(k, v)| {
                let ku: Vec<u8> = k.encode_utf16().flat_map(u16::to_le_bytes).collect();
                let vu: Vec<u8> = v.encode_utf16().flat_map(u16::to_le_bytes).collect();
                (ku, vu)
            })
            .collect();

        let kv_data_start = LOCATOR_HEADER_SIZE + count * KV_ENTRY_SIZE;
        let total_data_len: usize = encoded.iter().map(|(k, v)| k.len() + v.len()).sum();
        let total_len = kv_data_start + total_data_len;

        let mut buf = vec![0u8; METADATA_TABLE_SIZE + total_len];

        // Table header
        buf[0..8].copy_from_slice(b"metadata");
        buf[10..12].copy_from_slice(&1u16.to_le_bytes()); // 1 entry

        // Entry 0: Parent Locator
        let entry_off = TABLE_HEADER_SIZE;
        buf[entry_off..entry_off + 16].copy_from_slice(&StandardItems::PARENT_LOCATOR.to_bytes());
        buf[entry_off + 16..entry_off + 20].copy_from_slice(
            &u32::try_from(METADATA_TABLE_SIZE)
                .expect("metadata table size fits u32")
                .to_le_bytes(),
        );
        buf[entry_off + 20..entry_off + 24].copy_from_slice(
            &u32::try_from(total_len)
                .expect("total length fits u32")
                .to_le_bytes(),
        );
        buf[entry_off + 24..entry_off + 28].copy_from_slice(&0x0000_0004u32.to_le_bytes());

        // Locator header
        let base = METADATA_TABLE_SIZE;
        buf[base..base + 16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
        buf[base + 16..base + 18].copy_from_slice(&0u16.to_le_bytes()); // reserved
        buf[base + 18..base + 20].copy_from_slice(
            &u16::try_from(count)
                .expect("entry count fits u16")
                .to_le_bytes(),
        );

        // Write KV entries and data
        let mut data_offset = kv_data_start;
        for (i, (key_bytes, val_bytes)) in encoded.iter().enumerate() {
            let kv_entry_off = base + LOCATOR_HEADER_SIZE + i * KV_ENTRY_SIZE;
            buf[kv_entry_off..kv_entry_off + 4].copy_from_slice(
                &u32::try_from(data_offset)
                    .expect("data offset fits u32")
                    .to_le_bytes(),
            );
            buf[kv_entry_off + 4..kv_entry_off + 8].copy_from_slice(
                &u32::try_from(data_offset + key_bytes.len())
                    .expect("value offset fits u32")
                    .to_le_bytes(),
            );
            buf[kv_entry_off + 8..kv_entry_off + 10].copy_from_slice(
                &u16::try_from(key_bytes.len())
                    .expect("key length fits u16")
                    .to_le_bytes(),
            );
            buf[kv_entry_off + 10..kv_entry_off + 12].copy_from_slice(
                &u16::try_from(val_bytes.len())
                    .expect("value length fits u16")
                    .to_le_bytes(),
            );

            let koff = base + data_offset;
            buf[koff..koff + key_bytes.len()].copy_from_slice(key_bytes);
            let voff = koff + key_bytes.len();
            buf[voff..voff + val_bytes.len()].copy_from_slice(val_bytes);

            data_offset += key_bytes.len() + val_bytes.len();
        }

        buf
    }

    #[test]
    fn resolve_parent_path_accessible() {
        // relative_path points to Cargo.toml which exists
        let buf = build_locator_buf(&[("relative_path", "Cargo.toml")]);
        let meta = Metadata::new(&buf).unwrap();
        let locator = meta.items().parent_locator().unwrap();

        let path = locator.resolve_parent_path().unwrap();
        assert_eq!(path.to_str().unwrap(), "Cargo.toml");
    }

    #[test]
    fn resolve_parent_path_fallback() {
        // relative_path is nonexistent, but volume_path points to Cargo.toml
        let buf = build_locator_buf(&[
            ("relative_path", "nonexistent_file_xyz.vhdx"),
            ("volume_path", "Cargo.toml"),
        ]);
        let meta = Metadata::new(&buf).unwrap();
        let locator = meta.items().parent_locator().unwrap();

        let path = locator.resolve_parent_path().unwrap();
        assert_eq!(path.to_str().unwrap(), "Cargo.toml");
    }

    #[test]
    fn resolve_parent_path_all_inaccessible() {
        // All three keys point to nonexistent paths
        let buf = build_locator_buf(&[
            ("relative_path", "no_such_rel.vhdx"),
            ("volume_path", "no_such_vol.vhdx"),
            ("absolute_win32_path", "no_such_abs.vhdx"),
        ]);
        let meta = Metadata::new(&buf).unwrap();
        let locator = meta.items().parent_locator().unwrap();

        let err = locator.resolve_parent_path().unwrap_err();
        match &err {
            Error::ParentNotFound => {}
            _ => panic!("expected ParentNotFound, got: {err}"),
        }
    }
}
