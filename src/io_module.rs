//! IO module for sector-level operations

use crate::error::{Error, Result};
use crate::File;
use crate::PayloadBlockState;

/// IO module for sector-level read/write operations
pub struct IO<'a> {
    file: &'a File,
}

impl<'a> IO<'a> {
    /// Create a new IO instance
    pub fn new(file: &'a File) -> Self {
        Self { file }
    }

    /// Get a sector by global sector number
    ///
    /// Internally:
    /// 1. Calculates which block the sector is in
    /// 2. Looks up the block in BAT
    /// 3. Returns a Sector handle
    pub fn sector(&self, sector: u64) -> Option<Sector<'a>> {
        let sector_size = self.file.logical_sector_size() as u64;
        let block_size = self.file.block_size() as u64;

        // Calculate block index and sector index within block
        let sectors_per_block = block_size / sector_size;
        let block_idx = sector / sectors_per_block;
        let block_sector_idx = (sector % sectors_per_block) as u32;

        // Check if sector is within virtual disk bounds
        let total_sectors = self.file.virtual_disk_size() / sector_size;
        if sector >= total_sectors {
            return None;
        }

        Some(Sector {
            file: self.file,
            block_idx,
            block_sector_idx,
            sector_size: self.file.logical_sector_size(),
        })
    }

    /// Read sectors starting at the given sector number
    pub fn read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let num_sectors = buf.len() / sector_size;

        if buf.len() % sector_size != 0 {
            return Err(Error::InvalidParameter(
                "Buffer size must be a multiple of sector size".to_string(),
            ));
        }

        let mut total_read = 0;
        for i in 0..num_sectors {
            let sector_num = start_sector + i as u64;
            if let Some(sector) = self.sector(sector_num) {
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                let bytes_read = sector.read(sector_buf)?;
                total_read += bytes_read;
            } else {
                // Sector out of bounds - fill with zeros
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                for j in 0..sector_buf.len() {
                    sector_buf[j] = 0;
                }
                total_read += sector_size;
            }
        }

        Ok(total_read)
    }

    /// Write sectors starting at the given sector number
    pub fn write_sectors(&self, start_sector: u64, data: &[u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let num_sectors = data.len() / sector_size;

        if data.len() % sector_size != 0 {
            return Err(Error::InvalidParameter(
                "Data size must be a multiple of sector size".to_string(),
            ));
        }

        // This would need to be implemented with proper mutable access
        // For now, return an error
        Err(Error::InvalidParameter(
            "IO::write_sectors requires mutable access (not yet fully implemented)".to_string(),
        ))
    }
}

/// Sector-level access
///
/// Wraps a PayloadBlock reference and sector index within the block
pub struct Sector<'a> {
    file: &'a File,
    block_idx: u64,
    block_sector_idx: u32,
    sector_size: u32,
}

impl<'a> Sector<'a> {
    /// Get the block index
    pub fn block_idx(&self) -> u64 {
        self.block_idx
    }

    /// Get the sector index within the block
    pub fn block_sector_idx(&self) -> u32 {
        self.block_sector_idx
    }

    /// Get the global sector number
    pub fn global_sector(&self) -> u64 {
        let sectors_per_block = (self.file.block_size() / self.sector_size) as u64;
        self.block_idx * sectors_per_block + self.block_sector_idx as u64
    }

    /// Read sector data
    ///
    /// `buf` length must be equal to sector size
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() != self.sector_size as usize {
            return Err(Error::InvalidParameter(format!(
                "Buffer size {} does not match sector size {}",
                buf.len(),
                self.sector_size
            )));
        }

        let sector_offset = self.global_sector() * self.sector_size as u64;
        self.file.read(sector_offset, buf)
    }

    /// Get the corresponding PayloadBlock
    pub fn payload(&self) -> PayloadBlock<'_> {
        PayloadBlock {
            file: self.file,
            block_idx: self.block_idx,
        }
    }
}

/// Payload Block
///
/// Internal structure - users access through Sector, not directly
pub struct PayloadBlock<'a> {
    file: &'a File,
    block_idx: u64,
}

impl<'a> PayloadBlock<'a> {
    /// Get the block index
    pub fn block_idx(&self) -> u64 {
        self.block_idx
    }

    /// Read data from the block
    /// `offset` is relative to the start of the block
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let block_size = self.file.block_size() as u64;
        if offset >= block_size {
            return Ok(0);
        }

        let block_offset = self.block_idx * block_size + offset;
        self.file.read(block_offset, buf)
    }

    /// Get the BAT entry for this block
    pub fn bat_entry(&self) -> Option<crate::BatEntry> {
        if let Ok(bat) = self.file.sections().bat() {
            bat.entry(self.block_idx)
        } else {
            None
        }
    }

    /// Check if the block is allocated
    pub fn is_allocated(&self) -> bool {
        if let Some(entry) = self.bat_entry() {
            matches!(
                entry.state,
                crate::BatState::Payload(PayloadBlockState::FullyPresent)
            )
        } else {
            false
        }
    }
}
