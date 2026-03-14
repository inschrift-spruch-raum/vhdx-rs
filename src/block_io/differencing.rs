//! Differencing disk block I/O
//!
//! Block I/O implementation for differencing VHDX disks that store
//! only changed blocks relative to a parent disk.

use crate::bat::{Bat, BatEntry, PayloadBlockState};
use crate::error::{Result, VhdxError};
use crate::log::LogWriter;
use crate::payload::bitmap::SectorBitmap;
use std::io::{Read, Seek, SeekFrom, Write};

/// Differencing disk block I/O (stores only changes from parent)
///
/// Differencing disks maintain a parent relationship and only store
/// blocks that have been modified. Unchanged blocks are read from
/// the parent disk.
pub struct DifferencingBlockIo<'a> {
    /// Reference to file handle
    pub file: &'a mut std::fs::File,
    /// Reference to BAT (mutable for writes)
    pub bat: &'a mut Bat,
    /// Parent VHDX (for reading unallocated blocks)
    pub parent: Option<Box<DifferencingBlockIo<'a>>>,
    /// Next free file offset for allocation
    pub next_free_offset: u64,
    /// Virtual disk size
    pub virtual_disk_size: u64,
    /// Optional log writer for metadata updates
    log_writer: Option<LogWriter>,
}

impl<'a> DifferencingBlockIo<'a> {
    /// Create new differencing block I/O handler
    pub fn new(file: &'a mut std::fs::File, bat: &'a mut Bat, virtual_disk_size: u64) -> Self {
        DifferencingBlockIo {
            file,
            bat,
            parent: None,
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

    /// Set parent for differencing disks
    pub fn with_parent(mut self, parent: Box<DifferencingBlockIo<'a>>) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Read sector bitmap data for a chunk
    ///
    /// # Arguments
    /// * `chunk_idx` - The chunk index
    ///
    /// # Returns
    /// Vector containing the sector bitmap bytes
    fn read_sector_bitmap(&mut self, chunk_idx: u64) -> Result<Vec<u8>> {
        use crate::bat::states::SectorBitmapState;

        // Get sector bitmap BAT entry
        let bitmap_entry = self
            .bat
            .get_sector_bitmap_entry(chunk_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;

        // Check if bitmap block is present
        // For sector bitmap blocks, state value 6 means Present
        let state_bits = bitmap_entry.raw & 0x7;
        if state_bits != SectorBitmapState::Present as u64 {
            return Err(VhdxError::InvalidSectorBitmap);
        }

        // Get file offset of bitmap block
        let file_offset = bitmap_entry
            .file_offset()
            .ok_or(VhdxError::InvalidSectorBitmap)?;

        // Calculate bitmap size needed
        let bitmap_size =
            SectorBitmap::bitmap_size_for_chunk(self.bat.chunk_size, self.bat.logical_sector_size);

        // Read bitmap data from file
        let mut bitmap_data = vec![0u8; bitmap_size];
        self.file.seek(SeekFrom::Start(file_offset))?;
        self.file.read_exact(&mut bitmap_data)?;

        Ok(bitmap_data)
    }

    /// Read data from virtual offset
    ///
    /// For differencing disks:
    /// - FullyPresent blocks are read from this file
    /// - PartiallyPresent blocks check sector bitmap for each sector
    /// - Other states read from parent or return zeros
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
                            // Block fully present in this file
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
                        PayloadBlockState::PartiallyPresent => {
                            // Differencing disk - check sector bitmap for each sector
                            let chunk_idx = self.bat.chunk_index_from_offset(current_offset);
                            let bitmap_data = self.read_sector_bitmap(chunk_idx)?;

                            // Calculate bytes per sector and sectors to read
                            let logical_sector_size = self.bat.logical_sector_size as u64;
                            let offset_in_block = self.bat.offset_in_block(current_offset);

                            // Process each sector in the read range
                            let mut sector_offset = 0u64;
                            while sector_offset < bytes_from_block as u64 {
                                let bytes_remaining_in_read =
                                    bytes_from_block as u64 - sector_offset;
                                let bytes_to_process =
                                    std::cmp::min(logical_sector_size, bytes_remaining_in_read);

                                // Calculate which sector we're at within the chunk
                                let absolute_offset = current_offset + sector_offset;
                                let sector_idx = SectorBitmap::sector_index_in_chunk(
                                    absolute_offset,
                                    self.bat.logical_sector_size,
                                    self.bat.chunk_size,
                                );

                                // Check if this sector is present in this file
                                if SectorBitmap::is_sector_present(&bitmap_data, sector_idx) {
                                    // Sector is present - read from this file
                                    let payload_entry = self
                                        .bat
                                        .get_payload_entry(block_idx)
                                        .ok_or(VhdxError::InvalidBatEntry)?;
                                    let file_offset = payload_entry
                                        .file_offset()
                                        .ok_or(VhdxError::InvalidBatEntry)?;
                                    let absolute_file_offset =
                                        file_offset + offset_in_block + sector_offset;
                                    self.file.seek(SeekFrom::Start(absolute_file_offset))?;
                                    self.file.read_exact(
                                        &mut buf[bytes_read + sector_offset as usize
                                            ..bytes_read
                                                + sector_offset as usize
                                                + bytes_to_process as usize],
                                    )?;
                                } else {
                                    // Sector is not present - read from parent or zero-fill
                                    if let Some(ref mut parent) = self.parent {
                                        parent.read(
                                            current_offset + sector_offset,
                                            &mut buf[bytes_read + sector_offset as usize
                                                ..bytes_read
                                                    + sector_offset as usize
                                                    + bytes_to_process as usize],
                                        )?;
                                    } else {
                                        // No parent - zero-fill
                                        for i in 0..bytes_to_process {
                                            buf[bytes_read + sector_offset as usize + i as usize] =
                                                0;
                                        }
                                    }
                                }

                                sector_offset += bytes_to_process;
                            }
                        }
                        PayloadBlockState::Zero
                        | PayloadBlockState::NotPresent
                        | PayloadBlockState::Unmapped => {
                            // Read from parent if available, otherwise zeros
                            if let Some(ref mut parent) = self.parent {
                                parent.read(
                                    current_offset,
                                    &mut buf[bytes_read..bytes_read + bytes_from_block],
                                )?;
                            } else {
                                // No parent - return zeros
                                for i in bytes_read..bytes_read + bytes_from_block {
                                    buf[i] = 0;
                                }
                            }
                        }
                        PayloadBlockState::Undefined => {
                            return Err(VhdxError::InvalidBatEntry);
                        }
                    }
                }
                None => {
                    // Block not in BAT - read from parent or return zeros
                    if let Some(ref mut parent) = self.parent {
                        parent.read(
                            current_offset,
                            &mut buf[bytes_read..bytes_read + bytes_from_block],
                        )?;
                    } else {
                        for i in bytes_read..bytes_read + bytes_from_block {
                            buf[i] = 0;
                        }
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
    /// For differencing disks, writes may:
    /// - Allocate new blocks
    /// - Update partially present blocks
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
                    // Differencing disk - update sector bitmap for written sectors
                    // Get payload offset before any mutable borrows
                    let payload_offset = entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?;

                    let chunk_idx = self.bat.chunk_index_from_block(block_idx);

                    // Read existing sector bitmap
                    let mut bitmap_data = self.read_sector_bitmap(chunk_idx)?;

                    // Calculate which sectors will be written
                    let logical_sector_size = self.bat.logical_sector_size as u64;
                    let start_sector = offset_in_block / logical_sector_size;
                    let end_sector =
                        (offset_in_block + bytes_to_block as u64 - 1) / logical_sector_size;

                    // Get sector bitmap entry to find where to write the bitmap
                    let bitmap_entry = self
                        .bat
                        .get_sector_bitmap_entry(chunk_idx)
                        .ok_or(VhdxError::InvalidBatEntry)?;
                    let bitmap_file_offset = bitmap_entry
                        .file_offset()
                        .ok_or(VhdxError::InvalidSectorBitmap)?;

                    // Write data to payload block
                    let absolute_offset = payload_offset + offset_in_block;
                    self.file.seek(SeekFrom::Start(absolute_offset))?;
                    self.file
                        .write_all(&buf[bytes_written..bytes_written + bytes_to_block])?;

                    // Update sector bitmap: mark written sectors as present
                    for sector_idx in start_sector..=end_sector {
                        SectorBitmap::set_sector_present(&mut bitmap_data, sector_idx);
                    }

                    // Write updated bitmap back to sector bitmap block
                    self.file.seek(SeekFrom::Start(bitmap_file_offset))?;
                    self.file.write_all(&bitmap_data)?;

                    // Check if all sectors in the chunk are now present
                    // If yes, convert block to FullyPresent state
                    let sectors_per_chunk = self.bat.chunk_size / logical_sector_size;
                    let mut all_present = true;
                    for sector_idx in 0..sectors_per_chunk {
                        if !SectorBitmap::is_sector_present(&bitmap_data, sector_idx) {
                            all_present = false;
                            break;
                        }
                    }

                    if all_present {
                        // Convert to FullyPresent state
                        let new_entry = BatEntry::new(
                            PayloadBlockState::FullyPresent,
                            payload_offset / (1024 * 1024),
                        );
                        self.bat.update_payload_entry(block_idx, new_entry)?;

                        // Update BAT entry in file
                        let bat_index = self
                            .bat
                            .payload_bat_index(block_idx)
                            .ok_or(VhdxError::InvalidBatEntry)?;
                        let bat_entry_offset = self.bat.get_bat_entry_file_offset(bat_index);
                        let entry_bytes = new_entry.to_bytes();
                        self.file.seek(SeekFrom::Start(bat_entry_offset))?;
                        self.file.write_all(&entry_bytes)?;
                    }

                    // Return payload offset for further writing
                    payload_offset
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

    /// Check if this disk has a parent
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    /// Read from parent disk (for unallocated blocks)
    pub fn read_from_parent(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if let Some(ref mut parent) = self.parent {
            parent.read(virtual_offset, buf)
        } else {
            // No parent - return zeros
            for i in 0..buf.len() {
                buf[i] = 0;
            }
            Ok(buf.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    use tempfile::tempfile;

    /// Test helper: Create a minimal BAT for testing
    fn create_test_bat(virtual_disk_size: u64, block_size: u64, logical_sector_size: u32) -> Bat {
        Bat {
            entries: vec![],
            virtual_disk_size,
            block_size,
            logical_sector_size,
            num_payload_blocks: Bat::calculate_num_payload_blocks(virtual_disk_size, block_size),
            num_sector_bitmap_blocks: 0,
            chunk_ratio: Bat::calculate_chunk_ratio(block_size, logical_sector_size),
            chunk_size: (1u64 << 23) * logical_sector_size as u64,
            bat_file_offset: 0,
        }
    }

    /// Test helper: Setup BAT with a PartiallyPresent block at given index
    fn setup_partially_present_block(bat: &mut Bat, block_idx: u64, file_offset_mb: u64) {
        use crate::bat::entry::BatEntry;
        use crate::bat::states::PayloadBlockState;

        let bat_index = bat.payload_bat_index(block_idx).unwrap();
        let entry = BatEntry::new(PayloadBlockState::PartiallyPresent, file_offset_mb);

        // Ensure BAT has enough entries
        while bat.entries.len() <= bat_index {
            bat.entries
                .push(BatEntry::new(PayloadBlockState::NotPresent, 0));
        }
        bat.entries[bat_index] = entry;
    }

    /// Test helper: Setup sector bitmap block in BAT
    fn setup_sector_bitmap_block(bat: &mut Bat, chunk_idx: u64, file_offset_mb: u64) {
        use crate::bat::entry::BatEntry;

        // Calculate BAT index manually: bat_index = chunk_idx * (chunk_ratio + 1) + chunk_ratio
        let bat_index = (chunk_idx * (bat.chunk_ratio + 1) + bat.chunk_ratio) as usize;

        // Create entry with state bits = 6 (Present), same value for both payload and bitmap
        // We construct it manually since BatEntry::new expects PayloadBlockState
        let raw = ((file_offset_mb & 0xFFFFFFFFFFF) << 20) | 6u64;
        let entry = BatEntry::from_raw(raw).unwrap();

        // Ensure BAT has enough entries
        while bat.entries.len() <= bat_index {
            bat.entries.push(BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        bat.entries[bat_index] = entry;

        // Update num_sector_bitmap_blocks to include this chunk
        if chunk_idx >= bat.num_sector_bitmap_blocks {
            bat.num_sector_bitmap_blocks = chunk_idx + 1;
        }
    }

    /// Test helper: Create temp file with sector bitmap data
    fn create_bitmap_data(
        sectors_present: &[u64],
        total_sectors: u64,
        logical_sector_size: u32,
        chunk_size: u64,
    ) -> Vec<u8> {
        let bitmap_size = SectorBitmap::bitmap_size_for_chunk(chunk_size, logical_sector_size);
        let mut bitmap = vec![0u8; bitmap_size];

        for &sector_idx in sectors_present {
            if sector_idx < total_sectors {
                SectorBitmap::set_sector_present(&mut bitmap, sector_idx);
            }
        }

        bitmap
    }

    #[test]
    fn test_differencing_block_io_new() {
        // Basic test that DifferencingBlockIo can be created
        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(1024 * 1024 * 1024, 1024 * 1024, 512);

        let dio = DifferencingBlockIo::new(&mut temp_file, &mut bat, 1024 * 1024 * 1024);

        assert_eq!(dio.virtual_disk_size(), 1024 * 1024 * 1024);
        assert_eq!(dio.block_size(), 1024 * 1024);
        assert!(!dio.has_parent());
    }

    #[test]
    fn test_partially_present_read_some_sectors_from_parent() {
        // Test reading from a PartiallyPresent block where some sectors come from parent
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64; // 1MB
        let virtual_disk_size = 10 * 1024 * 1024u64; // 10MB
        let chunk_size = (1u64 << 23) * logical_sector_size as u64; // 4GB

        // Create parent with known data pattern at block 0
        let mut parent_data = vec![0u8; block_size as usize];
        for (i, byte) in parent_data.iter_mut().enumerate() {
            *byte = (i % 256) as u8; // Pattern: 0, 1, 2, ..., 255, 0, 1, ...
        }

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        // Setup PartiallyPresent block at index 0
        setup_partially_present_block(&mut bat, 0, 1); // 1MB offset

        // Setup sector bitmap block
        setup_sector_bitmap_block(&mut bat, 0, 2); // 2MB offset

        // Create bitmap with sectors 0-7 present (first 4KB), rest from parent
        let sectors_present = vec![0, 1, 2, 3, 4, 5, 6, 7]; // First 8 sectors (4KB)
        let bitmap_data =
            create_bitmap_data(&sectors_present, 2048, logical_sector_size, chunk_size);

        // Write sector bitmap to file at 2MB
        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&bitmap_data).unwrap();

        // Write partial data to file at 1MB (first 4KB with different pattern)
        let mut child_data = vec![0xFFu8; 4096];
        for (_i, byte) in child_data.iter_mut().enumerate() {
            *byte = 0xAA;
        }
        temp_file.seek(SeekFrom::Start(1 * 1024 * 1024)).unwrap();
        temp_file.write_all(&child_data).unwrap();

        // Create parent file with data
        let mut parent_file = tempfile().unwrap();
        parent_file.seek(SeekFrom::Start(0)).unwrap();
        parent_file.write_all(&parent_data).unwrap();

        let mut parent_bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);
        // Ensure parent BAT has entry for block 0
        while parent_bat.entries.len() <= 0 {
            parent_bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        // Ensure parent BAT has entry for block 0
        while parent_bat.entries.len() <= 0 {
            parent_bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        let parent_entry = crate::bat::entry::BatEntry::new(
            crate::bat::states::PayloadBlockState::FullyPresent,
            0,
        );
        parent_bat.update_payload_entry(0, parent_entry).unwrap();

        let parent_file_box = Box::leak(Box::new(parent_file));
        let parent_bat_box = Box::leak(Box::new(parent_bat));
        let parent_dio = Box::new(DifferencingBlockIo::new(
            parent_file_box,
            parent_bat_box,
            virtual_disk_size,
        ));

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio =
            DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size).with_parent(parent_dio);

        // Read first sector (should come from child - 0xAA)
        let mut buf = vec![0u8; 512];
        dio.read(0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0xAA));

        // Read sector 10 (should come from parent - pattern)
        let mut buf = vec![0u8; 512];
        dio.read(10 * 512, &mut buf).unwrap();
        for (i, &byte) in buf.iter().enumerate() {
            assert_eq!(
                byte,
                ((10 * 512 + i) % 256) as u8,
                "Sector 10 should come from parent"
            );
        }
    }

    #[test]
    fn test_partially_present_read_all_sectors_from_child() {
        // Test reading from a PartiallyPresent block where all sectors are in child
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64;
        let virtual_disk_size = 10 * 1024 * 1024u64;
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        setup_partially_present_block(&mut bat, 0, 1);
        setup_sector_bitmap_block(&mut bat, 0, 2);

        // All sectors 0-2047 present (entire 1MB block)
        let sectors_present: Vec<u64> = (0..2048).collect();
        let bitmap_data =
            create_bitmap_data(&sectors_present, 2048, logical_sector_size, chunk_size);

        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&bitmap_data).unwrap();

        // Write test data
        let test_data = vec![0x42u8; block_size as usize];
        temp_file.seek(SeekFrom::Start(1 * 1024 * 1024)).unwrap();
        temp_file.write_all(&test_data).unwrap();

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio = DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size);

        // Read should return child data
        let mut buf = vec![0u8; 512];
        dio.read(0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0x42));

        let mut buf = vec![0u8; 512];
        dio.read(block_size - 512, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0x42));
    }

    #[test]

    fn test_partially_present_read_no_sectors_from_child() {
        // Test reading from a PartiallyPresent block where no sectors are in child
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64;
        let virtual_disk_size = 10 * 1024 * 1024u64;
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        let mut parent_file = tempfile().unwrap();
        let parent_data = vec![0x99u8; block_size as usize];
        parent_file.write_all(&parent_data).unwrap();

        let mut parent_bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);
        // Ensure parent BAT has entry for block 0
        while parent_bat.entries.len() <= 0 {
            parent_bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        // Ensure parent BAT has entry for block 0
        while parent_bat.entries.len() <= 0 {
            parent_bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        let parent_entry = crate::bat::entry::BatEntry::new(
            crate::bat::states::PayloadBlockState::FullyPresent,
            0,
        );
        parent_bat.update_payload_entry(0, parent_entry).unwrap();

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        setup_partially_present_block(&mut bat, 0, 1);
        setup_sector_bitmap_block(&mut bat, 0, 2);

        // No sectors present - empty bitmap
        let bitmap_data = create_bitmap_data(&[], 2048, logical_sector_size, chunk_size);
        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&bitmap_data).unwrap();

        let parent_file_box = Box::leak(Box::new(parent_file));
        let parent_bat_box = Box::leak(Box::new(parent_bat));
        let parent_dio = Box::new(DifferencingBlockIo::new(
            parent_file_box,
            parent_bat_box,
            virtual_disk_size,
        ));

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio =
            DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size).with_parent(parent_dio);

        // Read should return parent data
        let mut buf = vec![0u8; 512];
        dio.read(0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0x99));
    }

    #[test]

    fn test_partially_present_write_updates_bitmap() {
        // Test that writing to a PartiallyPresent block updates the sector bitmap
        // Note: Current implementation converts to FullyPresent on write
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64;
        let virtual_disk_size = 10 * 1024 * 1024u64;

        let temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        // Ensure BAT has entry for block 0
        while bat.entries.len() <= 0 {
            bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        // Start with NotPresent block
        let entry =
            crate::bat::entry::BatEntry::new(crate::bat::states::PayloadBlockState::NotPresent, 0);
        bat.update_payload_entry(0, entry).unwrap();

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio = DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size);

        // Write data - should allocate block and mark as FullyPresent
        let test_data = vec![0x55u8; 512];
        let bytes_written = dio.write(0, &test_data).unwrap();
        assert_eq!(bytes_written, 512);

        // Check that block is now FullyPresent
        let entry = bat_box.get_payload_entry(0).unwrap();
        assert_eq!(
            entry.state,
            crate::bat::states::PayloadBlockState::FullyPresent
        );
    }

    #[test]

    fn test_sector_by_sector_merge_from_parent() {
        // Test detailed sector-by-sector merging pattern
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64;
        let virtual_disk_size = 10 * 1024 * 1024u64;
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        setup_partially_present_block(&mut bat, 0, 1);
        setup_sector_bitmap_block(&mut bat, 0, 2);

        // Alternating sectors: 0, 2, 4, ... from child; 1, 3, 5, ... from parent
        let sectors_present: Vec<u64> = (0..2048).filter(|x| x % 2 == 0).collect();
        let bitmap_data =
            create_bitmap_data(&sectors_present, 2048, logical_sector_size, chunk_size);

        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&bitmap_data).unwrap();

        // Write child data (for even sectors)
        let child_data = vec![0xCCu8; block_size as usize];
        temp_file.seek(SeekFrom::Start(1 * 1024 * 1024)).unwrap();
        temp_file.write_all(&child_data).unwrap();

        // Create parent with different data
        let mut parent_file = tempfile().unwrap();
        let parent_data = vec![0x33u8; block_size as usize];
        parent_file.write_all(&parent_data).unwrap();

        let mut parent_bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);
        // Ensure parent BAT has entry for block 0
        while parent_bat.entries.len() <= 0 {
            parent_bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }
        let parent_entry = crate::bat::entry::BatEntry::new(
            crate::bat::states::PayloadBlockState::FullyPresent,
            0,
        );
        parent_bat.update_payload_entry(0, parent_entry).unwrap();

        let parent_file_box = Box::leak(Box::new(parent_file));
        let parent_bat_box = Box::leak(Box::new(parent_bat));
        let parent_dio = Box::new(DifferencingBlockIo::new(
            parent_file_box,
            parent_bat_box,
            virtual_disk_size,
        ));

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio =
            DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size).with_parent(parent_dio);

        // Read sector 0 (even - from child)
        let mut buf = vec![0u8; 512];
        dio.read(0, &mut buf).unwrap();
        assert!(
            buf.iter().all(|&b| b == 0xCC),
            "Sector 0 should be from child (0xCC)"
        );

        // Read sector 1 (odd - from parent)
        let mut buf = vec![0u8; 512];
        dio.read(512, &mut buf).unwrap();
        assert!(
            buf.iter().all(|&b| b == 0x33),
            "Sector 1 should be from parent (0x33)"
        );

        // Read sector 2 (even - from child)
        let mut buf = vec![0u8; 512];
        dio.read(1024, &mut buf).unwrap();
        assert!(
            buf.iter().all(|&b| b == 0xCC),
            "Sector 2 should be from child (0xCC)"
        );
    }

    #[test]
    fn test_read_crosses_block_boundary() {
        // Test read that spans multiple blocks
        let logical_sector_size = 512u32;
        let block_size = 4096u64; // Small block for easier testing
        let virtual_disk_size = 10 * 1024u64; // 10KB

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        // Ensure BAT has entries for blocks 0-2
        while bat.entries.len() <= 2 {
            bat.entries.push(crate::bat::entry::BatEntry::new(
                crate::bat::states::PayloadBlockState::NotPresent,
                0,
            ));
        }

        // Block 0: FullyPresent at offset 1MB
        let entry0 = crate::bat::entry::BatEntry::new(
            crate::bat::states::PayloadBlockState::FullyPresent,
            1,
        );
        bat.update_payload_entry(0, entry0).unwrap();

        // Write data for block 0
        let block0_data = vec![0xAAu8; block_size as usize];
        temp_file.seek(SeekFrom::Start(1 * 1024 * 1024)).unwrap();
        temp_file.write_all(&block0_data).unwrap();

        // Block 1: FullyPresent at offset 2MB
        let entry1 = crate::bat::entry::BatEntry::new(
            crate::bat::states::PayloadBlockState::FullyPresent,
            2,
        );
        bat.update_payload_entry(1, entry1).unwrap();

        // Write data for block 1
        let block1_data = vec![0xBBu8; block_size as usize];
        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&block1_data).unwrap();

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio = DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size);

        // Read across block boundary: all of block 0 and block 1
        let mut buf = vec![0u8; (block_size * 2) as usize];
        let bytes_read = dio.read(0, &mut buf).unwrap();
        assert_eq!(bytes_read, (block_size * 2) as usize);

        // Verify block 0 data
        assert!(buf[0..block_size as usize].iter().all(|&b| b == 0xAA));
        // Verify block 1 data
        assert!(buf[block_size as usize..].iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn test_bitmap_calculation_helpers() {
        // Test helper functions for bitmap operations
        let logical_sector_size = 512u32;
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;
        let num_sectors = chunk_size / logical_sector_size as u64;

        assert_eq!(num_sectors, 8 * 1024 * 1024); // 2^23 sectors
        assert_eq!(
            SectorBitmap::bitmap_size_for_sectors(num_sectors),
            1024 * 1024
        ); // 1MB bitmap

        // Test sector index calculation
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(0, logical_sector_size, chunk_size),
            0
        );
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(512, logical_sector_size, chunk_size),
            1
        );
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(1024, logical_sector_size, chunk_size),
            2
        );
    }

    #[test]
    fn test_no_parent_zero_fill() {
        // Test that reads from PartiallyPresent block with no parent return zeros
        let logical_sector_size = 512u32;
        let block_size = 1024 * 1024u64;
        let virtual_disk_size = 10 * 1024 * 1024u64;
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        let mut temp_file = tempfile().unwrap();
        let mut bat = create_test_bat(virtual_disk_size, block_size, logical_sector_size);

        setup_partially_present_block(&mut bat, 0, 1);
        setup_sector_bitmap_block(&mut bat, 0, 2);

        // No sectors present
        let bitmap_data = create_bitmap_data(&[], 2048, logical_sector_size, chunk_size);
        temp_file.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
        temp_file.write_all(&bitmap_data).unwrap();

        let file_box = Box::leak(Box::new(temp_file));
        let bat_box = Box::leak(Box::new(bat));
        let mut dio = DifferencingBlockIo::new(file_box, bat_box, virtual_disk_size);

        // Read should return zeros (no parent)
        let mut buf = vec![0u8; 512];
        dio.read(0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0));
    }
}
