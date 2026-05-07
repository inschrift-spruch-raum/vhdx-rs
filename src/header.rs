//! Header section parser for VHDX files.
//!
//! Implements zero-copy parsing of the 1 MB header section:
//! - File Type Identifier (offset 0, 64 KB aligned)
//! - Header 1 (offset 64 KB, 4 KB structure, 64 KB aligned)
//! - Header 2 (offset 128 KB, 4 KB structure, 64 KB aligned)
//! - Region Table 1 (offset 192 KB, 64 KB)
//! - Region Table 2 (offset 256 KB, 64 KB)

use bitvec::prelude::*;

use crate::common::crc32c;
use crate::error::{Error, Result, SignaturePosition};
use crate::types::Guid;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const KB: usize = 1024;
const _MB: usize = 1024 * KB;

/// Offset of Header 1 within the header section.
const HEADER1_OFFSET: usize = 64 * KB;
/// Offset of Header 2 within the header section.
const HEADER2_OFFSET: usize = 128 * KB;
/// Offset of Region Table 1 within the header section.
const REGION_TABLE1_OFFSET: usize = 192 * KB;
/// Offset of Region Table 2 within the header section.
const REGION_TABLE2_OFFSET: usize = 256 * KB;

/// Size of each VHDX header structure (4 KB).
const HEADER_SIZE: usize = 4 * KB;
/// Size of each region table (64 KB).
const REGION_TABLE_SIZE: usize = 64 * KB;

/// Expected header signature: "head" (0x68656164).
const HEADER_SIGNATURE: [u8; 4] = [b'h', b'e', b'a', b'd'];
/// Expected region table signature: "regi" (0x72656769).
const REGION_SIGNATURE: [u8; 4] = [b'r', b'e', b'g', b'i'];

/// Size of each region table entry (32 bytes).
const REGION_ENTRY_SIZE: usize = 32;
/// Maximum number of region table entries per spec.
const MAX_REGION_ENTRIES: u32 = 2047;

/// Creator field size in the file type identifier.
const CREATOR_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// Header (top-level section view)
// ---------------------------------------------------------------------------

/// View over the 1 MB header section of a VHDX file.
///
/// Borrows a slice of the file's header buffer and provides validated access
/// to the file type identifier, headers, and region tables.
#[derive(Clone, Copy)]
pub struct Header<'a> {
    data: &'a [u8],
}

impl<'a> Header<'a> {
    /// Create a new `Header` view over the given buffer.
    ///
    /// The buffer must be at least 320 KB (covering through Region Table 2).
    /// In practice it should be the full 1 MB header section.
    pub(crate) fn new(data: &'a [u8]) -> Result<Self> {
        let min_size = REGION_TABLE2_OFFSET + REGION_TABLE_SIZE;
        if data.len() < min_size {
            return Err(Error::InvalidFile(format!(
                "header section too small: {} bytes, need at least {min_size}",
                data.len()
            )));
        }
        Ok(Self { data })
    }

    /// Return the file type identifier.
    pub fn file_type(&self) -> FileTypeIdentifier<'a> {
        FileTypeIdentifier { data: self.data }
    }

    /// Return a header structure.
    ///
    /// - `index = 0`: returns the **current** header (the one with the higher
    ///   sequence number among the two valid headers).
    /// - `index = 1`: returns Header 1 (physical offset 64 KB).
    /// - `index = 2`: returns Header 2 (physical offset 128 KB).
    pub fn header(&self, index: usize) -> Result<HeaderStructure<'a>> {
        match index {
            0 => self.current_header(),
            1 => self.validate_header_at(HEADER1_OFFSET),
            2 => self.validate_header_at(HEADER2_OFFSET),
            _ => Err(Error::InvalidParameter(format!(
                "header index must be 0, 1, or 2, got {index}"
            ))),
        }
    }

    /// Return a region table.
    ///
    /// - `index = 0`: returns the region table corresponding to the current header.
    /// - `index = 1`: returns Region Table 1 (physical offset 192 KB).
    /// - `index = 2`: returns Region Table 2 (physical offset 256 KB).
    pub fn region_table(&self, index: usize) -> Result<RegionTable<'a>> {
        match index {
            0 => self.current_region_table(),
            1 => self.validate_region_table_at(REGION_TABLE1_OFFSET),
            2 => self.validate_region_table_at(REGION_TABLE2_OFFSET),
            _ => Err(Error::InvalidParameter(format!(
                "region table index must be 0, 1, or 2, got {index}"
            ))),
        }
    }

    // -- Internal helpers ---------------------------------------------------

    /// Parse and validate the header at the given byte offset.
    fn validate_header_at(&self, offset: usize) -> Result<HeaderStructure<'a>> {
        let slice = &self.data[offset..][..HEADER_SIZE];

        // Check signature.
        if slice[..4] != HEADER_SIGNATURE {
            let mut found: [u8; 8] = [0; 8];
            found[..4].copy_from_slice(&slice[..4]);
            let mut expected: [u8; 8] = [0; 8];
            expected[..4].copy_from_slice(&HEADER_SIGNATURE);
            return Err(Error::InvalidSignature {
                position: SignaturePosition::Header,
                expected,
                found,
            });
        }

        // Verify CRC-32C: checksum is over the full 4 KB with the checksum
        // field (bytes 4..8) zeroed out. Compute in-place on the original
        // slice to avoid a 4 KB stack allocation.
        let stored_crc = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        let saved_checksum: [u8; 4] = slice[4..8].try_into().unwrap();
        // SAFETY: We temporarily zero the checksum field (bytes 4..8) to
        // compute the CRC-32C, then immediately restore the original bytes.
        // crc32c() is a pure read-only computation that cannot panic. The
        // modification is to raw u8 bytes which have no invalid states.
        let ptr = slice.as_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(ptr.add(4), 0, 4);
        }
        let computed_crc = crc32c(slice);
        unsafe {
            std::ptr::copy_nonoverlapping(saved_checksum.as_ptr(), ptr.add(4), 4);
        }

        if computed_crc != stored_crc {
            return Err(Error::InvalidChecksum {
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        Ok(HeaderStructure { data: slice })
    }

    /// Determine the current header by comparing sequence numbers.
    fn current_header(&self) -> Result<HeaderStructure<'a>> {
        let h1 = self.validate_header_at(HEADER1_OFFSET);
        let h2 = self.validate_header_at(HEADER2_OFFSET);

        match (h1, h2) {
            (Ok(h1), Ok(h2)) => {
                if h1.sequence_number() == h2.sequence_number() {
                    return Err(Error::HeaderSequenceNumberInvalid {
                        sequence_number_1: h1.sequence_number(),
                        sequence_number_2: h2.sequence_number(),
                    });
                }
                if h1.sequence_number() > h2.sequence_number() {
                    Ok(h1)
                } else {
                    Ok(h2)
                }
            }
            (Ok(h1), Err(_)) => Ok(h1),
            (Err(_), Ok(h2)) => Ok(h2),
            (Err(e1), Err(_)) => Err(Error::CorruptedHeader(format!(
                "both headers are invalid: {e1}"
            ))),
        }
    }

    /// Return the index (1 or 2) of the current header.
    fn current_header_index(&self) -> Result<usize> {
        let h1 = self.validate_header_at(HEADER1_OFFSET);
        let h2 = self.validate_header_at(HEADER2_OFFSET);

        match (h1, h2) {
            (Ok(h1), Ok(h2)) => {
                if h1.sequence_number() == h2.sequence_number() {
                    return Err(Error::HeaderSequenceNumberInvalid {
                        sequence_number_1: h1.sequence_number(),
                        sequence_number_2: h2.sequence_number(),
                    });
                }
                if h1.sequence_number() > h2.sequence_number() {
                    Ok(1)
                } else {
                    Ok(2)
                }
            }
            (Ok(_), Err(_)) => Ok(1),
            (Err(_), Ok(_)) => Ok(2),
            (Err(e1), Err(_)) => Err(Error::CorruptedHeader(format!(
                "both headers are invalid: {e1}"
            ))),
        }
    }

    /// Parse and validate a region table at the given byte offset.
    fn validate_region_table_at(&self, offset: usize) -> Result<RegionTable<'a>> {
        let slice = &self.data[offset..][..REGION_TABLE_SIZE];

        // Check signature.
        if slice[..4] != REGION_SIGNATURE {
            let mut found: [u8; 8] = [0; 8];
            found[..4].copy_from_slice(&slice[..4]);
            let mut expected: [u8; 8] = [0; 8];
            expected[..4].copy_from_slice(&REGION_SIGNATURE);
            return Err(Error::InvalidSignature {
                position: SignaturePosition::RegionTable,
                expected,
                found,
            });
        }

        // Verify CRC-32C: checksum is over the full 64 KB with the checksum
        // field (bytes 4..8) zeroed out. Compute in-place on the original
        // slice to avoid a 64 KB stack allocation.
        let stored_crc = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        let saved_checksum: [u8; 4] = slice[4..8].try_into().unwrap();
        // SAFETY: Same pattern as validate_header_at: temporarily zero
        // bytes 4..8 for CRC computation, then restore immediately.
        let ptr = slice.as_ptr() as *mut u8;
        unsafe {
            std::ptr::write_bytes(ptr.add(4), 0, 4);
        }
        let computed_crc = crc32c(slice);
        unsafe {
            std::ptr::copy_nonoverlapping(saved_checksum.as_ptr(), ptr.add(4), 4);
        }

        if computed_crc != stored_crc {
            return Err(Error::InvalidChecksum {
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        // Validate entry count.
        let entry_count = u32::from_le_bytes(slice[8..12].try_into().unwrap());
        if entry_count > MAX_REGION_ENTRIES {
            return Err(Error::InvalidRegionTable(format!(
                "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM: entry count {entry_count} exceeds maximum of {MAX_REGION_ENTRIES}"
            )));
        }

        // Check that entries fit within the 64 KB table.
        let entries_start = 16; // after the 16-byte region table header
        let entries_end = entries_start + entry_count as usize * REGION_ENTRY_SIZE;
        if entries_end > REGION_TABLE_SIZE {
            return Err(Error::InvalidRegionTable(format!(
                "entry count {entry_count} overflows region table"
            )));
        }

        Ok(RegionTable { data: slice })
    }

    /// Return the region table corresponding to the current header.
    ///
    /// Per spec, region table N corresponds to header N. The current header
    /// determines the current region table.
    fn current_region_table(&self) -> Result<RegionTable<'a>> {
        let idx = self.current_header_index()?;
        let offset = match idx {
            1 => REGION_TABLE1_OFFSET,
            2 => REGION_TABLE2_OFFSET,
            _ => unreachable!(),
        };
        self.validate_region_table_at(offset)
    }
}

// ---------------------------------------------------------------------------
// FileTypeIdentifier
// ---------------------------------------------------------------------------

/// View over the file type identifier (first 64 KB of the VHDX file).
///
/// Contains the 8-byte "vhdxfile" signature and a 512-byte UTF-16 creator string.
pub struct FileTypeIdentifier<'a> {
    data: &'a [u8],
}

impl<'a> FileTypeIdentifier<'a> {
    /// Return the 8-byte VHDX file signature ("vhdxfile").
    ///
    /// # Panics
    ///
    /// Cannot panic — the data slice is guaranteed to be at least 320 KB.
    pub fn signature(&self) -> &'a [u8; 8] {
        self.data[..8].try_into().unwrap()
    }

    /// Return the 512-byte creator field as raw bytes (UTF-16 LE, possibly null-terminated).
    pub fn creator(&self) -> &'a [u8; 512] {
        self.data[8..8 + CREATOR_SIZE].try_into().unwrap()
    }
}

// ---------------------------------------------------------------------------
// HeaderStructure
// ---------------------------------------------------------------------------

/// View over a single 4 KB VHDX header structure.
///
/// Fields are parsed on demand from the underlying byte slice.
pub struct HeaderStructure<'a> {
    data: &'a [u8],
}

impl<'a> HeaderStructure<'a> {
    /// Return the 4-byte header signature ("head").
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[..4].try_into().unwrap()
    }

    /// Return the stored CRC-32C checksum.
    pub fn checksum(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Return the sequence number.
    ///
    /// The header with the higher sequence number is considered current.
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    /// Return the file write GUID.
    pub fn file_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[16..32].try_into().unwrap())
    }

    /// Return the data write GUID.
    pub fn data_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[32..48].try_into().unwrap())
    }

    /// Return the log GUID.
    pub fn log_guid(&self) -> Guid {
        Guid::from_bytes(self.data[48..64].try_into().unwrap())
    }

    /// Return the log format version (must be 0 per spec).
    pub fn log_version(&self) -> u16 {
        u16::from_le_bytes(self.data[64..66].try_into().unwrap())
    }

    /// Return the VHDX format version (must be 1 per spec).
    pub fn version(&self) -> u16 {
        u16::from_le_bytes(self.data[66..68].try_into().unwrap())
    }

    /// Return the log length in bytes (must be a multiple of 1 MB).
    pub fn log_length(&self) -> u32 {
        u32::from_le_bytes(self.data[68..72].try_into().unwrap())
    }

    /// Return the log offset in the file (must be a multiple of 1 MB).
    pub fn log_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[72..80].try_into().unwrap())
    }
}

// ---------------------------------------------------------------------------
// RegionTable
// ---------------------------------------------------------------------------

/// View over a 64 KB region table.
///
/// Provides access to the region table header and a zero-copy iterator over entries.
pub struct RegionTable<'a> {
    data: &'a [u8],
}

/// Region table header is 16 bytes: signature(4) + checksum(4) + entry_count(4) + reserved(4).
const RT_HEADER_SIZE: usize = 16;

impl<'a> RegionTable<'a> {
    /// Return the region table header.
    pub fn header(&self) -> RegionTableHeader<'a> {
        RegionTableHeader {
            data: &self.data[..RT_HEADER_SIZE],
        }
    }

    /// Return a zero-copy iterator over region table entries.
    pub fn entries(&self) -> impl Iterator<Item = RegionTableEntry<'a>> + '_ {
        let count = self.header().entry_count() as usize;
        (0..count).map(move |i| {
            let offset = RT_HEADER_SIZE + i * REGION_ENTRY_SIZE;
            RegionTableEntry {
                data: &self.data[offset..][..REGION_ENTRY_SIZE],
            }
        })
    }
}

// ---------------------------------------------------------------------------
// RegionTableHeader
// ---------------------------------------------------------------------------

/// View over the 16-byte region table header.
pub struct RegionTableHeader<'a> {
    data: &'a [u8],
}

impl<'a> RegionTableHeader<'a> {
    /// Return the 4-byte signature ("regi").
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[..4].try_into().unwrap()
    }

    /// Return the stored CRC-32C checksum.
    pub fn checksum(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Return the number of region table entries.
    pub fn entry_count(&self) -> u32 {
        u32::from_le_bytes(self.data[8..12].try_into().unwrap())
    }

    /// Return the reserved field.
    pub fn reserved(&self) -> u32 {
        u32::from_le_bytes(self.data[12..16].try_into().unwrap())
    }
}

// ---------------------------------------------------------------------------
// RegionTableEntry
// ---------------------------------------------------------------------------

/// View over a single 32-byte region table entry.
pub struct RegionTableEntry<'a> {
    data: &'a [u8],
}

impl<'a> RegionTableEntry<'a> {
    /// Return the region GUID (16 bytes, mixed-endian RFC 4122 layout).
    pub fn guid(&self) -> Guid {
        Guid::from_bytes(self.data[..16].try_into().unwrap())
    }

    /// Return the byte offset of the region within the file.
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    /// Return the byte length of the region.
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    /// Whether this region is required (bit 0 of the Required field per MS-VHDX §2.2.3).
    pub fn required(&self) -> bool {
        self.data[28..32].view_bits::<Lsb0>()[0]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid 320 KB header section for testing.
    fn build_test_header_section() -> Vec<u8> {
        let mut buf = vec![0u8; REGION_TABLE2_OFFSET + REGION_TABLE_SIZE];

        // File type identifier
        buf[0..8].copy_from_slice(b"vhdxfile");

        // Header 1 at 64 KB (sequence_number = 5)
        write_header(&mut buf, HEADER1_OFFSET, 5);

        // Header 2 at 128 KB (sequence_number = 3)
        write_header(&mut buf, HEADER2_OFFSET, 3);

        // Region table 1 at 192 KB (2 entries)
        write_region_table(&mut buf, REGION_TABLE1_OFFSET, 2);

        // Region table 2 at 256 KB (2 entries)
        write_region_table(&mut buf, REGION_TABLE2_OFFSET, 2);

        buf
    }

    fn write_header(buf: &mut [u8], offset: usize, seq: u64) {
        let slice = &mut buf[offset..][..HEADER_SIZE];
        slice[..4].copy_from_slice(b"head");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
        slice[8..16].copy_from_slice(&seq.to_le_bytes());
        // file_write_guid, data_write_guid, log_guid: 3 x 16 zero bytes (offsets 16..64)
        // log_version at 64, version at 66
        slice[64..66].copy_from_slice(&0u16.to_le_bytes()); // log_version
        slice[66..68].copy_from_slice(&1u16.to_le_bytes()); // version
        slice[68..72].copy_from_slice(&(1024u32 * 1024).to_le_bytes()); // log_length
        slice[72..80].copy_from_slice(&(1024u64 * 1024).to_le_bytes()); // log_offset

        // Compute CRC-32C over the entire 4 KB with checksum zeroed.
        let checksum = crc32c(slice);
        slice[4..8].copy_from_slice(&checksum.to_le_bytes());
    }

    fn write_region_table(buf: &mut [u8], offset: usize, entry_count: u32) {
        let slice = &mut buf[offset..][..REGION_TABLE_SIZE];
        slice[..4].copy_from_slice(b"regi");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
        slice[8..12].copy_from_slice(&entry_count.to_le_bytes());
        slice[12..16].copy_from_slice(&0u32.to_le_bytes()); // reserved

        // Write entry_count entries starting at byte 16
        let entries_start = offset + RT_HEADER_SIZE;
        for i in 0..entry_count as usize {
            let eoff = entries_start + i * REGION_ENTRY_SIZE;
            // guid (16 bytes of incrementing pattern)
            buf[eoff..eoff + 16].copy_from_slice(&[i as u8; 16]);
            // file_offset
            buf[eoff + 16..eoff + 24]
                .copy_from_slice(&((1024 * 1024 * (i as u64 + 2)).to_le_bytes()));
            // length
            buf[eoff + 24..eoff + 28]
                .copy_from_slice(&(1024u32 * 1024).to_le_bytes());
            // required (bit 0 set)
            buf[eoff + 28..eoff + 32].view_bits_mut::<Lsb0>().set(0, true);
        }

        // Compute CRC-32C over the full 64 KB with checksum zeroed.
        let slice = &mut buf[offset..][..REGION_TABLE_SIZE];
        let checksum = crc32c(slice);
        slice[4..8].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn file_type_signature() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let ft = header.file_type();
        assert_eq!(ft.signature(), b"vhdxfile");
    }

    #[test]
    fn file_type_creator_is_512_bytes() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let creator = header.file_type().creator();
        assert_eq!(creator.len(), 512);
    }

    #[test]
    fn header1_valid() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let h = header.header(1).unwrap();
        assert_eq!(h.signature(), b"head");
        assert_eq!(h.sequence_number(), 5);
    }

    #[test]
    fn header2_valid() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let h = header.header(2).unwrap();
        assert_eq!(h.signature(), b"head");
        assert_eq!(h.sequence_number(), 3);
    }

    #[test]
    fn current_header_picks_higher_sequence() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let h = header.header(0).unwrap();
        // Header 1 has seq=5, Header 2 has seq=3 → header 1 is current
        assert_eq!(h.sequence_number(), 5);
    }

    #[test]
    fn current_header_picks_only_valid() {
        let mut buf = build_test_header_section();
        // Corrupt header 1 by overwriting signature
        buf[HEADER1_OFFSET] = 0xFF;
        let header = Header::new(&buf).unwrap();
        let h = header.header(0).unwrap();
        assert_eq!(h.sequence_number(), 3); // falls back to header 2
    }

    #[test]
    fn both_headers_corrupt_fails() {
        let mut buf = build_test_header_section();
        buf[HEADER1_OFFSET] = 0xFF;
        buf[HEADER2_OFFSET] = 0xFF;
        let header = Header::new(&buf).unwrap();
        let result = header.header(0);
        assert!(result.is_err());
    }

    #[test]
    fn header_index_out_of_range() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        assert!(header.header(3).is_err());
    }

    #[test]
    fn header_crc_validated() {
        let mut buf = build_test_header_section();
        // Corrupt the CRC of header 1
        buf[HEADER1_OFFSET + 4] ^= 0xFF;
        let header = Header::new(&buf).unwrap();
        assert!(header.header(1).is_err());
    }

    #[test]
    fn header_fields() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let h = header.header(1).unwrap();
        assert_eq!(h.log_version(), 0);
        assert_eq!(h.version(), 1);
        assert_eq!(h.log_length(), 1024 * 1024);
        assert_eq!(h.log_offset(), 1024 * 1024);
    }

    #[test]
    fn region_table1_valid() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(1).unwrap();
        assert_eq!(rt.header().signature(), b"regi");
        assert_eq!(rt.header().entry_count(), 2);
    }

    #[test]
    fn region_table2_valid() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(2).unwrap();
        assert_eq!(rt.header().signature(), b"regi");
        assert_eq!(rt.header().entry_count(), 2);
    }

    #[test]
    fn current_region_table_follows_current_header() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(0).unwrap();
        // Current header is header 1 (seq=5 > 3) → current region table is RT 1
        assert_eq!(rt.header().entry_count(), 2);
    }

    #[test]
    fn region_table_index_out_of_range() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        assert!(header.region_table(3).is_err());
    }

    #[test]
    fn region_table_crc_validated() {
        let mut buf = build_test_header_section();
        buf[REGION_TABLE1_OFFSET + 4] ^= 0xFF;
        let header = Header::new(&buf).unwrap();
        assert!(header.region_table(1).is_err());
    }

    #[test]
    fn region_table_entries_iterator() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(1).unwrap();
        let entries: Vec<_> = rt.entries().collect();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].required());
        assert!(entries[1].required());
    }

    #[test]
    fn region_table_entry_fields() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(1).unwrap();
        let first = rt.entries().next().unwrap();
        assert_eq!(first.file_offset(), 2 * 1024 * 1024);
        assert_eq!(first.length(), 1024 * 1024);
    }

    #[test]
    fn buffer_too_small_fails() {
        let buf = vec![0u8; 100];
        assert!(Header::new(&buf).is_err());
    }

    #[test]
    fn region_table_header_reserved() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(1).unwrap();
        assert_eq!(rt.header().reserved(), 0);
    }

    #[test]
    fn region_table_header_checksum() {
        let buf = build_test_header_section();
        let header = Header::new(&buf).unwrap();
        let rt = header.region_table(1).unwrap();
        let stored = rt.header().checksum();
        // Re-verify manually
        let slice = &buf[REGION_TABLE1_OFFSET..][..REGION_TABLE_SIZE];
        let mut tmp = slice.to_vec();
        tmp[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(stored, crc32c(&tmp));
    }

    #[test]
    fn current_header_picks_first_when_equal_sequence() {
        let mut buf = build_test_header_section();
        // Both headers have seq=5: header 1 already has seq=5, set header 2 to seq=5
        let h2_offset = HEADER2_OFFSET;
        buf[h2_offset + 8..h2_offset + 16].copy_from_slice(&5u64.to_le_bytes());
        // Recompute CRC for header 2
        buf[h2_offset + 4..h2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let slice = &buf[h2_offset..h2_offset + HEADER_SIZE];
        let checksum = crc32c(slice);
        buf[h2_offset + 4..h2_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        let header = Header::new(&buf).unwrap();
        let result = header.header(0);
        assert!(
            matches!(result, Err(Error::HeaderSequenceNumberInvalid { .. })),
            "equal sequence numbers should be rejected"
        );
    }
}
