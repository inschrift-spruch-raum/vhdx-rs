//! Log Section implementation
//!
//! The Log is a circular buffer for crash recovery.
//! Each log entry contains:
//! - Entry Header (64 bytes)
//! - Zero or more Descriptors
//! - Data sectors (for DataDescriptors)

use crate::common::constants::*;
use crate::error::{Error, Result};
use crate::types::Guid;

/// Log Section
pub struct Log {
    raw_data: Vec<u8>,
}

impl Log {
    /// Create from raw data
    pub fn new(data: Vec<u8>) -> Self {
        Self { raw_data: data }
    }

    /// Return the complete raw bytes
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// Get a log entry by index
    pub fn entry(&self, _index: usize) -> Option<LogEntry<'_>> {
        // Log entries are complex to parse as they have variable sizes
        // This is a simplified implementation
        None
    }

    /// Get all valid log entries
    pub fn entries(&self) -> Vec<LogEntry<'_>> {
        // Parse the log buffer to find all valid entries
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + LOG_ENTRY_HEADER_SIZE <= self.raw_data.len() {
            if let Ok(entry) = self.try_parse_entry_at(offset) {
                let entry_len = entry.header().entry_length() as usize;
                entries.push(entry);
                offset += entry_len;
            } else {
                // Move to next 4KB boundary
                offset += DATA_SECTOR_SIZE;
            }
        }

        entries
    }

    /// Check if log replay is required (log is non-empty)
    pub fn is_replay_required(&self) -> bool {
        !self.entries().is_empty()
    }

    /// Replay log entries to recover from crash
    ///
    /// Per MS-VHDX spec section 2.3.3: "If the log is non-empty when the VHDX file is opened,
    /// the implementation MUST replay the log before performing any I/O"
    ///
    /// # Arguments
    /// * `file` - The underlying file to apply log entries to
    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        let entries = self.entries();
        if entries.is_empty() {
            return Ok(()); // Nothing to replay
        }

        for entry in entries {
            let header = entry.header();

            // Validate signature
            if header.signature() != LOG_ENTRY_SIGNATURE {
                return Err(Error::LogEntryCorrupted(
                    "Invalid log entry signature".to_string(),
                ));
            }

            // Process descriptors
            let descriptors = entry.descriptors();
            let data_sectors = entry.data();
            let mut data_sector_index = 0;

            for desc in descriptors {
                match desc {
                    Descriptor::Data(data_desc) => {
                        // Write data from data sector to file
                        if data_sector_index < data_sectors.len() {
                            let sector = &data_sectors[data_sector_index];
                            let file_offset = data_desc.file_offset();

                            file.seek(SeekFrom::Start(file_offset))?;
                            // Write leading bytes (zeros), data, trailing bytes (zeros)
                            let leading = data_desc.leading_bytes();
                            let trailing = data_desc.trailing_bytes();

                            if leading > 0 {
                                file.write_all(&vec![0u8; leading as usize])?;
                            }
                            file.write_all(sector.data())?;
                            if trailing > 0 {
                                file.write_all(&vec![0u8; trailing as usize])?;
                            }

                            data_sector_index += 1;
                        }
                    }
                    Descriptor::Zero(zero_desc) => {
                        // Write zeros to file
                        let file_offset = zero_desc.file_offset();
                        let length = zero_desc.zero_length();

                        file.seek(SeekFrom::Start(file_offset))?;
                        file.write_all(&vec![0u8; length as usize])?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Try to parse an entry at the given offset
    fn try_parse_entry_at(&self, offset: usize) -> Result<LogEntry<'_>> {
        if offset + LOG_ENTRY_HEADER_SIZE > self.raw_data.len() {
            return Err(Error::LogEntryCorrupted("Not enough data".to_string()));
        }
        LogEntry::new(&self.raw_data[offset..])
    }
}

/// Log Entry
pub struct LogEntry<'a> {
    data: &'a [u8],
}

impl<'a> LogEntry<'a> {
    /// Create from raw data
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < LOG_ENTRY_HEADER_SIZE {
            return Err(Error::LogEntryCorrupted("Entry too small".to_string()));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get the entry header
    pub fn header(&self) -> LogEntryHeader<'_> {
        LogEntryHeader::new(&self.data[0..LOG_ENTRY_HEADER_SIZE])
    }

    /// Get a descriptor by index
    pub fn descriptor(&self, index: usize) -> Option<Descriptor<'_>> {
        let header = self.header();
        if index >= header.descriptor_count() as usize {
            return None;
        }

        // Calculate offset to descriptor
        // Descriptors start after the header, aligned to 32 bytes
        let desc_offset = LOG_ENTRY_HEADER_SIZE + index * DESCRIPTOR_SIZE;
        if desc_offset + DESCRIPTOR_SIZE > self.data.len() {
            return None;
        }

        Descriptor::parse(&self.data[desc_offset..desc_offset + DESCRIPTOR_SIZE]).ok()
    }

    /// Get all descriptors
    pub fn descriptors(&self) -> Vec<Descriptor<'_>> {
        let count = self.header().descriptor_count() as usize;
        (0..count).filter_map(|i| self.descriptor(i)).collect()
    }

    /// Get data sectors
    pub fn data(&self) -> Vec<DataSector<'_>> {
        // Data sectors follow descriptors
        let header = self.header();
        let desc_count = header.descriptor_count() as usize;
        let data_start = LOG_ENTRY_HEADER_SIZE + desc_count * DESCRIPTOR_SIZE;

        // Calculate how many data sectors we expect
        let data_sectors_needed: usize = self
            .descriptors()
            .iter()
            .filter_map(|d| match d {
                Descriptor::Data(_) => Some(1),
                Descriptor::Zero(_) => None,
            })
            .sum();

        let mut sectors = Vec::with_capacity(data_sectors_needed);
        for i in 0..data_sectors_needed {
            let offset = data_start + i * DATA_SECTOR_SIZE;
            if offset + DATA_SECTOR_SIZE > self.data.len() {
                break;
            }
            if let Ok(sector) = DataSector::new(&self.data[offset..offset + DATA_SECTOR_SIZE]) {
                sectors.push(sector);
            }
        }

        sectors
    }
}

/// Log Entry Header (64 bytes)
pub struct LogEntryHeader<'a> {
    data: &'a [u8],
}

impl<'a> LogEntryHeader<'a> {
    /// Create from raw data
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get signature (should be "loge")
    pub fn signature(&self) -> &[u8] {
        &self.data[0..4]
    }

    /// Get checksum
    pub fn checksum(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Get entry length (including header, descriptors, and data)
    pub fn entry_length(&self) -> u32 {
        u32::from_le_bytes(self.data[8..12].try_into().unwrap())
    }

    /// Get tail (offset to next entry)
    pub fn tail(&self) -> u32 {
        u32::from_le_bytes(self.data[12..16].try_into().unwrap())
    }

    /// Get sequence number
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    /// Get descriptor count
    pub fn descriptor_count(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    /// Get Log GUID
    pub fn log_guid(&self) -> Guid {
        Guid::from_bytes(self.data[32..48].try_into().unwrap())
    }

    /// Get flushed file offset
    pub fn flushed_file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[48..56].try_into().unwrap())
    }

    /// Get last file offset
    pub fn last_file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[56..64].try_into().unwrap())
    }
}

/// Descriptor - either Data or Zero
#[derive(Debug)]
pub enum Descriptor<'a> {
    Data(DataDescriptor<'a>),
    Zero(ZeroDescriptor<'a>),
}

impl<'a> Descriptor<'a> {
    /// Parse a descriptor from raw data
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted("Descriptor too small".to_string()));
        }

        let signature = &data[0..4];
        if signature == DATA_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Data(DataDescriptor::new(data)?))
        } else if signature == ZERO_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Zero(ZeroDescriptor::new(data)?))
        } else {
            Err(Error::InvalidSignature {
                expected: "desc or zero".to_string(),
                found: String::from_utf8_lossy(signature).to_string(),
            })
        }
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        match self {
            Descriptor::Data(d) => d.raw(),
            Descriptor::Zero(z) => z.raw(),
        }
    }
}

/// Data Descriptor (32 bytes)
#[derive(Debug)]
pub struct DataDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> DataDescriptor<'a> {
    /// Create from raw data
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Data Descriptor too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get trailing bytes
    pub fn trailing_bytes(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Get leading bytes
    pub fn leading_bytes(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    /// Get file offset
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    /// Get sequence number
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[24..32].try_into().unwrap())
    }
}

/// Zero Descriptor (32 bytes)
#[derive(Debug)]
pub struct ZeroDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> ZeroDescriptor<'a> {
    /// Create from raw data
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Zero Descriptor too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get zero length
    pub fn zero_length(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    /// Get file offset
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    /// Get sequence number
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[24..32].try_into().unwrap())
    }
}

/// Data Sector (4 KB)
pub struct DataSector<'a> {
    data: &'a [u8],
}

impl<'a> DataSector<'a> {
    /// Create from raw data
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != DATA_SECTOR_SIZE {
            return Err(Error::InvalidFile(format!(
                "Data Sector must be {} bytes, got {}",
                DATA_SECTOR_SIZE,
                data.len()
            )));
        }
        Ok(Self { data })
    }

    /// Return raw bytes
    pub fn raw(&self) -> &[u8] {
        self.data
    }

    /// Get sequence high (bits 32-63)
    pub fn sequence_high(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    /// Get data portion (4084 bytes)
    pub fn data(&self) -> &[u8] {
        &self.data[8..4092]
    }

    /// Get sequence low (bits 0-31)
    pub fn sequence_low(&self) -> u32 {
        u32::from_le_bytes(self.data[4092..4096].try_into().unwrap())
    }

    /// Get full sequence number
    pub fn sequence_number(&self) -> u64 {
        ((self.sequence_high() as u64) << 32) | (self.sequence_low() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_header() {
        let mut data = [0u8; 64];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        data[4..8].copy_from_slice(&0x12345678u32.to_le_bytes());
        data[8..12].copy_from_slice(&0x1000u32.to_le_bytes()); // 4KB entry
        data[16..24].copy_from_slice(&0x1u64.to_le_bytes()); // sequence
        data[24..28].copy_from_slice(&2u32.to_le_bytes()); // 2 descriptors

        let header = LogEntryHeader::new(&data);
        assert_eq!(header.signature(), LOG_ENTRY_SIGNATURE);
        assert_eq!(header.checksum(), 0x12345678);
        assert_eq!(header.entry_length(), 0x1000);
        assert_eq!(header.sequence_number(), 1);
        assert_eq!(header.descriptor_count(), 2);
    }

    #[test]
    fn test_data_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        data[4..8].copy_from_slice(&0x100u32.to_le_bytes()); // trailing
        data[8..16].copy_from_slice(&0x200u64.to_le_bytes()); // leading
        data[16..24].copy_from_slice(&0x100000u64.to_le_bytes()); // offset
        data[24..32].copy_from_slice(&0x1u64.to_le_bytes()); // sequence

        let desc = DataDescriptor::new(&data).unwrap();
        assert_eq!(desc.trailing_bytes(), 0x100);
        assert_eq!(desc.leading_bytes(), 0x200);
        assert_eq!(desc.file_offset(), 0x100000);
        assert_eq!(desc.sequence_number(), 1);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        data[8..16].copy_from_slice(&0x1000u64.to_le_bytes()); // length
        data[16..24].copy_from_slice(&0x200000u64.to_le_bytes()); // offset

        let desc = ZeroDescriptor::new(&data).unwrap();
        assert_eq!(desc.zero_length(), 0x1000);
        assert_eq!(desc.file_offset(), 0x200000);
    }
}
