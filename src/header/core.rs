//! Header section parser for VHDX files.
//!
//! Implements zero-copy parsing of the 1 MB header section:
//! - File Type Identifier (offset 0, 64 KB aligned)
//! - Header 1 (offset 64 KB, 4 KB structure, 64 KB aligned)
//! - Header 2 (offset 128 KB, 4 KB structure, 64 KB aligned)
//! - Region Table 1 (offset 192 KB, 64 KB)
//! - Region Table 2 (offset 256 KB, 64 KB)

use bitvec::prelude::*;

use crate::constants::{
    CREATOR_SIZE, HEADER_SIGNATURE, HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET,
    MAX_REGION_ENTRIES, REGION_ENTRY_SIZE, REGION_SIGNATURE, REGION_TABLE_SIZE,
    REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET, RT_HEADER_SIZE,
};
use crate::error::{Error, Result, SignaturePosition};
use crate::types::{Crc32c, Guid};

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
        let min_size = (REGION_TABLE2_OFFSET + REGION_TABLE_SIZE) as usize;
        if data.len() < min_size {
            return Err(Error::InvalidFile(format!(
                "header section too small: {} bytes, need at least {min_size}",
                data.len()
            )));
        }
        Ok(Self { data })
    }

    /// Return the file type identifier.
    #[must_use]
    pub fn file_type(&self) -> FileTypeIdentifier<'a> {
        FileTypeIdentifier { data: self.data }
    }

    /// Return a header structure.
    ///
    /// - `index = 0`: returns the **current** header (the one with the higher
    ///   sequence number among the two valid headers).
    /// - `index = 1`: returns Header 1 (physical offset 64 KB).
    /// - `index = 2`: returns Header 2 (physical offset 128 KB).
    ///
    /// # Errors
    ///
    /// Returns an error if the index is invalid or the requested header fails
    /// signature/checksum validation.
    pub fn header(&self, index: usize) -> Result<HeaderStructure<'a>> {
        match index {
            0 => self.current_header(),
            1 => self.validate_header_at(HEADER1_OFFSET as usize),
            2 => self.validate_header_at(HEADER2_OFFSET as usize),
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
    ///
    /// # Errors
    ///
    /// Returns an error if the index is invalid or the requested table fails
    /// signature/checksum validation.
    pub fn region_table(&self, index: usize) -> Result<RegionTable<'a>> {
        match index {
            0 => self.current_region_table(),
            1 => self.validate_region_table_at(REGION_TABLE1_OFFSET as usize),
            2 => self.validate_region_table_at(REGION_TABLE2_OFFSET as usize),
            _ => Err(Error::InvalidParameter(format!(
                "region table index must be 0, 1, or 2, got {index}"
            ))),
        }
    }

    // -- Internal helpers ---------------------------------------------------

    /// Parse and validate the header at the given byte offset.
    fn validate_header_at(&self, offset: usize) -> Result<HeaderStructure<'a>> {
        let slice = &self.data[offset..][..HEADER_SIZE as usize];

        // Check signature.
        if slice[..4].view_bits::<Lsb0>() != *HEADER_SIGNATURE {
            let mut found: [u8; 8] = [0; 8];
            found[..4].copy_from_slice(&slice[..4]);
            let mut expected: [u8; 8] = [0; 8];
            expected[..4].copy_from_slice(&HEADER_SIGNATURE.into_inner().to_le_bytes());
            return Err(Error::InvalidSignature {
                position: SignaturePosition::Header,
                expected,
                found,
            });
        }

        let stored_crc = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        let computed_crc = crate::common::crc32c_zeroed_checksum(slice);

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
        let h1 = self.validate_header_at(HEADER1_OFFSET as usize);
        let h2 = self.validate_header_at(HEADER2_OFFSET as usize);

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
        let h1 = self.validate_header_at(HEADER1_OFFSET as usize);
        let h2 = self.validate_header_at(HEADER2_OFFSET as usize);

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
        let slice = &self.data[offset..][..REGION_TABLE_SIZE as usize];

        // Check signature.
        if slice[..4].view_bits::<Lsb0>() != *REGION_SIGNATURE {
            let mut found: [u8; 8] = [0; 8];
            found[..4].copy_from_slice(&slice[..4]);
            let mut expected: [u8; 8] = [0; 8];
            expected[..4].copy_from_slice(&REGION_SIGNATURE.into_inner().to_le_bytes());
            return Err(Error::InvalidSignature {
                position: SignaturePosition::RegionTable,
                expected,
                found,
            });
        }

        let stored_crc = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        let computed_crc = crate::common::crc32c_zeroed_checksum(slice);

        if computed_crc != stored_crc {
            return Err(Error::InvalidChecksum {
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        // Validate entry count.
        let entry_count = u32::from_le_bytes(slice[8..12].try_into().unwrap());
        if entry_count > u32::from(MAX_REGION_ENTRIES) {
            return Err(Error::InvalidRegionTable(format!(
                "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM: entry count {entry_count} exceeds maximum of {MAX_REGION_ENTRIES}"
            )));
        }

        // Check that entries fit within the 64 KB table.
        let entries_start = 16; // after the 16-byte region table header
        let entries_end = entries_start + entry_count as usize * REGION_ENTRY_SIZE as usize;
        if entries_end > REGION_TABLE_SIZE as usize {
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
            1 => REGION_TABLE1_OFFSET as usize,
            2 => REGION_TABLE2_OFFSET as usize,
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
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 8] {
        self.data[..8].try_into().unwrap()
    }

    /// Return the 512-byte creator field as raw bytes (UTF-16 LE, possibly null-terminated).
    ///
    /// # Panics
    ///
    /// Panics if the header buffer is shorter than the required creator field.
    #[must_use]
    pub fn creator(&self) -> &'a [u8; 512] {
        self.data[8..8 + CREATOR_SIZE as usize].try_into().unwrap()
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
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[..4].try_into().unwrap()
    }

    /// Return the stored CRC-32C checksum.
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 8 bytes.
    #[must_use]
    pub fn checksum(&self) -> Crc32c {
        Crc32c::from_raw(u32::from_le_bytes(self.data[4..8].try_into().unwrap()))
    }

    /// Return the sequence number.
    ///
    /// The header with the higher sequence number is considered current.
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 16 bytes.
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    /// Return the file write GUID.
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 32 bytes.
    #[must_use]
    pub fn file_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[16..32].try_into().unwrap())
    }

    /// Return the data write GUID.
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 48 bytes.
    #[must_use]
    pub fn data_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[32..48].try_into().unwrap())
    }

    /// Return the log GUID.
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 64 bytes.
    #[must_use]
    pub fn log_guid(&self) -> Guid {
        Guid::from_bytes(self.data[48..64].try_into().unwrap())
    }

    /// Return the log format version.
    ///
    /// Per MS-VHDX §2.2.2, this field MUST be 0. The library enforces this
    /// requirement when a log is active (non-zero [`LogGuid`](Self::log_guid));
    /// see [`validate_header`](crate::validation::SpecValidator::validate_header).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 66 bytes.
    #[must_use]
    pub fn log_version(&self) -> u16 {
        u16::from_le_bytes(self.data[64..66].try_into().unwrap())
    }

    /// Return the VHDX format version (must be 1 per spec).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 68 bytes.
    #[must_use]
    pub fn version(&self) -> u16 {
        u16::from_le_bytes(self.data[66..68].try_into().unwrap())
    }

    /// Return the log length in bytes (must be a multiple of 1 MB).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 72 bytes.
    #[must_use]
    pub fn log_length(&self) -> u32 {
        u32::from_le_bytes(self.data[68..72].try_into().unwrap())
    }

    /// Return the log offset in the file (must be a multiple of 1 MB).
    ///
    /// # Panics
    ///
    /// Panics if the header slice is shorter than 80 bytes.
    #[must_use]
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

impl<'a> RegionTable<'a> {
    /// Return the region table header.
    #[must_use]
    pub fn header(&self) -> RegionTableHeader<'a> {
        RegionTableHeader {
            data: &self.data[..RT_HEADER_SIZE as usize],
        }
    }

    /// Return a zero-copy iterator over region table entries.
    pub fn entries(&self) -> impl Iterator<Item = RegionTableEntry<'a>> + '_ {
        let count = self.header().entry_count() as usize;
        (0..count).map(move |i| {
            let offset = RT_HEADER_SIZE as usize + i * REGION_ENTRY_SIZE as usize;
            RegionTableEntry {
                data: &self.data[offset..][..REGION_ENTRY_SIZE as usize],
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
    ///
    /// # Panics
    ///
    /// Panics if the region table header slice is shorter than 4 bytes.
    #[must_use]
    pub fn signature(&self) -> &'a [u8; 4] {
        self.data[..4].try_into().unwrap()
    }

    /// Return the stored CRC-32C checksum.
    ///
    /// # Panics
    ///
    /// Panics if the region table header slice is shorter than 8 bytes.
    #[must_use]
    pub fn checksum(&self) -> Crc32c {
        Crc32c::from_raw(u32::from_le_bytes(self.data[4..8].try_into().unwrap()))
    }

    /// Return the number of region table entries.
    ///
    /// # Panics
    ///
    /// Panics if the region table header slice is shorter than 12 bytes.
    #[must_use]
    pub fn entry_count(&self) -> u32 {
        u32::from_le_bytes(self.data[8..12].try_into().unwrap())
    }

    /// Return the reserved field.
    ///
    /// # Panics
    ///
    /// Panics if the region table header slice is shorter than 16 bytes.
    #[must_use]
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

impl RegionTableEntry<'_> {
    /// Return the region GUID (16 bytes, mixed-endian RFC 4122 layout).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 16 bytes.
    #[must_use]
    pub fn guid(&self) -> Guid {
        Guid::from_bytes(self.data[..16].try_into().unwrap())
    }

    /// Return the byte offset of the region within the file.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 24 bytes.
    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    /// Return the byte length of the region.
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 28 bytes.
    #[must_use]
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    /// Whether this region is required (bit 0 of the Required field per MS-VHDX §2.2.3).
    ///
    /// # Panics
    ///
    /// Panics if the entry slice is shorter than 32 bytes.
    #[must_use]
    pub fn required(&self) -> bool {
        self.data[28..32].view_bits::<Lsb0>()[0]
    }
}
