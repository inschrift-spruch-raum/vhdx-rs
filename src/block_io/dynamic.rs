//! Dynamic disk block I/O
//!
//! Block I/O implementation for dynamic VHDX disks where blocks
//! are allocated on-demand as data is written.

use crate::bat::{Bat, BatEntry, PayloadBlockState};
use crate::error::{Result, VhdxError};
use crate::log::LogWriter;
use std::io::{Read, Seek, SeekFrom, Write};

/// Dynamic disk block I/O (allocates blocks on demand)
///
/// For dynamic disks, blocks are allocated only when data is written
/// to a previously unallocated region. This allows sparse file storage
/// where unallocated regions read as zeros.
pub struct DynamicBlockIo<'a> {
    /// Reference to file handle
    pub file: &'a mut std::fs::File,
    /// Reference to BAT (mutable for writes)
    pub bat: &'a mut Bat,
    /// Next free file offset for allocation
    pub next_free_offset: u64,
    /// Virtual disk size
    pub virtual_disk_size: u64,
    /// Optional log writer for metadata updates
    log_writer: Option<LogWriter>,
}

impl<'a> DynamicBlockIo<'a> {
    /// Create new dynamic block I/O handler
    pub fn new(file: &'a mut std::fs::File, bat: &'a mut Bat, virtual_disk_size: u64) -> Self {
        DynamicBlockIo {
            file,
            bat,
            next_free_offset: 1024 * 1024, // Start after 1MB header
            virtual_disk_size,
            log_writer: None,
        }
    }

    /// Set log writer for metadata updates
    pub fn with_log_writer(mut self, log_writer: LogWriter) -> Self {
        self.log_writer = Some(log_writer);
        self
    }

    /// Read data from virtual offset
    ///
    /// Returns the number of bytes read (may be less than requested for sparse regions)
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_read =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        let mut bytes_read = 0;
        let mut current_offset = virtual_offset;

        while bytes_read < bytes_to_read {
            // Calculate block index and offset within block
            let block_idx = self.bat.block_index_from_offset(current_offset);
            let offset_in_block = self.bat.offset_in_block(current_offset);

            // Calculate bytes to read from this block
            let block_remaining = self.bat.block_size - offset_in_block;
            let bytes_from_block =
                std::cmp::min(block_remaining as usize, bytes_to_read - bytes_read);

            // Get BAT entry
            match self.bat.get_payload_entry(block_idx) {
                Some(entry) => {
                    match entry.state {
                        PayloadBlockState::FullyPresent => {
                            if let Some(file_offset) = entry.file_offset() {
                                let absolute_offset = file_offset + offset_in_block;
                                self.file.seek(SeekFrom::Start(absolute_offset))?;
                                self.file.read_exact(
                                    &mut buf[bytes_read..bytes_read + bytes_from_block],
                                )?;
                            } else {
                                return Err(VhdxError::InvalidBatEntry);
                            }
                        }
                        PayloadBlockState::Zero
                        | PayloadBlockState::NotPresent
                        | PayloadBlockState::Unmapped => {
                            // Return zeros for unallocated blocks
                            for i in bytes_read..bytes_read + bytes_from_block {
                                buf[i] = 0;
                            }
                        }
                        PayloadBlockState::PartiallyPresent => {
                            // Partially present should not occur in pure dynamic disks
                            // Treat as unallocated (return zeros)
                            for i in bytes_read..bytes_read + bytes_from_block {
                                buf[i] = 0;
                            }
                        }
                        PayloadBlockState::Undefined => {
                            return Err(VhdxError::InvalidBatEntry);
                        }
                    }
                }
                None => {
                    // Block not in BAT - return zeros
                    for i in bytes_read..bytes_read + bytes_from_block {
                        buf[i] = 0;
                    }
                }
            }

            bytes_read += bytes_from_block;
            current_offset += bytes_from_block as u64;
        }

        Ok(bytes_read)
    }

    /// Write data to virtual offset
    ///
    /// For dynamic disks, this will allocate new blocks as needed.
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_write =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        let mut bytes_written = 0;
        let mut current_offset = virtual_offset;

        while bytes_written < bytes_to_write {
            // Calculate block index and offset within block
            let block_idx = self.bat.block_index_from_offset(current_offset);
            let offset_in_block = self.bat.offset_in_block(current_offset);

            // Calculate bytes to write to this block
            let block_remaining = self.bat.block_size - offset_in_block;
            let bytes_to_block =
                std::cmp::min(block_remaining as usize, bytes_to_write - bytes_written);

            // Get or allocate BAT entry
            let entry = self
                .bat
                .get_payload_entry(block_idx)
                .ok_or(VhdxError::InvalidBatEntry)?;

            let file_offset = match entry.state {
                PayloadBlockState::FullyPresent => {
                    entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?
                }
                PayloadBlockState::NotPresent
                | PayloadBlockState::Zero
                | PayloadBlockState::Unmapped => {
                    // Need to allocate block
                    self.allocate_block(block_idx)?;
                    // Re-get the entry after allocation
                    self.bat
                        .get_payload_entry(block_idx)
                        .and_then(|e| e.file_offset())
                        .ok_or(VhdxError::InvalidBatEntry)?
                }
                PayloadBlockState::PartiallyPresent => {
                    // Treat as allocate for dynamic disks
                    self.allocate_block(block_idx)?;
                    self.bat
                        .get_payload_entry(block_idx)
                        .and_then(|e| e.file_offset())
                        .ok_or(VhdxError::InvalidBatEntry)?
                }
                PayloadBlockState::Undefined => {
                    return Err(VhdxError::InvalidBatEntry);
                }
            };

            // Write to file
            let absolute_offset = file_offset + offset_in_block;
            self.file.seek(SeekFrom::Start(absolute_offset))?;
            self.file
                .write_all(&buf[bytes_written..bytes_written + bytes_to_block])?;

            bytes_written += bytes_to_block;
            current_offset += bytes_to_block as u64;
        }

        // Flush to ensure data is stable
        self.file.flush()?;

        Ok(bytes_written)
    }

    /// Allocate a new block with optional log-based BAT update
    ///
    /// Returns the file offset of the allocated block
    fn allocate_block(&mut self, block_idx: u64) -> Result<u64> {
        // Align next free offset to 1MB
        let aligned_offset = (self.next_free_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1);

        // Allocate block space
        let block_size = self.bat.block_size;
        let file_offset_mb = aligned_offset / (1024 * 1024);

        // Extend file if necessary
        self.file
            .seek(SeekFrom::Start(aligned_offset + block_size - 1))?;
        self.file.write_all(&[0])?;

        // Calculate BAT entry location
        let bat_index = self
            .bat
            .payload_bat_index(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        let bat_entry_offset = self.bat.get_bat_entry_file_offset(bat_index);

        // Create new BAT entry
        let new_entry = BatEntry::new(PayloadBlockState::FullyPresent, file_offset_mb);

        // If we have a log writer, use it for atomic BAT update
        if let Some(ref mut log_writer) = self.log_writer {
            // Prepare 4KB sector with BAT entry data (padded)
            let mut sector_data = vec![0u8; 4096];
            let entry_bytes = new_entry.to_bytes();
            sector_data[0..8].copy_from_slice(&entry_bytes);

            // Write to log
            log_writer.write_data_entry(&mut self.file, bat_entry_offset, &sector_data)?;

            // Flush log to ensure it's stable
            self.file.flush()?;

            // Apply to BAT
            self.file.seek(SeekFrom::Start(bat_entry_offset))?;
            self.file.write_all(&entry_bytes)?;
            self.file.flush()?;
        } else {
            // No log writer - write BAT entry directly
            let entry_bytes = new_entry.to_bytes();
            self.file.seek(SeekFrom::Start(bat_entry_offset))?;
            self.file.write_all(&entry_bytes)?;
            self.file.flush()?;
        }

        // Update BAT in memory
        self.bat.update_payload_entry(block_idx, new_entry)?;

        self.next_free_offset = aligned_offset + block_size;

        Ok(aligned_offset)
    }

    /// Get virtual disk size
    pub fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }

    /// Get block size from BAT
    pub fn block_size(&self) -> u64 {
        self.bat.block_size
    }

    /// Check if a block is allocated
    pub fn is_block_allocated(&self, block_idx: u64) -> bool {
        match self.bat.get_payload_entry(block_idx) {
            Some(entry) => matches!(entry.state, PayloadBlockState::FullyPresent),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_block_io_new() {
        // Basic test that DynamicBlockIo can be created
        // Full testing would require a real file and BAT setup
    }
}
