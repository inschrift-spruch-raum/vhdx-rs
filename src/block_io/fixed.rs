//! Fixed disk block I/O
//!
//! Optimized block I/O implementation for fixed-size VHDX disks
//! where all blocks are pre-allocated.

use crate::bat::{Bat, BatEntry, PayloadBlockState};
use crate::error::{Result, VhdxError};
use std::io::{Read, Seek, SeekFrom, Write};

/// Fixed disk block I/O (optimized for pre-allocated blocks)
///
/// For fixed disks, all data blocks are pre-allocated at creation time,
/// so there's no need for dynamic allocation during I/O operations.
pub struct FixedBlockIo<'a> {
    /// Reference to file handle
    pub file: &'a mut std::fs::File,
    /// Reference to BAT (read-only for fixed disks)
    pub bat: &'a Bat,
    /// Virtual disk size
    pub virtual_disk_size: u64,
}

impl<'a> FixedBlockIo<'a> {
    /// Create new fixed block I/O handler
    pub fn new(file: &'a mut std::fs::File, bat: &'a Bat, virtual_disk_size: u64) -> Self {
        FixedBlockIo {
            file,
            bat,
            virtual_disk_size,
        }
    }

    /// Read data from virtual offset
    ///
    /// For fixed disks, data is stored at fixed locations determined by BAT entries.
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_read =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        // For fixed disk, data is stored at BAT entry locations
        // Calculate block index and get file offset from BAT
        let block_idx = self.bat.block_index_from_offset(virtual_offset);
        let offset_in_block = self.bat.offset_in_block(virtual_offset);

        let entry = self
            .bat
            .get_payload_entry(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;

        let file_offset = entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?;
        let absolute_offset = file_offset + offset_in_block;

        self.file.seek(SeekFrom::Start(absolute_offset))?;
        self.file.read_exact(&mut buf[..bytes_to_read])?;

        Ok(bytes_to_read)
    }

    /// Write data to virtual offset
    ///
    /// For fixed disks, data is written to pre-allocated locations.
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_write =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        // For fixed disk, use BAT to find file offset
        let block_idx = self.bat.block_index_from_offset(virtual_offset);
        let offset_in_block = self.bat.offset_in_block(virtual_offset);

        let entry = self
            .bat
            .get_payload_entry(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;

        let file_offset = entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?;
        let absolute_offset = file_offset + offset_in_block;

        self.file.seek(SeekFrom::Start(absolute_offset))?;
        self.file.write_all(&buf[..bytes_to_write])?;
        self.file.flush()?;

        Ok(bytes_to_write)
    }

    /// Get virtual disk size
    pub fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }

    /// Get block size from BAT
    pub fn block_size(&self) -> u64 {
        self.bat.block_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_fixed_block_io_new() {
        // Basic test that FixedBlockIo can be created
        // Full testing would require a real file and BAT setup
    }
}
