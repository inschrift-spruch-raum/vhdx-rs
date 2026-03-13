//! Main VHDX file handling
//!
//! Provides the high-level API for opening, reading, writing, and creating VHDX files.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::bat::Bat;
use crate::block::BlockIo;
use crate::error::{Result, VhdxError};
use crate::guid::Guid;
use crate::header::{read_headers, update_headers, FileTypeIdentifier, VhdxHeader};
use crate::log::LogReplayer;
use crate::metadata::MetadataRegion;
use crate::region::{read_region_tables, RegionTable};

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
}

impl VhdxFile {
    /// Open an existing VHDX file
    ///
    /// This will replay the log if necessary.
    pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;

        // Read file type identifier
        let mut ft_data = vec![0u8; FileTypeIdentifier::SIZE];
        file.read_exact(&mut ft_data)?;
        let file_type = FileTypeIdentifier::from_bytes(&ft_data)?;

        // Read headers and determine current one
        let (_header_idx, header, _) = read_headers(&mut file)?;

        // Store sequence number before moving header
        let sequence_number = header.sequence_number;

        // Verify version
        header.check_version()?;

        // Read region tables
        let (region_table, _) = read_region_tables(&mut file)?;

        // Replay log if needed
        if !header.log_guid.is_zero() {
            Self::replay_log(&mut file, &header)?;
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

        // Determine disk type
        let disk_type = if file_params.has_parent {
            DiskType::Differencing
        } else {
            // Check if it's fixed or dynamic based on BAT entries
            // This is a simplification - in reality we'd check more carefully
            DiskType::Dynamic
        };

        // Parse BAT
        let bat = Bat::from_bytes(
            &bat_data,
            virtual_disk_size,
            file_params.block_size as u64,
            logical_sector_size,
        )?;

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
        };

        // Update header GUIDs on first write-capable open
        if !read_only {
            vhdx.update_header_guids()?;
        }

        Ok(vhdx)
    }

    /// Replay log entries
    fn replay_log(file: &mut File, header: &VhdxHeader) -> Result<()> {
        if header.log_offset == 0 || header.log_length == 0 {
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
            // Replay the sequence
            LogReplayer::replay_sequence(&sequence, file)?;
        }

        Ok(())
    }

    /// Update header GUIDs on first open
    fn update_header_guids(&mut self) -> Result<()> {
        self.header.file_write_guid = Guid::new_v4();
        self.sequence_number += 1;
        self.header.sequence_number = self.sequence_number;

        // Update headers in file
        let current_header_idx = 0; // Simplified
        update_headers(&mut self.file, current_header_idx, &self.header)?;

        Ok(())
    }

    /// Read data from virtual offset
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }

        let mut block_io = BlockIo::new(&mut self.file, &self.bat, self.virtual_disk_size);

        // Set parent for differencing disks
        if let Some(ref _parent) = self.parent {
            // This is a simplified approach - in reality we'd need
            // proper lifetime management
        }

        block_io.read(virtual_offset, buf)
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

        let mut block_io = BlockIo::new(&mut self.file, &self.bat, self.virtual_disk_size);

        block_io.write(virtual_offset, buf)
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
    pub fn create<P: AsRef<Path>>(self, _path: P) -> Result<VhdxFile> {
        // This is a simplified implementation
        // In a full implementation, we'd:
        // 1. Create file type identifier
        // 2. Create headers
        // 3. Create region table
        // 4. Create metadata region
        // 5. Create BAT
        // 6. Initialize log (if needed)
        // 7. Allocate payload blocks (for fixed disk)
        // 8. Write all structures to file

        Err(VhdxError::InvalidMetadata(
            "Create not yet fully implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
