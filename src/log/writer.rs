//! Log Writer for creating log entries

use std::io::{Seek, SeekFrom, Write};

use crate::common::crc32c::crc32c_with_zero_field;
use crate::common::guid::Guid;
use crate::error::{Result, VhdxError};
use crate::log::{
    DATA_DESCRIPTOR_SIGNATURE, DATA_SECTOR_SIGNATURE, LOG_ENTRY_SIGNATURE,
    ZERO_DESCRIPTOR_SIGNATURE,
};
use byteorder::{ByteOrder, LittleEndian};

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
        total.div_ceil(4096) * 4096
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
        if !length.is_multiple_of(4096) {
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
