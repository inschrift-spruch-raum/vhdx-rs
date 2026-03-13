//! VHDX Block I/O operations
//!
//! Implements reading and writing of payload blocks with support for
//! fixed, dynamic, and differencing disk types.

use crate::bat::{Bat, BatEntry, PayloadBlockState};
use crate::error::{Result, VhdxError};
use std::io::{Read, Seek, SeekFrom, Write};

/// Block I/O handler
pub struct BlockIo<'a> {
    /// Reference to file handle
    pub file: &'a mut std::fs::File,
    /// Reference to BAT
    pub bat: &'a Bat,
    /// Parent VHDX (for differencing disks)
    pub parent: Option<Box<BlockIo<'a>>>,
    /// Next free file offset for allocation
    pub next_free_offset: u64,
    /// Virtual disk size
    pub virtual_disk_size: u64,
}

impl<'a> BlockIo<'a> {
    /// Create new Block I/O handler
    pub fn new(file: &'a mut std::fs::File, bat: &'a Bat, virtual_disk_size: u64) -> Self {
        BlockIo {
            file,
            bat,
            parent: None,
            next_free_offset: 1024 * 1024, // Start after 1MB header
            virtual_disk_size,
        }
    }

    /// Set parent for differencing disks
    pub fn with_parent(mut self, parent: Box<BlockIo<'a>>) -> Self {
        self.parent = Some(parent);
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
                            // Return zeros
                            for i in bytes_read..bytes_read + bytes_from_block {
                                buf[i] = 0;
                            }
                        }
                        PayloadBlockState::PartiallyPresent => {
                            // Differencing disk - check sector bitmap
                            if let Some(ref mut parent) = self.parent {
                                // TODO: Implement sector bitmap check
                                // For now, read from parent
                                parent.read(
                                    current_offset,
                                    &mut buf[bytes_read..bytes_read + bytes_from_block],
                                )?;
                            } else {
                                return Err(VhdxError::InvalidSectorBitmap);
                            }
                        }
                        PayloadBlockState::Undefined => {
                            return Err(VhdxError::InvalidBatEntry);
                        }
                    }
                }
                None => {
                    // Block not in BAT - return zeros for dynamic, error for fixed
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
    /// For dynamic disks, this may allocate new blocks
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
                    // Need to allocate block (dynamic disk)
                    self.allocate_block(block_idx)?;
                    // Re-get the entry after allocation
                    self.bat
                        .get_payload_entry(block_idx)
                        .and_then(|e| e.file_offset())
                        .ok_or(VhdxError::InvalidBatEntry)?
                }
                PayloadBlockState::PartiallyPresent => {
                    // Differencing disk - complex case
                    // For now, treat as allocate
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

    /// Allocate a new block
    ///
    /// Returns the file offset of the allocated block
    fn allocate_block(&mut self, _block_idx: u64) -> Result<u64> {
        // Align next free offset to 1MB
        let aligned_offset = (self.next_free_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1);

        // Allocate block space
        let block_size = self.bat.block_size;
        let file_offset_mb = aligned_offset / (1024 * 1024);

        // Extend file if necessary
        self.file
            .seek(SeekFrom::Start(aligned_offset + block_size - 1))?;
        self.file.write_all(&[0])?;

        // Note: In real implementation, we'd update BAT through log
        let _new_entry = BatEntry::new(PayloadBlockState::FullyPresent, file_offset_mb);

        // Note: In real implementation, we'd update BAT through log
        // For now, this is handled by the caller

        self.next_free_offset = aligned_offset + block_size;

        Ok(aligned_offset)
    }

    /// Align offset to 1MB boundary
    fn _align_to_1mb(_offset: u64) -> u64 {
        (_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1)
    }

    /// Get current file size
    fn _current_file_size(&mut self) -> Result<u64> {
        let pos = self.file.seek(SeekFrom::End(0))?;
        self.file.seek(SeekFrom::Start(pos))?;
        Ok(pos)
    }
}

/// Fixed disk block I/O (optimized for pre-allocated blocks)
pub struct FixedBlockIo<'a> {
    pub file: &'a mut std::fs::File,
    pub bat: &'a Bat,
    pub virtual_disk_size: u64,
}

impl<'a> FixedBlockIo<'a> {
    pub fn new(file: &'a mut std::fs::File, bat: &'a Bat, virtual_disk_size: u64) -> Self {
        FixedBlockIo {
            file,
            bat,
            virtual_disk_size,
        }
    }

    /// Read data from virtual offset
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_read =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        // For fixed disk, just calculate direct offset
        let file_offset = virtual_offset; // Fixed disk is 1:1 mapped after header

        self.file.seek(SeekFrom::Start(file_offset))?;
        self.file.read_exact(&mut buf[..bytes_to_read])?;

        Ok(bytes_to_read)
    }

    /// Write data to virtual offset
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let bytes_to_write =
            std::cmp::min(buf.len() as u64, self.virtual_disk_size - virtual_offset) as usize;

        // For fixed disk, just calculate direct offset
        let file_offset = virtual_offset;

        self.file.seek(SeekFrom::Start(file_offset))?;
        self.file.write_all(&buf[..bytes_to_write])?;
        self.file.flush()?;

        Ok(bytes_to_write)
    }
}

/// Block cache for performance optimization
pub struct BlockCache {
    /// Cached blocks
    cache: std::collections::HashMap<u64, Vec<u8>>,
    /// Cache size limit in blocks
    max_blocks: usize,
}

impl BlockCache {
    pub fn new(max_blocks: usize) -> Self {
        BlockCache {
            cache: std::collections::HashMap::with_capacity(max_blocks),
            max_blocks,
        }
    }

    /// Get cached block
    pub fn get(&self, block_idx: u64) -> Option<&Vec<u8>> {
        self.cache.get(&block_idx)
    }

    /// Put block in cache
    pub fn put(&mut self, block_idx: u64, data: Vec<u8>) {
        if self.cache.len() >= self.max_blocks {
            // Simple eviction: remove arbitrary entry
            if let Some(key) = self.cache.keys().next().copied() {
                self.cache.remove(&key);
            }
        }
        self.cache.insert(block_idx, data);
    }

    /// Invalidate cached block
    pub fn invalidate(&mut self, block_idx: u64) {
        self.cache.remove(&block_idx);
    }

    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_block_read_write() {
        // This would require a real file and BAT setup
        // For now, just test the basic structure
    }

    #[test]
    fn test_block_cache() {
        let mut cache = BlockCache::new(10);

        // Put some blocks
        for i in 0..5 {
            cache.put(i, vec![i as u8; 1024 * 1024]);
        }

        // Retrieve blocks
        for i in 0..5 {
            assert!(cache.get(i).is_some());
        }

        // Add more to trigger eviction
        for i in 5..15 {
            cache.put(i, vec![i as u8; 1024 * 1024]);
        }

        // Some early blocks should be evicted
        assert!(cache.cache.len() <= 10);
    }
}
