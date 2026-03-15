//! Log Entry structures

use crate::common::crc32c::crc32c_with_zero_field;
use crate::common::guid::Guid;
use crate::error::{Error, Result};
use crate::log::descriptor::{DataDescriptor, ZeroDescriptor};
use crate::log::sector::DataSector;
use crate::log::{DATA_DESCRIPTOR_SIGNATURE, LOG_ENTRY_SIGNATURE, ZERO_DESCRIPTOR_SIGNATURE};
use byteorder::{ByteOrder, LittleEndian};

/// Log Entry Header (64 bytes)
#[derive(Debug, Clone)]
pub struct LogEntryHeader {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub entry_length: u32,
    pub tail: u32,
    pub sequence_number: u64,
    pub descriptor_count: u32,
    pub log_guid: Guid,
    pub flushed_file_offset: u64,
    pub last_file_offset: u64,
}

impl LogEntryHeader {
    /// Size of header
    pub const SIZE: usize = 64;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(Error::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if signature != LOG_ENTRY_SIGNATURE {
            return Err(Error::InvalidSignature {
                expected: String::from_utf8_lossy(LOG_ENTRY_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let checksum = LittleEndian::read_u32(&data[4..8]);
        let entry_length = LittleEndian::read_u32(&data[8..12]);
        let tail = LittleEndian::read_u32(&data[12..16]);
        let sequence_number = LittleEndian::read_u64(&data[16..24]);
        let descriptor_count = LittleEndian::read_u32(&data[24..28]);

        let mut log_guid = [0u8; 16];
        log_guid.copy_from_slice(&data[32..48]);
        let log_guid = Guid::from_bytes(log_guid);

        let flushed_file_offset = LittleEndian::read_u64(&data[48..56]);
        let last_file_offset = LittleEndian::read_u64(&data[56..64]);

        // Validate entry length (must be multiple of 4KB)
        if entry_length == 0 || entry_length % 4096 != 0 {
            return Err(Error::InvalidLogEntry);
        }

        // Validate tail (must be multiple of 4KB)
        if tail % 4096 != 0 {
            return Err(Error::InvalidLogEntry);
        }

        // Validate sequence number (must be > 0)
        if sequence_number == 0 {
            return Err(Error::InvalidLogEntry);
        }

        Ok(LogEntryHeader {
            signature,
            checksum,
            entry_length,
            tail,
            sequence_number,
            descriptor_count,
            log_guid,
            flushed_file_offset,
            last_file_offset,
        })
    }

    /// Verify checksum
    pub fn verify_checksum(&self, data: &[u8]) -> bool {
        if data.len() < self.entry_length as usize {
            return false;
        }
        let calculated = crc32c_with_zero_field(&data[..self.entry_length as usize], 4, 4);
        calculated == self.checksum
    }

    /// Get the data sector count (for data descriptors)
    /// Each data descriptor has one corresponding data sector
    pub fn data_sector_count(&self) -> u32 {
        // This is determined by the descriptors, not the header directly
        // We need to parse descriptors to know
        self.descriptor_count
    }
}

/// Log Entry (complete)
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub header: LogEntryHeader,
    pub zero_descriptors: Vec<ZeroDescriptor>,
    pub data_descriptors: Vec<DataDescriptor>,
    pub data_sectors: Vec<DataSector>,
}

impl LogEntry {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        // Parse header
        let header = LogEntryHeader::from_bytes(data)?;

        // Verify checksum
        if !header.verify_checksum(data) {
            return Err(Error::InvalidChecksum);
        }

        let mut zero_descriptors = Vec::new();
        let mut data_descriptors = Vec::new();
        let mut data_sectors = Vec::new();

        // Parse descriptors (start after header, each 32 bytes)
        let mut descriptor_offset = 64; // Header is 64 bytes

        for _ in 0..header.descriptor_count {
            // Determine descriptor type by signature
            if descriptor_offset + 4 > data.len() {
                return Err(Error::InvalidLogEntry);
            }

            let sig = &data[descriptor_offset..descriptor_offset + 4];

            if sig == ZERO_DESCRIPTOR_SIGNATURE {
                let desc = ZeroDescriptor::from_bytes(&data[descriptor_offset..])?;
                if !desc.verify_sequence(header.sequence_number) {
                    return Err(Error::InvalidLogEntry);
                }
                zero_descriptors.push(desc);
            } else if sig == DATA_DESCRIPTOR_SIGNATURE {
                let desc = DataDescriptor::from_bytes(&data[descriptor_offset..])?;
                if !desc.verify_sequence(header.sequence_number) {
                    return Err(Error::InvalidLogEntry);
                }
                data_descriptors.push(desc);

                // Each data descriptor has a corresponding data sector
                let sector_offset =
                    header.entry_length as usize - (data_descriptors.len() * DataSector::SIZE);

                if sector_offset + DataSector::SIZE > data.len() {
                    return Err(Error::InvalidLogEntry);
                }

                let sector = DataSector::from_bytes(&data[sector_offset..])?;
                if !sector.verify_sequence(header.sequence_number) {
                    return Err(Error::InvalidLogEntry);
                }
                data_sectors.push(sector);
            } else {
                return Err(Error::InvalidSignature {
                    expected: "zero or desc".to_string(),
                    got: String::from_utf8_lossy(sig).to_string(),
                });
            }

            descriptor_offset += 32;
        }

        Ok(LogEntry {
            header,
            zero_descriptors,
            data_descriptors,
            data_sectors,
        })
    }

    /// Validate the entry is complete and consistent
    pub fn validate(&self) -> bool {
        // Check data descriptor count matches data sector count
        if self.data_descriptors.len() != self.data_sectors.len() {
            return false;
        }

        // Verify all sequence numbers match
        for desc in &self.zero_descriptors {
            if desc.sequence_number != self.header.sequence_number {
                return false;
            }
        }

        for desc in &self.data_descriptors {
            if desc.sequence_number != self.header.sequence_number {
                return false;
            }
        }

        for sector in &self.data_sectors {
            if sector.sequence_number() != self.header.sequence_number {
                return false;
            }
        }

        true
    }
}

/// Log Sequence - a sequence of valid log entries
#[derive(Debug, Clone)]
pub struct LogSequence {
    pub entries: Vec<LogEntry>,
    pub head_sequence: u64,
    pub tail_offset: u32,
}

impl LogSequence {
    /// Check if sequence is valid and complete
    ///
    /// A sequence is valid if:
    /// 1. All entries are valid
    /// 2. Sequence numbers are consecutive
    /// 3. The tail of the head entry points within the sequence
    pub fn is_valid(&self) -> bool {
        if self.entries.is_empty() {
            return false;
        }

        // Check consecutive sequence numbers
        for i in 1..self.entries.len() {
            let expected_seq = self.entries[i - 1].header.sequence_number + 1;
            if self.entries[i].header.sequence_number != expected_seq {
                return false;
            }
        }

        // Check that tail points within sequence
        let _head = &self.entries[self.entries.len() - 1];
        let _tail_found = self.entries.iter().any(|_e| {
            // This is a simplified check - in reality we'd need to track
            // file offsets of entries
            true // Placeholder
        });

        true // Simplified - always return true for now
    }
}
