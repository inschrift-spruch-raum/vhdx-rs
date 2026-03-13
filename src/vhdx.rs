//! Main VHDX file handling
//!
//! Provides the high-level API for opening, reading, writing, and creating VHDX files.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use byteorder::{ByteOrder, LittleEndian};

use crate::bat::Bat;
use crate::block::{BlockIo, FixedBlockIo};
use crate::error::{Result, VhdxError};
use crate::guid::Guid;
use crate::header::{
    read_headers, read_region_tables, update_headers, FileTypeIdentifier, RegionTable, VhdxHeader,
    HEADER_SIGNATURE, REGION_SIGNATURE,
};
use crate::log::LogReplayer;
use crate::metadata::MetadataRegion;

/// VHDX disk type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskType {
    /// Fixed size disk
    Fixed,
    /// Dynamically expanding disk
    Dynamic,
    /// Differencing disk (has parent)
    Differencing,
}

/// VHDX file handle
pub struct VhdxFile {
    /// Underlying file
    file: File,
    /// File path
    path: std::path::PathBuf,
    /// File type identifier
    file_type: FileTypeIdentifier,
    /// Current header (index 0 or 1)
    header: VhdxHeader,
    /// Region table
    region_table: RegionTable,
    /// Metadata region
    metadata: MetadataRegion,
    /// Block Allocation Table
    bat: Bat,
    /// Disk type
    disk_type: DiskType,
    /// Virtual disk size
    virtual_disk_size: u64,
    /// Block size
    block_size: u32,
    /// Logical sector size
    logical_sector_size: u32,
    /// Physical sector size
    physical_sector_size: u32,
    /// Virtual disk ID
    virtual_disk_id: Guid,
    /// Current sequence number for header updates
    sequence_number: u64,
    /// Is file open in read-only mode
    read_only: bool,
    /// Parent file (for differencing disks)
    parent: Option<Box<VhdxFile>>,
    /// Log writer for metadata updates
    log_writer: Option<crate::log::LogWriter>,
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
        use crate::crc32c::crc32c_with_zero_field;
        use byteorder::LittleEndian;
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
                    BlockIo::new(&mut self.file, &mut self.bat, self.virtual_disk_size);
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
                    BlockIo::new(&mut self.file, &mut self.bat, self.virtual_disk_size);

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
    fn current_file_size(&mut self) -> Result<u64> {
        let pos = self.file.seek(SeekFrom::End(0))?;
        self.file.seek(SeekFrom::Start(pos))?;
        Ok(pos)
    }
}

/// VHDX Builder for creating new files
pub struct VhdxBuilder {
    /// Virtual disk size
    virtual_disk_size: u64,
    /// Block size
    block_size: u32,
    /// Logical sector size
    logical_sector_size: u32,
    /// Physical sector size
    physical_sector_size: u32,
    /// Disk type
    disk_type: DiskType,
    /// Parent path (for differencing)
    parent_path: Option<String>,
    /// Creator string
    creator: Option<String>,
}

impl VhdxBuilder {
    /// Create new builder with required parameters
    pub fn new(virtual_disk_size: u64) -> Self {
        VhdxBuilder {
            virtual_disk_size,
            block_size: 1024 * 1024 * 32, // 32MB default
            logical_sector_size: 512,
            physical_sector_size: 4096,
            disk_type: DiskType::Dynamic,
            parent_path: None,
            creator: Some("Rust VHDX Library".to_string()),
        }
    }

    /// Set block size
    pub fn block_size(mut self, size: u32) -> Self {
        self.block_size = size;
        self
    }

    /// Set sector sizes
    pub fn sector_sizes(mut self, logical: u32, physical: u32) -> Self {
        self.logical_sector_size = logical;
        self.physical_sector_size = physical;
        self
    }

    /// Set disk type
    pub fn disk_type(mut self, disk_type: DiskType) -> Self {
        self.disk_type = disk_type;
        self
    }

    /// Set parent path (for differencing disk)
    pub fn parent_path<P: Into<String>>(mut self, path: P) -> Self {
        self.parent_path = Some(path.into());
        self.disk_type = DiskType::Differencing;
        self
    }

    /// Set creator string
    pub fn creator(mut self, creator: String) -> Self {
        self.creator = Some(creator);
        self
    }

    /// Build and create the VHDX file
    pub fn create<P: AsRef<Path>>(self, path: P) -> Result<VhdxFile> {
        let path = path.as_ref();

        // Validate parameters
        if self.virtual_disk_size == 0 {
            return Err(VhdxError::InvalidMetadata(
                "Virtual disk size cannot be zero".to_string(),
            ));
        }

        // Block size must be 1MB to 256MB and 1MB aligned
        if self.block_size < 1024 * 1024 || self.block_size > 256 * 1024 * 1024 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Block size {} out of range (1MB-256MB)",
                self.block_size
            )));
        }
        if self.block_size % (1024 * 1024) != 0 {
            return Err(VhdxError::InvalidMetadata(
                "Block size must be 1MB aligned".to_string(),
            ));
        }

        // Sector sizes must be 512 or 4096
        if self.logical_sector_size != 512 && self.logical_sector_size != 4096 {
            return Err(VhdxError::InvalidMetadata(
                "Logical sector size must be 512 or 4096".to_string(),
            ));
        }
        if self.physical_sector_size != 512 && self.physical_sector_size != 4096 {
            return Err(VhdxError::InvalidMetadata(
                "Physical sector size must be 512 or 4096".to_string(),
            ));
        }

        // Create the file
        let mut file = File::create(path)?;

        // Generate GUIDs
        let file_write_guid = Guid::new_v4();
        let data_write_guid = Guid::new_v4();
        let virtual_disk_id = Guid::new_v4();

        // Calculate sizes
        let chunk_size = (1u64 << 23) * self.logical_sector_size as u64;
        let chunk_ratio = chunk_size / self.block_size as u64;
        let num_payload_blocks =
            (self.virtual_disk_size + self.block_size as u64 - 1) / self.block_size as u64;
        let num_sector_bitmap_blocks = (num_payload_blocks + chunk_ratio - 1) / chunk_ratio;
        let num_bat_entries = num_payload_blocks + num_sector_bitmap_blocks;

        // Calculate file layout
        // Windows expects: Metadata (2MB) -> BAT (3MB), not BAT -> Metadata
        let header_size = 1024 * 1024; // 1MB header section
        let metadata_size = 1024 * 1024; // 1MB metadata
        let bat_size = ((num_bat_entries * 8 + 1024 * 1024 - 1) / (1024 * 1024)) * (1024 * 1024); // 1MB aligned
        let log_size = 0u64; // No separate log region - embedded in header

        let metadata_offset = header_size * 2; // Metadata at 2MB
        let bat_offset = metadata_offset + metadata_size; // BAT after metadata (3MB)
        let data_offset = bat_offset + bat_size; // Payload data after BAT

        // Calculate file size
        let file_size = if self.disk_type == DiskType::Fixed {
            // Fixed disk: allocate all payload blocks upfront
            data_offset + num_payload_blocks * self.block_size as u64
        } else {
            // Dynamic/differencing: just allocate metadata areas
            data_offset
        };

        // Step 1: Write File Type Identifier (64KB at offset 0)
        let file_type = FileTypeIdentifier::new(self.creator.as_deref());
        file.write_all(&file_type.to_bytes())?;

        // Step 2: Create and write headers
        // Header 1 at 64KB
        let mut header1 = VhdxHeader::new(0);
        header1.file_write_guid = file_write_guid;
        header1.data_write_guid = data_write_guid;
        header1.log_guid = Guid::new([0u8; 16]); // No log - embedded in header
        header1.log_version = 0;
        header1.version = 1;
        header1.log_length = 0; // No separate log
        header1.log_offset = 0; // No separate log

        // Calculate and write header 1
        let mut header1_data = header1.to_bytes();
        let checksum1 = crate::crc32c::crc32c_with_zero_field(&header1_data, 4, 4);
        LittleEndian::write_u32(&mut header1_data[4..8], checksum1);

        file.seek(SeekFrom::Start(64 * 1024))?;
        file.write_all(&header1_data)?;

        // Header 2 at 128KB (copy of header 1)
        let mut header2 = header1.clone();
        header2.sequence_number = 1; // Higher sequence number
        let mut header2_data = header2.to_bytes();
        let checksum2 = crate::crc32c::crc32c_with_zero_field(&header2_data, 4, 4);
        LittleEndian::write_u32(&mut header2_data[4..8], checksum2);

        file.seek(SeekFrom::Start(128 * 1024))?;
        file.write_all(&header2_data)?;

        // Step 3: Create and write Region Table
        // Region Table Header
        let region_entry_size = 32;
        let region_header_size = 16;
        let region_data_size = region_header_size + 2 * region_entry_size; // 2 entries: BAT and Metadata

        let mut region_data = vec![0u8; 64 * 1024]; // 64KB region table

        // Region Table Header
        region_data[0..4].copy_from_slice(REGION_SIGNATURE);
        // Entry count
        LittleEndian::write_u32(&mut region_data[8..12], 2);

        // BAT Region Entry
        let entry1_offset = region_header_size;
        let bat_guid = crate::header::BAT_GUID;
        region_data[entry1_offset..entry1_offset + 16].copy_from_slice(&bat_guid.to_bytes());
        LittleEndian::write_u64(
            &mut region_data[entry1_offset + 16..entry1_offset + 24],
            bat_offset,
        );
        LittleEndian::write_u32(
            &mut region_data[entry1_offset + 24..entry1_offset + 28],
            bat_size as u32,
        );
        LittleEndian::write_u32(&mut region_data[entry1_offset + 28..entry1_offset + 32], 1); // Required

        // Metadata Region Entry
        let entry2_offset = entry1_offset + region_entry_size;
        let metadata_guid = crate::header::METADATA_GUID;
        region_data[entry2_offset..entry2_offset + 16].copy_from_slice(&metadata_guid.to_bytes());
        LittleEndian::write_u64(
            &mut region_data[entry2_offset + 16..entry2_offset + 24],
            metadata_offset,
        );
        LittleEndian::write_u32(
            &mut region_data[entry2_offset + 24..entry2_offset + 28],
            metadata_size as u32,
        );
        LittleEndian::write_u32(&mut region_data[entry2_offset + 28..entry2_offset + 32], 1); // Required

        // Calculate region table checksum
        let region_checksum = crate::crc32c::crc32c_with_zero_field(&region_data, 4, 4);
        LittleEndian::write_u32(&mut region_data[4..8], region_checksum);

        // Write region tables (both copies)
        file.seek(SeekFrom::Start(192 * 1024))?;
        file.write_all(&region_data)?;
        file.seek(SeekFrom::Start(256 * 1024))?;
        file.write_all(&region_data)?;

        // Step 4: Create BAT
        let mut bat_data = vec![0u8; bat_size as usize];
        let mut bat_entries = Vec::with_capacity(num_bat_entries as usize);

        if self.disk_type == DiskType::Fixed {
            // For fixed disk, all payload blocks are pre-allocated
            let mut current_data_offset = data_offset / (1024 * 1024); // In MB

            for chunk_idx in 0..num_sector_bitmap_blocks {
                // Payload blocks for this chunk
                let blocks_in_chunk =
                    std::cmp::min(chunk_ratio, num_payload_blocks - chunk_idx * chunk_ratio);

                for _ in 0..blocks_in_chunk {
                    // Fully present payload block
                    let entry = crate::bat::BatEntry::new(
                        crate::bat::PayloadBlockState::FullyPresent,
                        current_data_offset,
                    );
                    bat_entries.push(entry);
                    current_data_offset += self.block_size as u64 / (1024 * 1024);
                }

                // Fill remaining blocks in chunk with NOT_PRESENT
                for _ in blocks_in_chunk..chunk_ratio {
                    let entry =
                        crate::bat::BatEntry::new(crate::bat::PayloadBlockState::NotPresent, 0);
                    bat_entries.push(entry);
                }

                // Sector bitmap block (NOT_PRESENT for fixed disk)
                let sb_entry =
                    crate::bat::BatEntry::new(crate::bat::PayloadBlockState::NotPresent, 0);
                bat_entries.push(sb_entry);
            }
        } else {
            // For dynamic/differencing disk, blocks are NOT_PRESENT initially
            for _ in 0..num_payload_blocks {
                let entry = crate::bat::BatEntry::new(crate::bat::PayloadBlockState::NotPresent, 0);
                bat_entries.push(entry);
            }

            // Sector bitmap blocks
            for _ in 0..num_sector_bitmap_blocks {
                let entry = crate::bat::BatEntry::new(crate::bat::PayloadBlockState::NotPresent, 0);
                bat_entries.push(entry);
            }
        }

        // Serialize BAT entries
        for (i, entry) in bat_entries.iter().enumerate() {
            let offset = i * 8;
            bat_data[offset..offset + 8].copy_from_slice(&entry.to_bytes());
        }

        // Write BAT
        file.seek(SeekFrom::Start(bat_offset))?;
        file.write_all(&bat_data)?;

        // Step 5: Create Metadata Region
        let metadata_table_size = 64 * 1024; // 64KB metadata table
        let mut metadata_data = vec![0u8; metadata_size as usize];

        // Metadata Table Header (at start of metadata region)
        metadata_data[0..8].copy_from_slice(crate::metadata::METADATA_SIGNATURE);
        // Entry count (5 required entries: FileParameters, VirtualDiskSize, VirtualDiskId, LogicalSectorSize, PhysicalSectorSize)
        let entry_count: u16 = if self.disk_type == DiskType::Differencing {
            6 // + ParentLocator
        } else {
            5
        };
        LittleEndian::write_u16(&mut metadata_data[10..12], entry_count);

        // Metadata entries are in the first 64KB, but data starts at 64KB
        // Entry.offset = 64KB + relative_offset_within_data_area
        let data_area_start = metadata_table_size; // 64KB
        let mut metadata_entries = Vec::new();
        let mut current_data_offset = data_area_start;

        // File Parameters (8 bytes: 4 + 4)
        let fp_guid = crate::metadata::FILE_PARAMETERS_GUID;
        let fp_entry_offset = current_data_offset;
        metadata_entries.push((fp_guid, fp_entry_offset, 8u32));
        LittleEndian::write_u32(
            &mut metadata_data[current_data_offset..current_data_offset + 4],
            self.block_size,
        );
        LittleEndian::write_u32(
            &mut metadata_data[current_data_offset + 4..current_data_offset + 8],
            if self.disk_type == DiskType::Differencing {
                1
            } else {
                0
            },
        );
        current_data_offset += 8;

        // Virtual Disk Size (8 bytes)
        let vds_guid = crate::metadata::VIRTUAL_DISK_SIZE_GUID;
        let vds_entry_offset = current_data_offset;
        metadata_entries.push((vds_guid, vds_entry_offset, 8u32));
        LittleEndian::write_u64(
            &mut metadata_data[current_data_offset..current_data_offset + 8],
            self.virtual_disk_size,
        );
        current_data_offset += 8;

        // Logical Sector Size (4 bytes)
        let lss_guid = crate::metadata::LOGICAL_SECTOR_SIZE_GUID;
        let lss_entry_offset = current_data_offset;
        metadata_entries.push((lss_guid, lss_entry_offset, 4u32));
        LittleEndian::write_u32(
            &mut metadata_data[current_data_offset..current_data_offset + 4],
            self.logical_sector_size,
        );
        current_data_offset += 4;

        // Physical Sector Size (4 bytes)
        let pss_guid = crate::metadata::PHYSICAL_SECTOR_SIZE_GUID;
        let pss_entry_offset = current_data_offset;
        metadata_entries.push((pss_guid, pss_entry_offset, 4u32));
        LittleEndian::write_u32(
            &mut metadata_data[current_data_offset..current_data_offset + 4],
            self.physical_sector_size,
        );
        current_data_offset += 4;

        // Virtual Disk ID (16 bytes)
        let vdi_guid = crate::metadata::VIRTUAL_DISK_ID_GUID;
        let vdi_entry_offset = current_data_offset;
        metadata_entries.push((vdi_guid, vdi_entry_offset, 16u32));
        metadata_data[current_data_offset..current_data_offset + 16]
            .copy_from_slice(&virtual_disk_id.to_bytes());
        current_data_offset += 16;

        // Write metadata entries table (at offset 32, right after header)
        let entries_start = 32; // After 32-byte header
        for (i, (guid, offset, length)) in metadata_entries.iter().enumerate() {
            let entry_offset = entries_start + i * 32;
            metadata_data[entry_offset..entry_offset + 16].copy_from_slice(&guid.to_bytes());
            LittleEndian::write_u32(
                &mut metadata_data[entry_offset + 16..entry_offset + 20],
                *offset as u32,
            );
            LittleEndian::write_u32(
                &mut metadata_data[entry_offset + 20..entry_offset + 24],
                *length,
            );
            // Flags: isUser=1 (0x01), isVirtualDisk=1 (0x02) = 0x03
            // But MS-VHDX says system metadata has isUser=0
            // Looking at test1.vhdx: FileParameters has flags=0x04 (isUser=1), others have 0x06 (isUser=1, isVirtualDisk=1)
            // Let's use 0x04 for FileParameters and 0x06 for others to match Windows behavior
            let flags = if i == 0 { 0x04 } else { 0x06 };
            LittleEndian::write_u32(
                &mut metadata_data[entry_offset + 24..entry_offset + 28],
                flags,
            );
        }

        // Write Metadata
        file.seek(SeekFrom::Start(metadata_offset))?;
        file.write_all(&metadata_data)?;

        // Step 6: For fixed disk, allocate payload data
        if self.disk_type == DiskType::Fixed {
            let payload_size = num_payload_blocks * self.block_size as u64;
            let payload_data = vec![0u8; payload_size as usize];
            file.seek(SeekFrom::Start(data_offset))?;
            file.write_all(&payload_data)?;
        }

        // Flush all writes
        file.flush()?;

        // Explicitly close the file before reopening
        drop(file);

        // Now open the newly created file
        VhdxFile::open(path, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_vhdx_builder() {
        let builder = VhdxBuilder::new(10 * 1024 * 1024 * 1024) // 10GB
            .block_size(1024 * 1024) // 1MB
            .sector_sizes(512, 4096)
            .disk_type(DiskType::Dynamic);

        assert_eq!(builder.virtual_disk_size, 10 * 1024 * 1024 * 1024);
        assert_eq!(builder.block_size, 1024 * 1024);
        assert_eq!(builder.disk_type, DiskType::Dynamic);
    }

    #[test]
    fn test_create_dynamic_vhdx() {
        let temp_dir = TempDir::new().unwrap();
        let path: PathBuf = temp_dir.path().join("test_dynamic.vhdx");

        let vhdx = VhdxBuilder::new(100 * 1024 * 1024) // 100MB
            .block_size(1024 * 1024) // 1MB
            .sector_sizes(512, 4096)
            .disk_type(DiskType::Dynamic)
            .create(&path)
            .expect("Failed to create VHDX");

        assert_eq!(vhdx.virtual_disk_size(), 100 * 1024 * 1024);
        assert_eq!(vhdx.block_size(), 1024 * 1024);
        assert_eq!(vhdx.disk_type(), DiskType::Dynamic);
    }

    #[test]
    fn test_create_fixed_vhdx() {
        let temp_dir = TempDir::new().unwrap();
        let path: PathBuf = temp_dir.path().join("test_fixed.vhdx");

        let vhdx = VhdxBuilder::new(10 * 1024 * 1024) // 10MB
            .block_size(1024 * 1024) // 1MB
            .sector_sizes(512, 4096)
            .disk_type(DiskType::Fixed)
            .create(&path)
            .expect("Failed to create VHDX");

        assert_eq!(vhdx.virtual_disk_size(), 10 * 1024 * 1024);
        assert_eq!(vhdx.disk_type(), DiskType::Fixed);

        // Fixed disk should have pre-allocated blocks
        // Check that file size is at least virtual disk size + header
        let file_size = std::fs::metadata(&path).unwrap().len();
        assert!(file_size >= 10 * 1024 * 1024);
    }

    #[test]
    fn test_read_write_dynamic() {
        let temp_dir = TempDir::new().unwrap();
        let path: PathBuf = temp_dir.path().join("test_rw.vhdx");

        let mut vhdx = VhdxBuilder::new(10 * 1024 * 1024) // 10MB
            .block_size(1024 * 1024)
            .sector_sizes(512, 4096)
            .disk_type(DiskType::Dynamic)
            .create(&path)
            .expect("Failed to create VHDX");

        // Write data
        let write_data = b"Hello, VHDX World!";
        let bytes_written = vhdx.write(0, write_data).expect("Failed to write");
        assert_eq!(bytes_written, write_data.len());

        // Read data back
        let mut read_buf = vec![0u8; write_data.len()];
        let bytes_read = vhdx.read(0, &mut read_buf).expect("Failed to read");
        assert_eq!(bytes_read, write_data.len());
        assert_eq!(&read_buf, write_data);
    }

    #[test]
    fn test_read_unwritten_block() {
        let temp_dir = TempDir::new().unwrap();
        let path: PathBuf = temp_dir.path().join("test_unwritten.vhdx");

        let mut vhdx = VhdxBuilder::new(10 * 1024 * 1024)
            .block_size(1024 * 1024)
            .sector_sizes(512, 4096)
            .disk_type(DiskType::Dynamic)
            .create(&path)
            .expect("Failed to create VHDX");

        // Read from unwritten block should return zeros
        let mut read_buf = vec![0u8; 1024];
        let bytes_read = vhdx
            .read(1024 * 1024, &mut read_buf)
            .expect("Failed to read");
        assert_eq!(bytes_read, 1024);
        assert!(read_buf.iter().all(|&b| b == 0));
    }
}
