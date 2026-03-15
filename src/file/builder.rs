//! VHDX Builder for creating new files
//!
//! Provides a builder pattern for creating new VHDX files with various configurations.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use byteorder::{ByteOrder, LittleEndian};

use crate::common::Guid;
use crate::error::{Result, VhdxError};
use crate::header::{FileTypeIdentifier, VhdxHeader, REGION_SIGNATURE};

use super::{DiskType, VhdxFile};

/// VHDX Builder for creating new files
pub struct VhdxBuilder {
    /// Virtual disk size
    pub(crate) virtual_disk_size: u64,
    /// Block size
    pub(crate) block_size: u32,
    /// Logical sector size
    pub(crate) logical_sector_size: u32,
    /// Physical sector size
    pub(crate) physical_sector_size: u32,
    /// Disk type
    pub(crate) disk_type: DiskType,
    /// Parent path (for differencing)
    pub(crate) parent_path: Option<String>,
    /// Creator string
    pub(crate) creator: Option<String>,
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

        // Block size must be power of 2, 1MB to 256MB per MS-VHDX Section 2.2.2
        const MIN_BLOCK_SIZE: u32 = 1024 * 1024; // 1MB
        const MAX_BLOCK_SIZE: u32 = 256 * 1024 * 1024; // 256MB

        if self.block_size < MIN_BLOCK_SIZE || self.block_size > MAX_BLOCK_SIZE {
            return Err(VhdxError::InvalidBlockSize(self.block_size));
        }

        // Check power of 2: only one bit set
        if self.block_size & (self.block_size - 1) != 0 {
            return Err(VhdxError::InvalidBlockSize(self.block_size));
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
        let num_payload_blocks = self.virtual_disk_size.div_ceil(self.block_size as u64);
        let num_sector_bitmap_blocks = num_payload_blocks.div_ceil(chunk_ratio);
        let num_bat_entries = num_payload_blocks + num_sector_bitmap_blocks;

        // Calculate file layout
        // Windows expects: Metadata (2MB) -> BAT (3MB), not BAT -> Metadata
        let header_size = 1024 * 1024; // 1MB header section
        let metadata_size = 1024 * 1024; // 1MB metadata
        let bat_size = (num_bat_entries * 8).div_ceil(1024 * 1024) * (1024 * 1024); // 1MB aligned
        let _log_size = 0u64; // No separate log region - embedded in header

        let metadata_offset = header_size * 2; // Metadata at 2MB
        let bat_offset = metadata_offset + metadata_size; // BAT after metadata (3MB)
        let data_offset = bat_offset + bat_size; // Payload data after BAT

        // Calculate file size
        let _file_size = if self.disk_type == DiskType::Fixed {
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
        header1.log_guid = Guid::from_bytes([0u8; 16]); // No log - embedded in header
        header1.log_version = 0;
        header1.version = 1;
        header1.log_length = 0; // No separate log
        header1.log_offset = 0; // No separate log

        // Calculate and write header 1
        let mut header1_data = header1.to_bytes();
        let checksum1 = crate::common::crc32c::crc32c_with_zero_field(&header1_data, 4, 4);
        LittleEndian::write_u32(&mut header1_data[4..8], checksum1);

        file.seek(SeekFrom::Start(64 * 1024))?;
        file.write_all(&header1_data)?;

        // Header 2 at 128KB (copy of header 1)
        let mut header2 = header1.clone();
        header2.sequence_number = 1; // Higher sequence number
        let mut header2_data = header2.to_bytes();
        let checksum2 = crate::common::crc32c::crc32c_with_zero_field(&header2_data, 4, 4);
        LittleEndian::write_u32(&mut header2_data[4..8], checksum2);

        file.seek(SeekFrom::Start(128 * 1024))?;
        file.write_all(&header2_data)?;

        // Step 3: Create and write Region Table
        // Region Table Header
        let region_entry_size = 32;
        let region_header_size = 16;
        let _region_data_size = region_header_size + 2 * region_entry_size; // 2 entries: BAT and Metadata

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
        let region_checksum = crate::common::crc32c::crc32c_with_zero_field(&region_data, 4, 4);
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
        let _ = current_data_offset + 16; // Suppress unused assignment warning

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
