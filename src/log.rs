//! VHDX Log System structures and operations
//!
//! The log ensures crash consistency for metadata updates.
//! It must be replayed before any writes are allowed.

use std::io::{Seek, SeekFrom, Write};

use crate::crc32c::crc32c_with_zero_field;
use crate::error::{Result, VhdxError};
use crate::guid::Guid;
use byteorder::{ByteOrder, LittleEndian};

/// Log Entry signature: "loge"
pub const LOG_ENTRY_SIGNATURE: &[u8] = b"loge";

/// Zero Descriptor signature: "zero"
pub const ZERO_DESCRIPTOR_SIGNATURE: &[u8] = b"zero";

/// Data Descriptor signature: "desc"
pub const DATA_DESCRIPTOR_SIGNATURE: &[u8] = b"desc";

/// Data Sector signature: "data"
pub const DATA_SECTOR_SIGNATURE: &[u8] = b"data";

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
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != LOG_ENTRY_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
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
            return Err(VhdxError::InvalidLogEntry);
        }

        // Validate tail (must be multiple of 4KB)
        if tail % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        // Validate sequence number (must be > 0)
        if sequence_number == 0 {
            return Err(VhdxError::InvalidLogEntry);
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

/// Zero Descriptor (32 bytes)
#[derive(Debug, Clone)]
pub struct ZeroDescriptor {
    pub signature: [u8; 4],
    pub zero_length: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
}

impl ZeroDescriptor {
    /// Size of descriptor
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != ZERO_DESCRIPTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(ZERO_DESCRIPTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let zero_length = LittleEndian::read_u64(&data[8..16]);
        let file_offset = LittleEndian::read_u64(&data[16..24]);
        let sequence_number = LittleEndian::read_u64(&data[24..32]);

        // Validate zero_length (must be multiple of 4KB)
        if zero_length == 0 || zero_length % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        // Validate file_offset (must be multiple of 4KB)
        if file_offset % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        Ok(ZeroDescriptor {
            signature,
            zero_length,
            file_offset,
            sequence_number,
        })
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number == header_seq
    }
}

/// Data Descriptor (32 bytes)
#[derive(Debug, Clone)]
pub struct DataDescriptor {
    pub signature: [u8; 4],
    pub trailing_bytes: [u8; 4],
    pub leading_bytes: [u8; 8],
    pub file_offset: u64,
    pub sequence_number: u64,
}

impl DataDescriptor {
    /// Size of descriptor
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != DATA_DESCRIPTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(DATA_DESCRIPTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let mut trailing_bytes = [0u8; 4];
        trailing_bytes.copy_from_slice(&data[4..8]);

        let mut leading_bytes = [0u8; 8];
        leading_bytes.copy_from_slice(&data[8..16]);

        let file_offset = LittleEndian::read_u64(&data[16..24]);
        let sequence_number = LittleEndian::read_u64(&data[24..32]);

        // Validate file_offset (must be multiple of 4KB)
        if file_offset % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        Ok(DataDescriptor {
            signature,
            trailing_bytes,
            leading_bytes,
            file_offset,
            sequence_number,
        })
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number == header_seq
    }
}

/// Data Sector (4KB)
#[derive(Debug, Clone)]
pub struct DataSector {
    pub signature: [u8; 4],
    pub sequence_high: u32,
    pub data: [u8; 4084],
    pub sequence_low: u32,
}

impl DataSector {
    /// Size of sector
    pub const SIZE: usize = 4096;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::InvalidLogEntry);
        }

        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != DATA_SECTOR_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(DATA_SECTOR_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let sequence_high = LittleEndian::read_u32(&data[4..8]);

        let mut sector_data = [0u8; 4084];
        sector_data.copy_from_slice(&data[8..4092]);

        let sequence_low = LittleEndian::read_u32(&data[4092..4096]);

        Ok(DataSector {
            signature,
            sequence_high,
            data: sector_data,
            sequence_low,
        })
    }

    /// Get full sequence number
    pub fn sequence_number(&self) -> u64 {
        ((self.sequence_high as u64) << 32) | (self.sequence_low as u64)
    }

    /// Verify sequence number matches header
    pub fn verify_sequence(&self, header_seq: u64) -> bool {
        self.sequence_number() == header_seq
    }

    /// Reconstruct full 4KB sector data
    /// Combines leading bytes (from descriptor) + data + trailing bytes (from descriptor)
    pub fn reconstruct_sector(&self, descriptor: &DataDescriptor) -> [u8; 4096] {
        let mut full_data = [0u8; 4096];

        // Leading bytes (first 8 bytes)
        full_data[0..8].copy_from_slice(&descriptor.leading_bytes);

        // Data (bytes 8-4091)
        full_data[8..4092].copy_from_slice(&self.data);

        // Trailing bytes (last 4 bytes)
        full_data[4092..4096].copy_from_slice(&descriptor.trailing_bytes);

        full_data
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
            return Err(VhdxError::InvalidChecksum);
        }

        let mut zero_descriptors = Vec::new();
        let mut data_descriptors = Vec::new();
        let mut data_sectors = Vec::new();

        // Parse descriptors (start after header, each 32 bytes)
        let mut descriptor_offset = 64; // Header is 64 bytes

        for _ in 0..header.descriptor_count {
            // Determine descriptor type by signature
            if descriptor_offset + 4 > data.len() {
                return Err(VhdxError::InvalidLogEntry);
            }

            let sig = &data[descriptor_offset..descriptor_offset + 4];

            if sig == ZERO_DESCRIPTOR_SIGNATURE {
                let desc = ZeroDescriptor::from_bytes(&data[descriptor_offset..])?;
                if !desc.verify_sequence(header.sequence_number) {
                    return Err(VhdxError::InvalidLogEntry);
                }
                zero_descriptors.push(desc);
            } else if sig == DATA_DESCRIPTOR_SIGNATURE {
                let desc = DataDescriptor::from_bytes(&data[descriptor_offset..])?;
                if !desc.verify_sequence(header.sequence_number) {
                    return Err(VhdxError::InvalidLogEntry);
                }
                data_descriptors.push(desc);

                // Each data descriptor has a corresponding data sector
                let sector_offset =
                    header.entry_length as usize - (data_descriptors.len() * DataSector::SIZE);

                if sector_offset + DataSector::SIZE > data.len() {
                    return Err(VhdxError::InvalidLogEntry);
                }

                let sector = DataSector::from_bytes(&data[sector_offset..])?;
                if !sector.verify_sequence(header.sequence_number) {
                    return Err(VhdxError::InvalidLogEntry);
                }
                data_sectors.push(sector);
            } else {
                return Err(VhdxError::InvalidSignature {
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

/// Log Replay algorithm implementation
pub struct LogReplayer;

impl LogReplayer {
    /// Find the active log sequence
    ///
    /// Implements the algorithm from MS-VHDX section 2.3.3
    pub fn find_active_sequence(
        log_data: &[u8],
        log_size: u32,
        log_guid: &Guid,
    ) -> Result<Option<LogSequence>> {
        let mut candidate_sequence: Option<LogSequence> = None;
        let mut current_tail: u32 = 0;
        let mut old_tail: u32 = 0;

        loop {
            // Try to build a sequence starting at current_tail
            let mut current_sequence = Vec::new();
            let mut current_seq_num: u64 = 0;
            let mut entry_offset = current_tail;

            // Scan forward looking for valid entries
            loop {
                if entry_offset as usize + 64 > log_data.len() {
                    break;
                }

                // Try to parse entry at this offset
                match LogEntry::from_bytes(&log_data[entry_offset as usize..]) {
                    Ok(entry) => {
                        // Verify LogGuid matches
                        if entry.header.log_guid != *log_guid {
                            break;
                        }

                        // Get entry length before moving entry
                        let entry_length = entry.header.entry_length;
                        let seq_num = entry.header.sequence_number;

                        // Check sequence number continuity
                        if current_sequence.is_empty() {
                            current_sequence.push(entry);
                            current_seq_num = seq_num;
                            entry_offset += entry_length;
                        } else if seq_num == current_seq_num + 1 {
                            current_sequence.push(entry);
                            current_seq_num = seq_num;
                            entry_offset += entry_length;
                        } else {
                            break;
                        }

                        // Check if this completes the sequence (tail points to start)
                        if !current_sequence.is_empty() {
                            let head = &current_sequence[current_sequence.len() - 1];
                            if head.header.tail == current_tail {
                                // Found a valid complete sequence
                                let sequence = LogSequence {
                                    head_sequence: head.header.sequence_number,
                                    tail_offset: head.header.tail,
                                    entries: current_sequence.clone(),
                                };

                                // Update candidate if this has higher sequence number
                                if candidate_sequence.is_none()
                                    || sequence.head_sequence
                                        > candidate_sequence.as_ref().unwrap().head_sequence
                                {
                                    candidate_sequence = Some(sequence);
                                }
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            // Move to next position
            if current_sequence.is_empty() {
                current_tail = (current_tail + 4096) % log_size;
            } else {
                current_tail = entry_offset % log_size;
            }

            // Check if we've wrapped around
            if current_tail < old_tail {
                break;
            }
            old_tail = current_tail;
        }

        Ok(candidate_sequence)
    }

    /// Replay a log sequence
    ///
    /// Applies all changes from the sequence to the file
    pub fn replay_sequence<W: std::io::Write + std::io::Seek>(
        sequence: &LogSequence,
        writer: &mut W,
    ) -> Result<u64> {
        // Replay from tail to head
        for entry in &sequence.entries {
            // Replay zero descriptors
            for zero_desc in &entry.zero_descriptors {
                let zeros = vec![0u8; zero_desc.zero_length as usize];
                writer.seek(std::io::SeekFrom::Start(zero_desc.file_offset))?;
                writer.write_all(&zeros)?;
            }

            // Replay data descriptors
            for (i, data_desc) in entry.data_descriptors.iter().enumerate() {
                if let Some(sector) = entry.data_sectors.get(i) {
                    let full_data = sector.reconstruct_sector(data_desc);
                    writer.seek(std::io::SeekFrom::Start(data_desc.file_offset))?;
                    writer.write_all(&full_data)?;
                }
            }
        }

        // Return the flushed file offset from head entry (guaranteed stable size)
        if let Some(head) = sequence.entries.last() {
            Ok(head.header.flushed_file_offset)
        } else {
            Ok(0)
        }
    }
}

/// Log Writer for creating log entries
///
/// Implements the log write side of the VHDX log protocol.
/// All metadata updates must go through the log for crash consistency.
pub struct LogWriter {
    /// Log offset in file
    log_offset: u64,
    /// Log size
    log_size: u32,
    /// Current write position (circular buffer)
    write_offset: u32,
    /// Current sequence number
    sequence_number: u64,
    /// Log GUID for entry validation
    log_guid: Guid,
    /// Current file size (for FlushedFileOffset)
    file_size: u64,
}

impl LogWriter {
    /// Create a new log writer
    pub fn new(log_offset: u64, log_size: u32, log_guid: Guid, file_size: u64) -> Self {
        LogWriter {
            log_offset,
            log_size,
            write_offset: 0,
            sequence_number: 1, // Start at 1 (0 is invalid)
            log_guid,
            file_size,
        }
    }

    /// Set sequence number (should be loaded from header)
    pub fn set_sequence_number(&mut self, seq: u64) {
        self.sequence_number = seq;
    }

    /// Get next sequence number
    pub fn next_sequence(&mut self) -> u64 {
        let seq = self.sequence_number;
        self.sequence_number += 1;
        seq
    }

    /// Calculate entry size needed for data descriptors
    ///
    /// Entry structure:
    /// - 64 bytes: Entry Header
    /// - 32 bytes per descriptor
    /// - 4096 bytes per data sector
    fn calculate_entry_size(&self, num_data_descriptors: usize) -> u32 {
        let header_size = 64u32;
        let descriptors_size = (num_data_descriptors * 32) as u32;
        let data_sectors_size = (num_data_descriptors * 4096) as u32;
        let total = header_size + descriptors_size + data_sectors_size;

        // Round up to 4KB
        ((total + 4095) / 4096) * 4096
    }

    /// Write a log entry with a single data update
    ///
    /// This is the primary method for logging metadata updates.
    /// Returns the file offset where the entry was written.
    pub fn write_data_entry<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        file_offset: u64,
        data: &[u8],
    ) -> Result<u64> {
        if data.len() != 4096 {
            return Err(VhdxError::InvalidLogEntry);
        }

        let entry_size = self.calculate_entry_size(1);

        // Check if we have room (simplified - doesn't handle wraparound)
        if self.write_offset + entry_size > self.log_size {
            return Err(VhdxError::LogReplayFailed("Log full".to_string()));
        }

        let seq = self.next_sequence();
        let entry_file_offset = self.log_offset + self.write_offset as u64;

        // Build entry data
        let mut entry_data = vec![0u8; entry_size as usize];

        // Entry Header (64 bytes)
        entry_data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        // Checksum (calculated later)
        LittleEndian::write_u32(&mut entry_data[8..12], entry_size);
        LittleEndian::write_u32(&mut entry_data[12..16], self.write_offset); // Tail points to self
        LittleEndian::write_u64(&mut entry_data[16..24], seq);
        LittleEndian::write_u32(&mut entry_data[24..28], 1); // 1 descriptor
                                                             // Reserved [28..32] = 0
        entry_data[32..48].copy_from_slice(&self.log_guid.to_bytes());
        LittleEndian::write_u64(&mut entry_data[48..56], self.file_size); // FlushedFileOffset
        LittleEndian::write_u64(&mut entry_data[56..64], self.file_size); // LastFileOffset

        // Data Descriptor (32 bytes) at offset 64
        entry_data[64..68].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        // TrailingBytes [4..8] - last 4 bytes of data sector
        entry_data[68..72].copy_from_slice(&data[4092..4096]);
        // LeadingBytes [8..16] - first 8 bytes of data sector
        entry_data[72..80].copy_from_slice(&data[0..8]);
        LittleEndian::write_u64(&mut entry_data[80..88], file_offset);
        LittleEndian::write_u64(&mut entry_data[88..96], seq);

        // Data Sector (4096 bytes) at offset 4096
        let sector_offset = 4096usize;
        entry_data[sector_offset..sector_offset + 4].copy_from_slice(DATA_SECTOR_SIGNATURE);
        // SequenceHigh [4..8]
        LittleEndian::write_u32(
            &mut entry_data[sector_offset + 4..sector_offset + 8],
            (seq >> 32) as u32,
        );
        // Data [8..4092]
        entry_data[sector_offset + 8..sector_offset + 4092].copy_from_slice(&data[8..4092]);
        // SequenceLow [4092..4096]
        LittleEndian::write_u32(
            &mut entry_data[sector_offset + 4092..sector_offset + 4096],
            seq as u32,
        );

        // Calculate and write checksum
        let checksum = crc32c_with_zero_field(&entry_data, 4, 4);
        LittleEndian::write_u32(&mut entry_data[4..8], checksum);

        // Write to log
        writer.seek(SeekFrom::Start(entry_file_offset))?;
        writer.write_all(&entry_data)?;

        // Update state
        self.write_offset += entry_size;

        Ok(entry_file_offset)
    }

    /// Write a log entry with a zero descriptor
    ///
    /// Used when clearing/trimming blocks
    pub fn write_zero_entry<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        file_offset: u64,
        length: u64,
    ) -> Result<u64> {
        if length % 4096 != 0 {
            return Err(VhdxError::InvalidLogEntry);
        }

        let entry_size = 4096u32; // Header + 1 zero descriptor, rounded to 4KB

        if self.write_offset + entry_size > self.log_size {
            return Err(VhdxError::LogReplayFailed("Log full".to_string()));
        }

        let seq = self.next_sequence();
        let entry_file_offset = self.log_offset + self.write_offset as u64;

        // Build entry data
        let mut entry_data = vec![0u8; entry_size as usize];

        // Entry Header (64 bytes)
        entry_data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        LittleEndian::write_u32(&mut entry_data[8..12], entry_size);
        LittleEndian::write_u32(&mut entry_data[12..16], self.write_offset);
        LittleEndian::write_u64(&mut entry_data[16..24], seq);
        LittleEndian::write_u32(&mut entry_data[24..28], 1); // 1 descriptor
        entry_data[32..48].copy_from_slice(&self.log_guid.to_bytes());
        LittleEndian::write_u64(&mut entry_data[48..56], self.file_size);
        LittleEndian::write_u64(&mut entry_data[56..64], self.file_size);

        // Zero Descriptor (32 bytes) at offset 64
        entry_data[64..68].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        // Reserved [68..72] = 0
        LittleEndian::write_u64(&mut entry_data[72..80], length);
        LittleEndian::write_u64(&mut entry_data[80..88], file_offset);
        LittleEndian::write_u64(&mut entry_data[88..96], seq);

        // Calculate and write checksum
        let checksum = crc32c_with_zero_field(&entry_data, 4, 4);
        LittleEndian::write_u32(&mut entry_data[4..8], checksum);

        // Write to log
        writer.seek(SeekFrom::Start(entry_file_offset))?;
        writer.write_all(&entry_data)?;

        // Update state
        self.write_offset += entry_size;

        Ok(entry_file_offset)
    }

    /// Clear the log after successful operations
    ///
    /// According to spec, after replaying log and applying all changes,
    /// the log should be cleared and LogGuid reset.
    pub fn clear_log<W: Write + Seek>(&self, writer: &mut W) -> Result<()> {
        // Write zeros to entire log region
        let zeros = vec![0u8; self.log_size as usize];
        writer.seek(SeekFrom::Start(self.log_offset))?;
        writer.write_all(&zeros)?;
        Ok(())
    }

    /// Update file size tracking
    pub fn update_file_size(&mut self, new_size: u64) {
        self.file_size = new_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_header() {
        let mut data = vec![0u8; 4096];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        LittleEndian::write_u32(&mut data[8..12], 4096); // entry_length
        LittleEndian::write_u32(&mut data[12..16], 0); // tail
        LittleEndian::write_u64(&mut data[16..24], 1); // sequence_number
        LittleEndian::write_u32(&mut data[24..28], 0); // descriptor_count

        // Calculate and write checksum
        let checksum = crc32c_with_zero_field(&data, 4, 4);
        LittleEndian::write_u32(&mut data[4..8], checksum);

        let header = LogEntryHeader::from_bytes(&data).unwrap();
        assert!(header.verify_checksum(&data));
        assert_eq!(header.sequence_number, 1);
        assert_eq!(header.entry_length, 4096);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        LittleEndian::write_u64(&mut data[8..16], 4096); // zero_length
        LittleEndian::write_u64(&mut data[16..24], 4096); // file_offset
        LittleEndian::write_u64(&mut data[24..32], 1); // sequence_number

        let desc = ZeroDescriptor::from_bytes(&data).unwrap();
        assert_eq!(desc.zero_length, 4096);
        assert_eq!(desc.file_offset, 4096);
        assert!(desc.verify_sequence(1));
    }

    #[test]
    fn test_data_descriptor() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        data[4..8].copy_from_slice(&[1, 2, 3, 4]); // trailing_bytes
        data[8..16].copy_from_slice(&[5, 6, 7, 8, 9, 10, 11, 12]); // leading_bytes
        LittleEndian::write_u64(&mut data[16..24], 4096); // file_offset
        LittleEndian::write_u64(&mut data[24..32], 1); // sequence_number

        let desc = DataDescriptor::from_bytes(&data).unwrap();
        assert_eq!(desc.file_offset, 4096);
        assert!(desc.verify_sequence(1));
    }

    #[test]
    fn test_data_sector() {
        let mut data = vec![0u8; 4096];
        data[0..4].copy_from_slice(DATA_SECTOR_SIGNATURE);
        LittleEndian::write_u32(&mut data[4..8], 0); // sequence_high
                                                     // data in middle
        LittleEndian::write_u32(&mut data[4092..4096], 1); // sequence_low

        let sector = DataSector::from_bytes(&data).unwrap();
        assert_eq!(sector.sequence_number(), 1);
    }
}
