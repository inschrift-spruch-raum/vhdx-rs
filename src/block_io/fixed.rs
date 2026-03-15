//! Fixed disk block I/O
//!
//! Optimized block I/O implementation for fixed-size VHDX disks
//! where all blocks are pre-allocated.

use crate::bat::Bat;
use crate::error::{Result, VhdxError};
use std::io::{Read, Seek, SeekFrom, Write};

/// Fixed VHDX data offset from the start of file
/// According to MS-VHDX spec, fixed disk data starts at 0x00400000 (4MB)
const FIXED_DATA_OFFSET: u64 = 0x00400000;

/// Fixed disk block I/O (optimized for pre-allocated blocks)
///
/// For fixed disks, all data blocks are pre-allocated at creation time,
/// so there's no need for dynamic allocation during I/O operations.
///
/// Windows-created fixed VHDX files have an empty BAT table (all entries are 0/NotPresent),
/// so we use a fixed offset formula instead of BAT lookups.
pub struct FixedBlockIo<'a> {
    /// Reference to file handle
    pub file: &'a mut std::fs::File,
    /// Reference to BAT (for backward compatibility with existing fixed VHDX files)
    pub bat: &'a Bat,
    /// Virtual disk size
    pub virtual_disk_size: u64,
    /// Whether to use BAT-based offset calculation (for legacy compatibility)
    use_bat: bool,
}

impl<'a> FixedBlockIo<'a> {
    /// Create new fixed block I/O handler
    ///
    /// Automatically detects whether to use BAT-based or fixed-offset I/O
    /// based on whether the first BAT entry is valid.
    pub fn new(file: &'a mut std::fs::File, bat: &'a Bat, virtual_disk_size: u64) -> Self {
        // Check if BAT has valid entries (for backward compatibility)
        // If BAT[0] has a valid file offset, use BAT-based I/O
        // Otherwise, use fixed offset formula
        let use_bat = bat
            .get_payload_entry(0)
            .and_then(|entry| entry.file_offset())
            .is_some();

        FixedBlockIo {
            file,
            bat,
            virtual_disk_size,
            use_bat,
        }
    }

    /// Create new fixed block I/O handler with explicit mode
    ///
    /// # Arguments
    /// * `file` - File handle
    /// * `bat` - BAT reference (used for block size info)
    /// * `virtual_disk_size` - Virtual disk size
    /// * `use_bat` - If true, use BAT-based offset calculation (legacy mode)
    pub fn with_mode(
        file: &'a mut std::fs::File,
        bat: &'a Bat,
        virtual_disk_size: u64,
        use_bat: bool,
    ) -> Self {
        FixedBlockIo {
            file,
            bat,
            virtual_disk_size,
            use_bat,
        }
    }

    /// Calculate file offset for a given virtual offset
    ///
    /// For modern fixed VHDX (Windows-created): file_offset = 0x00400000 + virtual_offset
    /// For legacy fixed VHDX (with valid BAT): use BAT entry offset
    fn calculate_file_offset(&self, virtual_offset: u64) -> Result<u64> {
        if self.use_bat {
            // Legacy mode: use BAT to find file offset
            let block_idx = self.bat.block_index_from_offset(virtual_offset);
            let offset_in_block = self.bat.offset_in_block(virtual_offset);

            let entry = self
                .bat
                .get_payload_entry(block_idx)
                .ok_or(VhdxError::InvalidBatEntry)?;

            let file_offset = entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?;
            Ok(file_offset + offset_in_block)
        } else {
            // Modern mode: use fixed offset formula
            Ok(FIXED_DATA_OFFSET + virtual_offset)
        }
    }

    /// Read data from virtual offset
    ///
    /// For fixed disks, data is stored at fixed locations:
    /// - Windows-created: file_offset = 0x00400000 + virtual_offset
    /// - Legacy with BAT: file_offset = BAT[block_idx].offset + offset_in_block
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_read =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        let absolute_offset = self.calculate_file_offset(virtual_offset)?;

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

        let absolute_offset = self.calculate_file_offset(virtual_offset)?;

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

    /// Check if using BAT-based offset calculation
    pub fn uses_bat(&self) -> bool {
        self.use_bat
    }
}
