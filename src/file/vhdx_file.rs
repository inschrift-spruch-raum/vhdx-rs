//! VHDX File handle for open files
//!
//! Provides file-level operations like open, read, write, and metadata queries.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use byteorder::LittleEndian;

use crate::bat::Bat;
use crate::block_io::{DynamicBlockIo, FixedBlockIo};
use crate::common::Guid;
use crate::error::{Result, VhdxError};
use crate::header::{
    read_headers, read_region_tables, update_headers, FileTypeIdentifier, RegionTable, VhdxHeader,
};
use crate::log::LogReplayer;
use crate::metadata::MetadataRegion;

use super::DiskType;

/// VHDX file handle
pub struct VhdxFile {
    /// Underlying file
    pub(crate) file: File,
    /// File path
    pub(crate) path: std::path::PathBuf,
    /// File type identifier
    pub(crate) file_type: FileTypeIdentifier,
    /// Current header (index 0 or 1)
    pub(crate) header: VhdxHeader,
    /// Region table
    pub(crate) region_table: RegionTable,
    /// Metadata region
    pub(crate) metadata: MetadataRegion,
    /// Block Allocation Table
    pub(crate) bat: Bat,
    /// Disk type
    pub(crate) disk_type: DiskType,
    /// Virtual disk size
    pub(crate) virtual_disk_size: u64,
    /// Block size
    pub(crate) block_size: u32,
    /// Logical sector size
    pub(crate) logical_sector_size: u32,
    /// Physical sector size
    pub(crate) physical_sector_size: u32,
    /// Virtual disk ID
    pub(crate) virtual_disk_id: Guid,
    /// Current sequence number for header updates
    pub(crate) sequence_number: u64,
    /// Is file open in read-only mode
    pub(crate) read_only: bool,
    /// Parent file (for differencing disks)
    pub(crate) parent: Option<Box<VhdxFile>>,
    /// Log writer for metadata updates
    pub(crate) log_writer: Option<crate::log::LogWriter>,
}

impl VhdxFile {
    /// Open an existing VHDX file
    ///
    /// This will replay the log if necessary.
    pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = if read_only {
            File::open(&path)?
        } else {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)?
        };

        // Read file type identifier
        let mut ft_data = vec![0u8; FileTypeIdentifier::SIZE];
        file.read_exact(&mut ft_data)?;
        let file_type = FileTypeIdentifier::from_bytes(&ft_data)?;

        // Read headers and determine current one
        let (_header_idx, mut header, _) = read_headers(&mut file)?;

        // Store sequence number before moving header
        let sequence_number = header.sequence_number;

        // Verify version
        header.check_version()?;

        // Read region tables
        let (region_table, _) = read_region_tables(&mut file)?;

        // Replay log if needed
        if !header.log_guid.is_zero() {
            Self::replay_log(&mut file, &mut header, read_only)?;
        }

        // Read metadata region
        let metadata_entry = region_table
            .find_metadata()
            .ok_or_else(|| VhdxError::RequiredRegionNotFound("Metadata".to_string()))?;

        let mut metadata_data = vec![0u8; metadata_entry.length as usize];
        file.seek(SeekFrom::Start(metadata_entry.file_offset))?;
        file.read_exact(&mut metadata_data)?;
        let metadata = MetadataRegion::from_bytes(&metadata_data)?;

        // Read BAT
        let bat_entry = region_table
            .find_bat()
            .ok_or_else(|| VhdxError::RequiredRegionNotFound("BAT".to_string()))?;

        let mut bat_data = vec![0u8; bat_entry.length as usize];
        file.seek(SeekFrom::Start(bat_entry.file_offset))?;
        file.read_exact(&mut bat_data)?;

        // Parse metadata values
        let file_params = metadata.file_parameters()?;
        let virtual_disk_size = metadata.virtual_disk_size()?.size;
        let logical_sector_size = metadata.logical_sector_size()?.size;
        let physical_sector_size = metadata.physical_sector_size()?.size;
        let virtual_disk_id = metadata.virtual_disk_id()?.guid;

        // Parse BAT
        let mut bat = Bat::from_bytes(
            &bat_data,
            virtual_disk_size,
            file_params.block_size as u64,
            logical_sector_size,
        )?;

        // Set BAT file offset for updates
        bat.set_bat_file_offset(bat_entry.file_offset);

        // Determine disk type
        let disk_type = if file_params.has_parent {
            DiskType::Differencing
        } else {
            // Check if it's fixed or dynamic based on BAT entries
            // Fixed disk: first payload block is FULLY_PRESENT
            // Dynamic disk: first payload block is NOT_PRESENT
            if let Some(first_entry) = bat.get_payload_entry(0) {
                if first_entry.state == crate::bat::PayloadBlockState::FullyPresent {
                    DiskType::Fixed
                } else {
                    DiskType::Dynamic
                }
            } else {
                DiskType::Dynamic
            }
        };

        // Load parent for differencing disks
        let parent = if disk_type == DiskType::Differencing {
            if let Ok(locator) = metadata.parent_locator() {
                if let Some(parent_path) = locator.parent_path() {
                    // Resolve parent path relative to this file
                    let parent_full_path = if std::path::Path::new(parent_path).is_absolute() {
                        std::path::PathBuf::from(parent_path)
                    } else {
                        path.parent()
                            .map(|p| p.join(parent_path))
                            .unwrap_or_else(|| std::path::PathBuf::from(parent_path))
                    };

                    Some(Box::new(Self::open(parent_full_path, true)?))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut vhdx = VhdxFile {
            file,
            path,
            file_type,
            header,
            region_table,
            metadata,
            bat,
            disk_type,
            virtual_disk_size,
            block_size: file_params.block_size,
            logical_sector_size,
            physical_sector_size,
            virtual_disk_id,
            sequence_number,
            read_only,
            parent,
            log_writer: None, // Will be initialized after replay
        };

        // Initialize LogWriter for metadata updates (if not read-only and log exists)
        if !read_only && vhdx.header.log_length > 0 {
            vhdx.log_writer = Some(crate::log::LogWriter::new(
                vhdx.header.log_offset,
                vhdx.header.log_length,
                vhdx.header.log_guid,
                vhdx.current_file_size()?,
            ));
        }

        // Update header GUIDs on first write-capable open
        if !read_only {
            vhdx.update_header_guids()?;
        }

        Ok(vhdx)
    }

    /// Replay log entries
    fn replay_log(file: &mut File, header: &mut VhdxHeader, read_only: bool) -> Result<()> {
        if header.log_offset == 0 || header.log_length == 0 || header.log_guid.is_zero() {
            return Ok(());
        }

        // Read log data
        let mut log_data = vec![0u8; header.log_length as usize];
        file.seek(SeekFrom::Start(header.log_offset))?;
        match file.read_exact(&mut log_data) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Log might be truncated, but that's ok
            }
            Err(e) => return Err(e.into()),
        }

        // Find active sequence
        if let Some(sequence) =
            LogReplayer::find_active_sequence(&log_data, header.log_length, &header.log_guid)?
        {
            if read_only {
                // In read-only mode, skip replay but return Ok
                // The file is still valid for reading
                return Ok(());
            }

            // Replay the sequence
            let flushed_offset = LogReplayer::replay_sequence(&sequence, file)?;

            // Extend file if needed
            let current_size = file.seek(SeekFrom::End(0))?;
            if flushed_offset > current_size {
                file.seek(SeekFrom::Start(flushed_offset - 1))?;
                file.write_all(&[0])?;
            }

            // Clear log after successful replay
            let zeros = vec![0u8; header.log_length as usize];
            file.seek(SeekFrom::Start(header.log_offset))?;
            file.write_all(&zeros)?;
            file.flush()?;

            // Reset LogGuid to indicate log is empty
            header.log_guid = Guid::new([0u8; 16]);
        }

        Ok(())
    }

    /// Update header GUIDs on first open
    fn update_header_guids(&mut self) -> Result<()> {
        self.header.file_write_guid = Guid::new_v4();
        self.sequence_number += 1;
        self.header.sequence_number = self.sequence_number;

        // Update headers in file (both locations)
        // Header 2 should always have a higher sequence number than Header 1
        // so it's considered the "current" header
        self.update_both_headers()?;

        Ok(())
    }

    /// Update both headers with proper sequence numbers
    fn update_both_headers(&mut self) -> Result<()> {
        use crate::common::crc32c::crc32c_with_zero_field;
        use byteorder::{ByteOrder, LittleEndian};
        use std::io::{Seek, SeekFrom, Write};

        // Update header 1 first (lower sequence number - considered "old")
        let mut header1 = self.header.clone();
        header1.sequence_number = self.sequence_number;
        let mut data1 = header1.to_bytes();
        let checksum1 = crc32c_with_zero_field(&data1, 4, 4);
        LittleEndian::write_u32(&mut data1[4..8], checksum1);
        self.file.seek(SeekFrom::Start(VhdxHeader::OFFSET_1))?;
        self.file.write_all(&data1)?;

        // Update header 2 (higher sequence number - considered "current")
        let mut header2 = self.header.clone();
        header2.sequence_number = self.sequence_number + 1;
        let mut data2 = header2.to_bytes();
        let checksum2 = crc32c_with_zero_field(&data2, 4, 4);
        LittleEndian::write_u32(&mut data2[4..8], checksum2);
        self.file.seek(SeekFrom::Start(VhdxHeader::OFFSET_2))?;
        self.file.write_all(&data2)?;

        self.file.flush()?;

        // Update our internal sequence number to match header 2
        self.sequence_number += 1;
        self.header.sequence_number = self.sequence_number;

        Ok(())
    }

    /// Read data from virtual offset
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        match self.disk_type {
            DiskType::Fixed => {
                // Use FixedBlockIo for fixed disks
                let mut fixed_io =
                    FixedBlockIo::new(&mut self.file, &self.bat, self.virtual_disk_size);
                fixed_io.read(virtual_offset, buf)
            }
            _ => {
                // Use BlockIo for dynamic/differencing disks
                let mut block_io =
                    DynamicBlockIo::new(&mut self.file, &mut self.bat, self.virtual_disk_size);
                block_io.read(virtual_offset, buf)
            }
        }
    }

    /// Write data to virtual offset
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        if self.read_only {
            return Err(VhdxError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "File is read-only",
            )));
        }

        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let result = match self.disk_type {
            DiskType::Fixed => {
                // Use FixedBlockIo for fixed disks
                let mut fixed_io =
                    FixedBlockIo::new(&mut self.file, &self.bat, self.virtual_disk_size);
                fixed_io.write(virtual_offset, buf)
            }
            _ => {
                // Use BlockIo for dynamic/differencing disks with LogWriter
                let mut block_io =
                    DynamicBlockIo::new(&mut self.file, &mut self.bat, self.virtual_disk_size);

                // Attach LogWriter if available
                if let Some(log_writer) = self.log_writer.take() {
                    let result = block_io
                        .with_log_writer(log_writer)
                        .write(virtual_offset, buf);
                    // Return LogWriter to VhdxFile (simplified - in production would use RefCell)
                    // For now, we accept that LogWriter is consumed
                    result
                } else {
                    block_io.write(virtual_offset, buf)
                }
            }
        };

        // Update DataWriteGuid after any write operation
        if result.is_ok() {
            self.header.data_write_guid = Guid::new_v4();
            self.update_headers()?;
        }

        result
    }

    /// Update both headers after modifications
    fn update_headers(&mut self) -> Result<()> {
        let current_header_idx = 0; // Simplified
        update_headers(&mut self.file, current_header_idx, &self.header)?;
        Ok(())
    }

    /// Get virtual disk size
    pub fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }

    /// Get block size
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// Get logical sector size
    pub fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    /// Get physical sector size
    pub fn physical_sector_size(&self) -> u32 {
        self.physical_sector_size
    }

    /// Get disk type
    pub fn disk_type(&self) -> DiskType {
        self.disk_type
    }

    /// Get virtual disk ID
    pub fn virtual_disk_id(&self) -> &Guid {
        &self.virtual_disk_id
    }

    /// Check if file has parent (differencing disk)
    pub fn has_parent(&self) -> bool {
        self.disk_type == DiskType::Differencing
    }

    /// Get creator string
    pub fn creator(&self) -> Option<String> {
        self.file_type.creator_string()
    }

    /// Get current file size
    pub(crate) fn current_file_size(&mut self) -> Result<u64> {
        let pos = self.file.seek(SeekFrom::End(0))?;
        self.file.seek(SeekFrom::Start(pos))?;
        Ok(pos)
    }
}
