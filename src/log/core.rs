//! VHDX Log Section Parser
//!
//! Implements zero-copy parsing of the VHDX log ring buffer.
//! The log consists of variable-sized entries (4KB aligned), each containing
//! a header, descriptors, and data sectors.

use std::borrow::Cow;
use std::fmt;
use std::sync::OnceLock;

use crate::constants::{
    DESCRIPTOR_SIZE, ENTRY_HEADER_SIZE, SECTOR_SIZE, SIGNATURE_DESC, SIGNATURE_LOGE, SIGNATURE_ZERO,
};
use crate::error::{Error, Result, SignaturePosition};
use crate::types::{Crc32c, Guid};

// ---------------------------------------------------------------------------
// Log layout
// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

/// View into the VHDX log ring buffer.
///
/// The log is a circular buffer stored contiguously at a location specified
/// in the VHDX header. It consists of variable-sized entries that are at
/// least 4 KB aligned.
#[derive(Clone, Copy)]
pub struct Log<'a> {
    data: &'a [u8],
}

impl<'a> Log<'a> {
    /// Create a new `Log` view over the raw log buffer.
    ///
    /// The buffer length must be a multiple of 4 KB (MB-aligned on disk,
    /// but we just need 4 KB alignment for entry parsing).
    ///
    /// # Errors
    ///
    /// Returns `Error::LogEntryCorrupted` if the buffer size is not a
    /// multiple of 4 KB.
    pub(crate) fn new(data: &'a [u8]) -> Result<Self> {
        if !data.len().is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Error::LogEntryCorrupted(
                "log buffer size is not a multiple of 4KB".into(),
            ));
        }
        Ok(Self { data })
    }

    /// Return the total size of the log buffer in bytes.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    /// Return `true` if the log buffer is empty.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a log entry by index (0-based, scanning from the start of the buffer).
    ///
    /// Entries are located by walking the buffer: entry 0 starts at offset 0,
    /// entry 1 starts at `entry0.header().entry_length()` bytes from the start, etc.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of bounds or an entry is malformed.
    pub fn entry(&self, index: usize) -> Result<Entry<'_>> {
        let mut offset: usize = 0;
        for i in 0..=index {
            if offset >= self.data.len() {
                return Err(Error::InvalidParameter(format!(
                    "log entry index {index} out of bounds"
                )));
            }
            if i == index {
                return self.parse_entry_at(offset);
            }
            // Skip this entry by reading its length from the header
            let entry_length = u32_at(&self.data[offset + 8..offset + 12]).ok_or_else(|| {
                Error::LogEntryCorrupted("log buffer too small for entry header".into())
            })?;
            if entry_length == 0 || !(entry_length as usize).is_multiple_of(SECTOR_SIZE as usize) {
                return Err(Error::LogEntryCorrupted(format!(
                    "invalid entry length {entry_length} at index {i}"
                )));
            }
            offset += entry_length as usize;
        }
        Err(Error::InvalidParameter(format!(
            "log entry index {index} not found"
        )))
    }

    /// Parse a log entry at a specific byte offset within the log buffer.
    ///
    /// The offset must be 4KB-aligned and the entry must be fully contained
    /// within the buffer.
    ///
    /// # Errors
    ///
    /// Returns errors from [`parse_entry_at`](Self::parse_entry_at):
    /// `Error::InvalidSignature` if the entry signature is not `"loge"`,
    /// `Error::LogEntryCorrupted` if the entry length is invalid or the
    /// entry extends beyond the buffer.
    pub(crate) fn entry_at(&self, offset: usize) -> Result<Entry<'_>> {
        self.parse_entry_at(offset)
    }

    /// Iterate over all valid log entries in the buffer.
    ///
    /// Scans entries sequentially from the beginning. Stops when the buffer
    /// is exhausted or an invalid entry is encountered.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> + '_ {
        LogEntryIter {
            log: self,
            offset: 0,
            done: false,
        }
    }

    /// Parse an entry starting at `offset` within the log buffer.
    fn parse_entry_at(&self, offset: usize) -> Result<Entry<'_>> {
        if offset + ENTRY_HEADER_SIZE as usize > self.data.len() {
            return Err(Error::LogEntryCorrupted(
                "insufficient data for log entry header".into(),
            ));
        }
        let entry_data = &self.data[offset..];

        // Validate signature
        let sig = &entry_data[0..4];
        if sig != SIGNATURE_LOGE.into_inner().to_le_bytes() {
            let mut found = [0u8; 4];
            found.copy_from_slice(sig);
            return Err(Error::InvalidSignature {
                position: SignaturePosition::LogEntry,
                expected: crate::error::pad_signature_4to8(
                    SIGNATURE_LOGE.into_inner().to_le_bytes(),
                ),
                found: crate::error::pad_signature_4to8(found),
            });
        }

        // Read entry_length and validate
        let entry_length = u32_at(&entry_data[8..12])
            .ok_or_else(|| Error::LogEntryCorrupted("entry_length read failed".into()))?;
        if entry_length == 0 || !(entry_length as usize).is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Error::LogEntryCorrupted(format!(
                "entry length {entry_length} is not a multiple of 4KB"
            )));
        }
        let total = entry_length as usize;
        if offset + total > self.data.len() {
            return Err(Error::LogEntryCorrupted(format!(
                "entry extends beyond log buffer (offset={offset}, length={total}, buf={})",
                self.data.len()
            )));
        }

        Ok(Entry {
            data: &entry_data[..total],
            assembled_sectors: OnceLock::new(),
        })
    }
}

/// Iterator over log entries in a sequential scan.
struct LogEntryIter<'a> {
    log: &'a Log<'a>,
    offset: usize,
    done: bool,
}

impl<'a> Iterator for LogEntryIter<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.offset >= self.log.data.len() {
            return None;
        }
        // Check if there's room for at least a header
        if self.offset + ENTRY_HEADER_SIZE as usize > self.log.data.len() {
            self.done = true;
            return None;
        }
        let remaining = &self.log.data[self.offset..];
        // Check signature — if not "loge", stop
        if remaining[0..4] != SIGNATURE_LOGE.into_inner().to_le_bytes() {
            self.done = true;
            return None;
        }
        if let Ok(entry) = self.log.parse_entry_at(self.offset) {
            let entry_length = entry.header().entry_length() as usize;
            self.offset += entry_length;
            Some(entry)
        } else {
            self.done = true;
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

/// A single log entry, containing a header, descriptors, and data sectors.
#[derive(Debug)]
pub struct Entry<'a> {
    /// The full entry data (header + descriptor sectors + data sectors).
    data: &'a [u8],
    /// Lazily initialized per-sector `OnceLock` cells. Each cell holds the full
    /// reassembled 4096-byte sector on first access: `LeadingBytes(8) + Middle(4084) + TrailingBytes(4)`.
    assembled_sectors: OnceLock<Vec<OnceLock<[u8; SECTOR_SIZE as usize]>>>,
}

impl<'a> Entry<'a> {
    /// Return the entry header (first 64 bytes).
    pub fn header(&self) -> LogEntryHeader<'_> {
        LogEntryHeader {
            data: &self.data[..ENTRY_HEADER_SIZE as usize],
        }
    }

    /// Get a descriptor by index (0-based).
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of range or the descriptor is malformed.
    pub fn descriptor(&self, index: usize) -> Result<Descriptor<'_>> {
        let desc_count = self.header().descriptor_count() as usize;
        if index >= desc_count {
            return Err(Error::InvalidParameter(format!(
                "descriptor index {index} out of range (count={desc_count})"
            )));
        }
        let raw = self.descriptor_bytes(index)?;
        let sig = &raw[0..4];
        if sig == SIGNATURE_DESC.into_inner().to_le_bytes() {
            Ok(Descriptor::Data(DataDescriptor { data: raw }))
        } else if sig == SIGNATURE_ZERO.into_inner().to_le_bytes() {
            Ok(Descriptor::Zero(ZeroDescriptor { data: raw }))
        } else {
            let mut found = [0u8; 4];
            found.copy_from_slice(sig);
            Err(Error::LogEntryCorrupted(format!(
                "LOG_DESCRIPTOR_SIGNATURE_INVALID: unknown signature {found:?} at descriptor {index}"
            )))
        }
    }

    /// Iterate over all descriptors in this entry.
    ///
    /// Each item is validated; signature errors produce `Err`.
    pub fn descriptors(&self) -> impl Iterator<Item = Result<Descriptor<'_>>> + '_ {
        let count = self.header().descriptor_count() as usize;
        (0..count).map(|i| self.descriptor(i))
    }

    /// Iterate over data sectors in this entry.
    ///
    /// The number of data sectors equals the number of data descriptors.
    /// Data sectors start after all descriptor sectors.
    ///
    /// On first call, a Vec of empty `OnceLock` cells is initialized (one per
    /// data descriptor). Each sector is assembled individually on first access
    /// via `DataSector::data()`.
    pub fn data(&self) -> impl Iterator<Item = DataSector<'_>> + '_ {
        // Initialize the Vec of empty OnceLock cells — one per DATA descriptor
        let caches = self.assembled_sectors.get_or_init(|| {
            let desc_count = self.header().descriptor_count() as usize;
            let mut data_count = 0;
            for di in 0..desc_count {
                if let Ok(Descriptor::Data(_)) = self.descriptor(di) {
                    data_count += 1;
                }
            }
            (0..data_count).map(|_| OnceLock::new()).collect()
        });

        let desc_count = self.header().descriptor_count() as usize;
        // Number of descriptor sectors: first sector holds 64-byte header + 126 descriptors,
        // subsequent sectors hold 128 descriptors each.
        let desc_sectors = if desc_count == 0 {
            1 // first sector still exists with header only
        } else {
            let after_first = desc_count.saturating_sub(126);
            1 + after_first.div_ceil(128)
        };
        let data_offset = desc_sectors * SECTOR_SIZE as usize;
        let entry_length = self.data.len();
        let raw_data = self.data;

        // Build list of (descriptor_index,) for data descriptors only
        let mut data_indices: Vec<usize> = Vec::new();
        for di in 0..desc_count {
            if let Ok(Descriptor::Data(_)) = self.descriptor(di) {
                data_indices.push(di);
            }
        }

        data_indices
            .into_iter()
            .enumerate()
            .filter_map(move |(sector_idx, di)| {
                let sector_start = data_offset + sector_idx * SECTOR_SIZE as usize;
                if sector_start + SECTOR_SIZE as usize > entry_length {
                    return None;
                }
                let Ok(Descriptor::Data(desc)) = self.descriptor(di) else {
                    return None;
                };
                Some(DataSector {
                    data: &raw_data[sector_start..sector_start + SECTOR_SIZE as usize],
                    leading_bytes: desc.leading_bytes_raw(),
                    trailing_bytes: desc.trailing_bytes_raw(),
                    cache: &caches[sector_idx],
                })
            })
    }

    /// Validate the CRC-32C checksum of this entry.
    ///
    /// The checksum covers the entire entry (per `EntryLength`), with the
    /// Checksum field (bytes 4..8) set to zero during computation.
    ///
    /// This method avoids allocation by zeroing the checksum field in place
    /// (previously used `self.data.to_vec()`).
    ///
    /// # Errors
    ///
    /// Returns `Error::InvalidChecksum` if the computed CRC-32C does not
    /// match the stored checksum.
    pub(crate) fn verify_checksum(&self) -> Result<()> {
        let stored = self.header().checksum();
        let computed = Crc32c::from_raw(crate::common::crc32c_zeroed_checksum(self.data));

        if computed != stored {
            return Err(Error::InvalidChecksum {
                expected: stored.value(),
                actual: computed.value(),
            });
        }
        Ok(())
    }

    /// Get the raw bytes for descriptor at the given index.
    ///
    /// Descriptors are laid out starting at byte 64 (after the header).
    /// First sector: bytes 64..4096 → 126 descriptors (32 bytes each).
    /// Subsequent sectors: full 128 descriptors each.
    fn descriptor_bytes(&self, index: usize) -> Result<&'a [u8]> {
        let abs_offset = if index < 126 {
            // First descriptor sector: header (64) + index * 32
            ENTRY_HEADER_SIZE as usize + index * DESCRIPTOR_SIZE as usize
        } else {
            // Subsequent descriptor sectors
            let remaining = index - 126;
            let sector_index = remaining / 128;
            let within_sector = remaining % 128;
            SECTOR_SIZE as usize * (1 + sector_index) + within_sector * DESCRIPTOR_SIZE as usize
        };
        if abs_offset + DESCRIPTOR_SIZE as usize > self.data.len() {
            return Err(Error::LogEntryCorrupted(format!(
                "descriptor {index} at offset {abs_offset} extends beyond entry"
            )));
        }
        Ok(&self.data[abs_offset..abs_offset + DESCRIPTOR_SIZE as usize])
    }
}

// ---------------------------------------------------------------------------
// LogEntryHeader
// ---------------------------------------------------------------------------

/// Log entry header (64 bytes).
///
/// Layout (MS-VHDX §2.3.1.1):
/// ```text
/// [0..4]   Signature ("loge")
/// [4..8]   Checksum (CRC-32C)
/// [8..12]  EntryLength
/// [12..16] Tail
/// [16..24] SequenceNumber
/// [24..28] DescriptorCount
/// [28..32] Reserved
/// [32..48] LogGuid
/// [48..56] FlushedFileOffset
/// [56..64] LastFileOffset
/// ```
pub struct LogEntryHeader<'a> {
    data: &'a [u8],
}

impl<'a> LogEntryHeader<'a> {
    /// Entry signature. MUST be `"loge"` (0x65676F6C).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[0..4].try_into().expect("header is 64 bytes")
    }

    /// CRC-32C checksum computed over the entire entry (checksum field zeroed).
    #[must_use]
    pub fn checksum(&self) -> Crc32c {
        Crc32c::from_raw(u32_at(&self.data[4..8]).unwrap_or(0))
    }

    /// Total length of the entry in bytes. MUST be a multiple of 4KB.
    #[must_use]
    pub fn entry_length(&self) -> u32 {
        u32_at(&self.data[8..12]).unwrap_or(0)
    }

    /// Offset from log start to the first entry of the active sequence.
    /// MUST be a multiple of 4KB.
    #[must_use]
    pub fn tail(&self) -> u32 {
        u32_at(&self.data[12..16]).unwrap_or(0)
    }

    /// Monotonically increasing sequence number. MUST be > 0.
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64_at(&self.data[16..24]).unwrap_or(0)
    }

    /// Number of descriptors in this entry.
    #[must_use]
    pub fn descriptor_count(&self) -> u32 {
        u32_at(&self.data[24..28]).unwrap_or(0)
    }

    /// Reserved. MUST be 0.
    #[must_use]
    pub fn reserved(&self) -> u32 {
        u32_at(&self.data[28..32]).unwrap_or(0)
    }

    /// `LogGuid` that was present in the file header when this entry was written.
    #[must_use]
    pub fn log_guid(&self) -> Guid {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&self.data[32..48]);
        Guid::from_bytes(bytes)
    }

    /// VHDX file size guaranteed to be stable on disk.
    #[must_use]
    pub fn flushed_file_offset(&self) -> u64 {
        u64_at(&self.data[48..56]).unwrap_or(0)
    }

    /// File size that all allocated structures fit into.
    #[must_use]
    pub fn last_file_offset(&self) -> u64 {
        u64_at(&self.data[56..64]).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Descriptor enum
// ---------------------------------------------------------------------------

/// A log entry descriptor — either a data or zero descriptor.
///
/// The variant is determined by the 4-byte signature:
/// - `"desc"` → `Data(DataDescriptor)`
/// - `"zero"` → `Zero(ZeroDescriptor)`
/// - Any other signature is treated as corruption.
pub enum Descriptor<'a> {
    Data(DataDescriptor<'a>),
    Zero(ZeroDescriptor<'a>),
}

impl Descriptor<'_> {
    /// Return the sequence number from the descriptor.
    #[must_use]
    pub(crate) fn sequence_number(&self) -> u64 {
        match self {
            Descriptor::Data(d) => d.sequence_number(),
            Descriptor::Zero(z) => z.sequence_number(),
        }
    }
}

impl fmt::Debug for Descriptor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Descriptor::Data(d) => f
                .debug_struct("Descriptor::Data")
                .field("file_offset", &d.file_offset())
                .field("sequence_number", &d.sequence_number())
                .finish(),
            Descriptor::Zero(z) => f
                .debug_struct("Descriptor::Zero")
                .field("file_offset", &z.file_offset())
                .field("zero_length", &z.zero_length())
                .field("sequence_number", &z.sequence_number())
                .finish(),
        }
    }
}

// ---------------------------------------------------------------------------
// DataDescriptor
// ---------------------------------------------------------------------------

/// Data descriptor (32 bytes).
///
/// Layout (MS-VHDX §2.3.1.3):
/// ```text
/// [0..4]   DataSignature ("desc")
/// [4..8]   TrailingBytes
/// [8..16]  LeadingBytes
/// [16..24] FileOffset
/// [24..32] SequenceNumber
/// ```
pub struct DataDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> DataDescriptor<'a> {
    /// Signature. MUST be `"desc"` (0x63736564).
    ///
    /// # Panics
    ///
    /// Panics if the descriptor slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[0..4].try_into().expect("descriptor is 32 bytes")
    }

    /// Trailing 4 bytes removed from the 4KB sector update.
    #[must_use]
    pub fn trailing_bytes(&self) -> u32 {
        u32_at(&self.data[4..8]).unwrap_or(0)
    }

    /// Leading 8 bytes removed from the 4KB sector update.
    #[must_use]
    pub fn leading_bytes(&self) -> u64 {
        u64_at(&self.data[8..16]).unwrap_or(0)
    }

    /// File offset where the data must be written. MUST be 4KB aligned.
    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64_at(&self.data[16..24]).unwrap_or(0)
    }

    /// MUST match the entry header's `SequenceNumber`.
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64_at(&self.data[24..32]).unwrap_or(0)
    }

    /// Return the leading bytes as a raw slice (8 bytes).
    #[must_use]
    pub(crate) fn leading_bytes_raw(&self) -> &'a [u8] {
        &self.data[8..16]
    }

    /// Return the trailing bytes as a raw slice (4 bytes).
    #[must_use]
    pub(crate) fn trailing_bytes_raw(&self) -> &'a [u8] {
        &self.data[4..8]
    }
}

// ---------------------------------------------------------------------------
// ZeroDescriptor
// ---------------------------------------------------------------------------

/// Zero descriptor (32 bytes).
///
/// Layout (MS-VHDX §2.3.1.2):
/// ```text
/// [0..4]   ZeroSignature ("zero")
/// [4..8]   Reserved
/// [8..16]  ZeroLength
/// [16..24] FileOffset
/// [24..32] SequenceNumber
/// ```
pub struct ZeroDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> ZeroDescriptor<'a> {
    /// Signature. MUST be `"zero"` (0x6F72657A).
    ///
    /// # Panics
    ///
    /// Panics if the descriptor slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[0..4].try_into().expect("descriptor is 32 bytes")
    }

    /// Reserved. MUST be 0.
    #[must_use]
    pub fn reserved(&self) -> u32 {
        u32_at(&self.data[4..8]).unwrap_or(0)
    }

    /// Length of the section to zero. MUST be 4KB aligned.
    #[must_use]
    pub fn zero_length(&self) -> u64 {
        u64_at(&self.data[8..16]).unwrap_or(0)
    }

    /// File offset to zero. MUST be 4KB aligned.
    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64_at(&self.data[16..24]).unwrap_or(0)
    }

    /// MUST match the entry header's `SequenceNumber`.
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64_at(&self.data[24..32]).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// DataSector
// ---------------------------------------------------------------------------

/// Data sector (4096 bytes).
///
/// Layout (MS-VHDX §2.3.1.4):
/// ```text
/// [0..4]    DataSignature ("data")
/// [4..8]    SequenceHigh (high 4 bytes of SequenceNumber)
/// [8..4092] Data (4084 bytes — middle portion of original sector)
/// [4092..4096] SequenceLow (low 4 bytes of SequenceNumber)
/// ```
pub struct DataSector<'a> {
    /// Raw sector bytes in the log buffer (4096 bytes).
    pub(super) data: &'a [u8],
    /// Leading 8 bytes from the data descriptor.
    leading_bytes: &'a [u8],
    /// Trailing 4 bytes from the data descriptor.
    trailing_bytes: &'a [u8],
    /// Per-sector lazy cache for the assembled 4096-byte sector.
    cache: &'a OnceLock<[u8; SECTOR_SIZE as usize]>,
}

impl<'a> DataSector<'a> {
    /// Signature. MUST be `"data"` (0x61746164).
    ///
    /// # Panics
    ///
    /// Panics if the data sector slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[0..4]
            .try_into()
            .expect("data sector is 4096 bytes")
    }

    /// The reconstructed full 64-bit sequence number.
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        let high = u32::from_le_bytes(self.data[4..8].try_into().unwrap_or([0; 4]));
        let low = u32::from_le_bytes(self.data[4092..4096].try_into().unwrap_or([0; 4]));
        (u64::from(high) << 32) | u64::from(low)
    }

    /// Return the assembled full 4096-byte sector.
    ///
    /// The assembled data is: `LeadingBytes(8B) + middle(4084B) + TrailingBytes(4B)`.
    /// Lazily assembled on first access via per-sector `OnceLock` cache.
    #[must_use]
    pub fn data(&self) -> Cow<'a, [u8]> {
        let assembled = self.cache.get_or_init(|| {
            let mut buf = [0u8; SECTOR_SIZE as usize];
            buf[0..8].copy_from_slice(&self.leading_bytes[..8]);
            buf[8..4092].copy_from_slice(&self.data[8..4092]);
            buf[4092..4096].copy_from_slice(&self.trailing_bytes[..4]);
            buf
        });
        Cow::Borrowed(assembled)
    }
}

// ---------------------------------------------------------------------------
// DataSectorAssembly — helper for assembling full 4096-byte sectors
// ---------------------------------------------------------------------------

/// An assembled data sector containing the full 4096 bytes.
///
/// Created by pairing a `DataSector` with a `DataDescriptor` to reconstruct
/// the original sector: `LeadingBytes(8) + Data(4084) + TrailingBytes(4)`.
///
/// This type is kept `pub(crate)` for cross-validation in tests only.
/// Normal code should use `DataSector::data()` which returns a borrowed
/// view into the Entry's lazily-assembled buffer.
#[cfg(test)]
pub(crate) struct DataSectorAssembly {
    buf: [u8; SECTOR_SIZE as usize],
}

#[cfg(test)]
impl DataSectorAssembly {
    /// Assemble a full 4096-byte sector from a data sector and its descriptor.
    ///
    /// The assembled data is: `LeadingBytes(8B) + DataSector middle(4084B) + TrailingBytes(4B)`.
    pub fn new(descriptor: &DataDescriptor<'_>, sector: &DataSector<'_>) -> Self {
        let mut buf = [0u8; SECTOR_SIZE as usize];
        // Leading 8 bytes
        buf[0..8].copy_from_slice(descriptor.leading_bytes_raw());
        // Middle 4084 bytes
        buf[8..4092].copy_from_slice(&sector.data[8..4092]);
        // Trailing 4 bytes
        buf[4092..4096].copy_from_slice(descriptor.trailing_bytes_raw());
        Self { buf }
    }

    /// Return the assembled 4096-byte sector.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.buf
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a little-endian u32 from a 4-byte slice.
fn u32_at(buf: &[u8]) -> Option<u32> {
    if buf.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes(buf[..4].try_into().unwrap()))
}

/// Read a little-endian u64 from an 8-byte slice.
fn u64_at(buf: &[u8]) -> Option<u64> {
    if buf.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes(buf[..8].try_into().unwrap()))
}
