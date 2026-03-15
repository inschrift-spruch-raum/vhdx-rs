//! Region Table structures for VHDX
//!
//! The region table lists regions within the VHDX file.
//! There are two copies stored at offsets 192KB and 256KB.

use crate::common::crc32c::crc32c_with_zero_field;
use crate::common::guid::Guid;
use crate::error::{Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use uuid::Uuid;

/// Region Table signature: "regi"
pub const REGION_SIGNATURE: &[u8] = b"regi";

/// BAT Region GUID: 2DC27766-F623-4200-9D64-115E9BFD4A08
pub const BAT_GUID: Guid = Guid(Uuid::from_bytes_le([
    0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
]));

/// Metadata Region GUID: 8B7CA206-4790-4B9A-B8FE-575F050F886E
pub const METADATA_GUID: Guid = Guid(Uuid::from_bytes_le([
    0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
]));

/// Region Table Header
#[derive(Debug, Clone)]
pub struct RegionTableHeader {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub entry_count: u32,
}

impl RegionTableHeader {
    /// Size of header
    pub const SIZE: usize = 16;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::FileTooSmall("file size is insufficient".to_string()));
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if signature != REGION_SIGNATURE {
            return Err(Error::InvalidSignature {
                expected: String::from_utf8_lossy(REGION_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let checksum = LittleEndian::read_u32(&data[4..8]);
        let entry_count = LittleEndian::read_u32(&data[8..12]);

        // Validate entry count (max 2047)
        if entry_count > 2047 {
            return Err(Error::InvalidRegion(format!(
                "Entry count {} exceeds maximum 2047",
                entry_count
            )));
        }

        Ok(RegionTableHeader {
            signature,
            checksum,
            entry_count,
        })
    }

    /// Verify checksum
    pub fn verify_checksum(&self, data: &[u8]) -> bool {
        let calculated = crc32c_with_zero_field(data, 4, 4);
        calculated == self.checksum
    }
}

/// Region Table Entry
#[derive(Debug, Clone)]
pub struct RegionTableEntry {
    pub guid: Guid,
    pub file_offset: u64,
    pub length: u32,
    pub required: u32,
}

impl RegionTableEntry {
    /// Size of entry
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::FileTooSmall("file size is insufficient".to_string()));
        }

        let mut guid_bytes = [0u8; 16];
        guid_bytes.copy_from_slice(&data[0..16]);
        let guid = Guid::from_bytes(guid_bytes);

        let file_offset = LittleEndian::read_u64(&data[16..24]);
        let length = LittleEndian::read_u32(&data[24..28]);
        let required = LittleEndian::read_u32(&data[28..32]);

        // Validate alignment (must be 1MB aligned)
        if file_offset % (1024 * 1024) != 0 {
            return Err(Error::Alignment(file_offset, 1024 * 1024));
        }

        // Validate length (must be 1MB multiple)
        if length % (1024 * 1024) != 0 {
            return Err(Error::Alignment(length as u64, 1024 * 1024));
        }

        // Validate minimum offset (must be >= 1MB)
        if file_offset < 1024 * 1024 {
            return Err(Error::InvalidRegion(format!(
                "Region offset {} must be >= 1MB",
                file_offset
            )));
        }

        Ok(RegionTableEntry {
            guid,
            file_offset,
            length,
            required,
        })
    }

    /// Check if this is a required region
    pub fn is_required(&self) -> bool {
        self.required == 1
    }

    /// Check if this is the BAT region
    pub fn is_bat(&self) -> bool {
        self.guid == BAT_GUID
    }

    /// Check if this is the Metadata region
    pub fn is_metadata(&self) -> bool {
        self.guid == METADATA_GUID
    }
}

/// Complete Region Table
#[derive(Debug, Clone)]
pub struct RegionTable {
    pub header: RegionTableHeader,
    pub entries: Vec<RegionTableEntry>,
}

impl RegionTable {
    /// Offset of Region Table 1
    pub const OFFSET_1: u64 = 192 * 1024;
    /// Offset of Region Table 2
    pub const OFFSET_2: u64 = 256 * 1024;
    /// Size of each region table (64KB)
    pub const SIZE: usize = 64 * 1024;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::FileTooSmall("file size is insufficient".to_string()));
        }

        // Parse header
        let header = RegionTableHeader::from_bytes(data)?;

        // Verify checksum over entire 64KB
        if !header.verify_checksum(data) {
            return Err(Error::InvalidChecksum);
        }

        // Parse entries
        let mut entries = Vec::with_capacity(header.entry_count as usize);
        let entries_start = RegionTableHeader::SIZE;

        for i in 0..header.entry_count as usize {
            let entry_offset = entries_start + i * RegionTableEntry::SIZE;
            if entry_offset + RegionTableEntry::SIZE > data.len() {
                return Err(Error::InvalidRegion(
                    "Entry extends beyond table".to_string(),
                ));
            }
            let entry = RegionTableEntry::from_bytes(&data[entry_offset..])?;
            entries.push(entry);
        }

        Ok(RegionTable { header, entries })
    }

    /// Find BAT region
    pub fn find_bat(&self) -> Option<&RegionTableEntry> {
        self.entries.iter().find(|e| e.is_bat())
    }

    /// Find Metadata region
    pub fn find_metadata(&self) -> Option<&RegionTableEntry> {
        self.entries.iter().find(|e| e.is_metadata())
    }

    /// Validate that regions don't overlap
    pub fn validate_no_overlap(&self) -> Result<()> {
        for i in 0..self.entries.len() {
            for j in (i + 1)..self.entries.len() {
                let a = &self.entries[i];
                let b = &self.entries[j];

                let a_end = a.file_offset + a.length as u64;
                let b_end = b.file_offset + b.length as u64;

                if a.file_offset < b_end && b.file_offset < a_end {
                    return Err(Error::InvalidRegion(format!(
                        "Regions overlap: [{}..{}) and [{}..{})",
                        a.file_offset, a_end, b.file_offset, b_end
                    )));
                }
            }
        }
        Ok(())
    }

    /// Check for required regions that we don't recognize
    pub fn validate_known_regions(&self) -> Result<()> {
        for entry in &self.entries {
            if entry.is_required() && !entry.is_bat() && !entry.is_metadata() {
                return Err(Error::RequiredRegionNotFound(format!(
                    "Unknown required region: {}",
                    entry.guid
                )));
            }
        }
        Ok(())
    }
}

/// Read and validate both region tables
///
/// Returns (region_table, is_from_copy_1) - prefers copy 1 if both valid
pub fn read_region_tables(file: &mut std::fs::File) -> Result<(RegionTable, bool)> {
    use std::io::{Read, Seek, SeekFrom};

    // Read Region Table 1
    let mut table1_data = vec![0u8; RegionTable::SIZE];
    file.seek(SeekFrom::Start(RegionTable::OFFSET_1))?;
    match file.read_exact(&mut table1_data) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Error::FileTooSmall("file size is insufficient".to_string()));
        }
        Err(e) => return Err(e.into()),
    }

    // Read Region Table 2
    let mut table2_data = vec![0u8; RegionTable::SIZE];
    file.seek(SeekFrom::Start(RegionTable::OFFSET_2))?;
    match file.read_exact(&mut table2_data) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            // Table 2 might not exist, that's ok
            table2_data.clear();
        }
        Err(e) => return Err(e.into()),
    }

    // Try to parse both
    let table1 = RegionTable::from_bytes(&table1_data);
    let table2 = if !table2_data.is_empty() {
        RegionTable::from_bytes(&table2_data)
    } else {
        Err(Error::InvalidRegion("Table 2 not present".to_string()))
    };

    // Prefer table 1 if valid, otherwise try table 2
    match (table1, table2) {
        (Ok(t), _) => {
            t.validate_no_overlap()?;
            t.validate_known_regions()?;
            Ok((t, true))
        }
        (Err(_), Ok(t)) => {
            t.validate_no_overlap()?;
            t.validate_known_regions()?;
            Ok((t, false))
        }
        (Err(e1), Err(_)) => Err(e1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_table_entry() {
        let mut data = vec![0u8; 32];

        // Set GUID (BAT)
        data[0..16].copy_from_slice(&BAT_GUID.to_bytes());

        // Set file offset (1MB)
        LittleEndian::write_u64(&mut data[16..24], 1024 * 1024);

        // Set length (1MB)
        LittleEndian::write_u32(&mut data[24..28], 1024 * 1024);

        // Set required
        LittleEndian::write_u32(&mut data[28..32], 1);

        let entry = RegionTableEntry::from_bytes(&data).unwrap();
        assert!(entry.is_bat());
        assert!(entry.is_required());
        assert_eq!(entry.file_offset, 1024 * 1024);
        assert_eq!(entry.length, 1024 * 1024);
    }

    #[test]
    fn test_region_overlap_detection() {
        let mut sig = [0u8; 4];
        sig.copy_from_slice(REGION_SIGNATURE);
        let table = RegionTable {
            header: RegionTableHeader {
                signature: sig,
                checksum: 0,
                entry_count: 2,
            },
            entries: vec![
                RegionTableEntry {
                    guid: BAT_GUID,
                    file_offset: 1024 * 1024,
                    length: 1024 * 1024,
                    required: 1,
                },
                RegionTableEntry {
                    guid: METADATA_GUID,
                    file_offset: 1024 * 1024 + 512 * 1024, // Overlaps!
                    length: 1024 * 1024,
                    required: 1,
                },
            ],
        };

        assert!(table.validate_no_overlap().is_err());
    }
}
