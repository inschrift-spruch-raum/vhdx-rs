use std::fs::{File as StdFile, OpenOptions as StdOpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use crate::common::constants::{
    BAT_ENTRY_SIZE, DEFAULT_BLOCK_SIZE, FILE_TYPE_SIGNATURE, FILE_TYPE_SIZE, HEADER_1_OFFSET,
    HEADER_2_OFFSET, HEADER_SECTION_SIZE, LOGICAL_SECTOR_SIZE_512, MAX_BLOCK_SIZE,
    METADATA_SIGNATURE, METADATA_TABLE_SIZE, MIN_BLOCK_SIZE, MiB, REGION_TABLE_1_OFFSET,
    REGION_TABLE_2_OFFSET, REGION_TABLE_SIGNATURE, REGION_TABLE_SIZE, align_1mib,
};
use crate::common::region_guids;
use crate::error::{Error, Result};
use crate::io_module::IO;
use crate::sections::Bat;
use crate::sections::{FileTypeIdentifier, Header, HeaderStructure, Sections, SectionsConfig};
use crate::types::Guid;

pub struct File {
    inner: StdFile,
    sections: Sections,
    virtual_disk_size: u64,
    block_size: u32,
    logical_sector_size: u32,
    is_fixed: bool,
    has_parent: bool,
    has_pending_logs: bool,
}

impl File {
    pub fn open(path: impl AsRef<Path>) -> OpenOptions {
        OpenOptions {
            path: path.as_ref().to_path_buf(),
            write: false,
        }
    }

    pub fn create(path: impl AsRef<Path>) -> CreateOptions {
        CreateOptions {
            path: path.as_ref().to_path_buf(),
            size: None,
            fixed: false,
            has_parent: false,
            block_size: DEFAULT_BLOCK_SIZE,
            logical_sector_size: LOGICAL_SECTOR_SIZE_512,
        }
    }

    pub const fn sections(&self) -> &Sections {
        &self.sections
    }

    pub const fn io(&self) -> IO<'_> {
        IO::new(self)
    }

    pub const fn inner(&self) -> &StdFile {
        &self.inner
    }

    pub const fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }

    pub const fn block_size(&self) -> u32 {
        self.block_size
    }

    pub const fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    pub const fn is_fixed(&self) -> bool {
        self.is_fixed
    }

    pub const fn has_parent(&self) -> bool {
        self.has_parent
    }

    pub const fn has_pending_logs(&self) -> bool {
        self.has_pending_logs
    }

    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.virtual_disk_size {
            return Ok(0);
        }

        let bytes_to_read = usize::try_from(std::cmp::min(
            u64::try_from(buf.len()).unwrap_or(u64::MAX),
            self.virtual_disk_size - offset,
        ))
        .unwrap_or(usize::MAX);

        if self.is_fixed {
            let header_size = u64::try_from(HEADER_SECTION_SIZE).unwrap_or(0);
            let file_offset = header_size + offset;

            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(file_offset))?;
            let bytes_read = file.read(buf)?;
            Ok(bytes_read)
        } else {
            for item in buf.iter_mut().take(bytes_to_read) {
                *item = 0;
            }
            Ok(bytes_to_read)
        }
    }

    pub fn write(&mut self, offset: u64, data: &[u8]) -> Result<usize> {
        if offset >= self.virtual_disk_size {
            return Err(Error::InvalidParameter(format!(
                "Write offset {} exceeds virtual disk size {}",
                offset, self.virtual_disk_size
            )));
        }

        let bytes_to_write = usize::try_from(std::cmp::min(
            u64::try_from(data.len()).unwrap_or(u64::MAX),
            self.virtual_disk_size - offset,
        ))
        .unwrap_or(usize::MAX);

        if self.is_fixed {
            let header_size = u64::try_from(HEADER_SECTION_SIZE).unwrap_or(0);
            let file_offset = header_size + offset;

            self.inner.seek(SeekFrom::Start(file_offset))?;
            self.inner.write_all(&data[..bytes_to_write])?;
            Ok(bytes_to_write)
        } else {
            self.write_dynamic(offset, &data[..bytes_to_write])?;
            Ok(bytes_to_write)
        }
    }

    fn write_dynamic(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        let block_size = u64::from(self.block_size);
        let block_idx = offset / block_size;
        let block_offset = offset % block_size;

        let bat = self.sections.bat()?;
        let block_idx_usize = usize::try_from(block_idx).map_err(|_| {
            Error::InvalidParameter(format!("block_idx {block_idx} exceeds usize::MAX"))
        })?;
        let bat_entry = bat.entry(block_idx_usize);

        let file_offset = if let Some(entry) = bat_entry {
            if entry.file_offset() > 0 {
                entry.file_offset() + block_offset
            } else {
                return Err(Error::InvalidParameter(
                    "Dynamic block allocation not yet fully implemented".to_string(),
                ));
            }
        } else {
            return Err(Error::InvalidParameter(
                "Dynamic block allocation beyond current entries not yet implemented".to_string(),
            ));
        };

        self.inner.seek(SeekFrom::Start(file_offset))?;
        self.inner.write_all(data)?;

        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.inner.sync_all()?;
        Ok(())
    }

    fn open_file(path: &Path, writable: bool) -> Result<Self> {
        let mut file = Self::open_file_with_share_mode(path, writable)?;

        let mut file_type_data = [0u8; 8];
        file.read_exact(&mut file_type_data)?;
        if &file_type_data != FILE_TYPE_SIGNATURE {
            return Err(Error::InvalidSignature {
                expected: String::from_utf8_lossy(FILE_TYPE_SIGNATURE).to_string(),
                found: String::from_utf8_lossy(&file_type_data).to_string(),
            });
        }

        file.seek(SeekFrom::Start(0))?;

        let mut header_data = vec![0u8; HEADER_SECTION_SIZE];
        file.read_exact(&mut header_data)?;
        let header = Header::new(header_data)?;

        let current_header = header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("No valid header found".to_string()))?;
        let region_table = header
            .region_table(0)
            .ok_or_else(|| Error::InvalidRegionTable("No valid region table found".to_string()))?;

        let (bat_offset, bat_size, metadata_offset, metadata_size) =
            Self::extract_region_info(&region_table)?;
        let (virtual_disk_size, block_size, is_fixed, has_parent, logical_sector_size) =
            Self::read_metadata(&mut file, metadata_offset, metadata_size)?;

        let log_offset = current_header.log_offset();
        let log_size = u64::from(current_header.log_length());

        let entry_count =
            Bat::calculate_total_entries(virtual_disk_size, block_size, logical_sector_size);

        let file_clone2 = file.try_clone()?;
        let sections = Sections::new(SectionsConfig {
            file: file_clone2,
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            entry_count,
        });

        let has_pending_logs =
            Self::handle_log_replay(&mut file, &sections, &current_header, writable)?;

        Ok(Self {
            inner: file,
            sections,
            virtual_disk_size,
            block_size,
            logical_sector_size,
            is_fixed,
            has_parent,
            has_pending_logs,
        })
    }

    fn open_file_with_share_mode(path: &Path, writable: bool) -> Result<StdFile> {
        let mut options = StdOpenOptions::new();
        options.read(true);
        if writable {
            options.write(true);
        }

        #[cfg(windows)]
        {
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }

        match options.open(path) {
            Ok(f) => Ok(f),
            Err(e) => {
                #[cfg(windows)]
                {
                    if e.raw_os_error() == Some(5) {
                        return Err(Error::FileLocked);
                    }
                }
                Err(Error::Io(e))
            }
        }
    }

    fn extract_region_info(
        region_table: &crate::sections::RegionTable<'_>,
    ) -> Result<(u64, u64, u64, u64)> {
        let bat_entry = region_table
            .find_entry(&region_guids::BAT_REGION)
            .ok_or_else(|| Error::InvalidRegionTable("BAT region not found".to_string()))?;
        let bat_offset = bat_entry.file_offset();
        let bat_size = u64::from(bat_entry.length());

        let metadata_entry = region_table
            .find_entry(&region_guids::METADATA_REGION)
            .ok_or_else(|| Error::InvalidRegionTable("Metadata region not found".to_string()))?;
        let metadata_offset = metadata_entry.file_offset();
        let metadata_size = u64::from(metadata_entry.length());

        Ok((bat_offset, bat_size, metadata_offset, metadata_size))
    }

    fn read_metadata(
        file: &mut StdFile, metadata_offset: u64, metadata_size: u64,
    ) -> Result<(u64, u32, bool, bool, u32)> {
        let mut file_clone = file.try_clone()?;
        file_clone.seek(SeekFrom::Start(metadata_offset))?;
        let mut metadata_data = vec![0u8; usize::try_from(metadata_size).unwrap_or(0)];
        file_clone.read_exact(&mut metadata_data)?;
        let temp_metadata = crate::sections::Metadata::new(metadata_data)?;
        let temp_items = temp_metadata.items();

        let virtual_disk_size = temp_items
            .virtual_disk_size()
            .ok_or_else(|| Error::InvalidMetadata("Virtual disk size not found".to_string()))?;

        let file_params = temp_items
            .file_parameters()
            .ok_or_else(|| Error::InvalidMetadata("File parameters not found".to_string()))?;
        let block_size = file_params.block_size();
        let is_fixed = file_params.leave_block_allocated();
        let has_parent = file_params.has_parent();

        let logical_sector_size = temp_items
            .logical_sector_size()
            .unwrap_or(LOGICAL_SECTOR_SIZE_512);

        Ok((
            virtual_disk_size,
            block_size,
            is_fixed,
            has_parent,
            logical_sector_size,
        ))
    }

    fn handle_log_replay(
        file: &mut StdFile, sections: &Sections,
        current_header: &crate::sections::HeaderStructure<'_>, writable: bool,
    ) -> Result<bool> {
        if current_header.log_guid() != Guid::nil() {
            let log = sections.log()?;
            if (*log).is_replay_required() {
                if !writable {
                    return Ok(true);
                }
                (*log).replay(file)?;
                file.sync_all()?;

                let new_header = crate::HeaderStructure::create(
                    current_header.sequence_number(),
                    current_header.file_write_guid(),
                    current_header.data_write_guid(),
                    Guid::nil(),
                    current_header.log_length(),
                    current_header.log_offset(),
                );
                file.seek(SeekFrom::Start(64 * 1024))?;
                file.write_all(&new_header)?;
                file.seek(SeekFrom::Start(128 * 1024))?;
                file.write_all(&new_header)?;
                file.sync_all()?;
            }
        }
        Ok(false)
    }

    fn create_file(
        path: &Path, virtual_size: u64, fixed: bool, has_parent: bool, block_size: u32,
        logical_sector_size: u32,
    ) -> Result<Self> {
        Self::validate_create_params(virtual_size, block_size, logical_sector_size)?;

        if path.exists() {
            return Err(Error::InvalidParameter(format!(
                "File already exists: {}",
                path.display()
            )));
        }

        let mut file = StdOpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let file_write_guid = Guid::from(uuid::Uuid::new_v4());
        let data_write_guid = Guid::from(uuid::Uuid::new_v4());
        let log_guid = Guid::nil();

        let (
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            payload_offset,
            bat_entries,
        ) = Self::calculate_layout(virtual_size, block_size, logical_sector_size);

        let file_type_data = FileTypeIdentifier::create(Some("vhdx-rs"));
        file.write_all(&file_type_data)?;

        let header_padding = vec![0u8; HEADER_SECTION_SIZE - FILE_TYPE_SIZE];
        file.write_all(&header_padding)?;

        file.seek(SeekFrom::Start(metadata_offset))?;
        let metadata_data = create_metadata(
            virtual_size,
            block_size,
            logical_sector_size,
            fixed,
            has_parent,
            data_write_guid,
        );
        file.write_all(&metadata_data)?;
        let actual_metadata_size = u64::try_from(metadata_data.len()).unwrap_or(0);
        if actual_metadata_size < metadata_size {
            let padding =
                vec![0u8; usize::try_from(metadata_size - actual_metadata_size).unwrap_or(0)];
            file.write_all(&padding)?;
        }

        file.seek(SeekFrom::Start(bat_offset))?;
        let bat_data = Self::create_bat_data(fixed, bat_entries, payload_offset, block_size);
        file.write_all(&bat_data)?;

        file.seek(SeekFrom::Start(log_offset))?;
        let log_data = vec![0u8; usize::try_from(log_size).unwrap_or(0)];
        file.write_all(&log_data)?;

        let header_data = HeaderStructure::create(
            0,
            file_write_guid,
            data_write_guid,
            log_guid,
            u32::try_from(log_size).unwrap_or(0),
            log_offset,
        );

        file.seek(SeekFrom::Start(HEADER_1_OFFSET as u64))?;
        file.write_all(&header_data)?;
        file.seek(SeekFrom::Start(HEADER_2_OFFSET as u64))?;
        file.write_all(&header_data)?;

        let region_table_data =
            create_region_table(bat_offset, bat_size, metadata_offset, metadata_size);

        file.seek(SeekFrom::Start(REGION_TABLE_1_OFFSET as u64))?;
        file.write_all(&region_table_data)?;
        file.seek(SeekFrom::Start(REGION_TABLE_2_OFFSET as u64))?;
        file.write_all(&region_table_data)?;

        if fixed {
            let total_size = virtual_size;
            file.seek(SeekFrom::Start(payload_offset + total_size - 1))?;
            file.write_all(&[0u8])?;
        }

        file.sync_all()?;

        drop(file);
        Self::open_file(path, true)
    }

    fn validate_create_params(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> Result<()> {
        if virtual_size == 0 {
            return Err(Error::InvalidParameter(
                "Virtual size cannot be zero".to_string(),
            ));
        }
        if !block_size.is_power_of_two() || !(MIN_BLOCK_SIZE..=MAX_BLOCK_SIZE).contains(&block_size)
        {
            return Err(Error::InvalidParameter(format!(
                "Block size must be power of 2 between {MIN_BLOCK_SIZE} and {MAX_BLOCK_SIZE}"
            )));
        }
        if logical_sector_size != 512 && logical_sector_size != 4096 {
            return Err(Error::InvalidParameter(
                "Logical sector size must be 512 or 4096".to_string(),
            ));
        }
        Ok(())
    }

    fn calculate_layout(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
        let bat_entries =
            Bat::calculate_total_entries(virtual_size, block_size, logical_sector_size);
        let bat_size = align_1mib(bat_entries * BAT_ENTRY_SIZE as u64);

        let metadata_size = align_1mib(METADATA_TABLE_SIZE as u64 + 256);

        let log_size = MiB;

        let metadata_offset = HEADER_SECTION_SIZE as u64 * 2;
        let bat_offset = metadata_offset + metadata_size;
        let log_offset = bat_offset + bat_size;
        let payload_offset = align_1mib(log_offset + log_size);

        (
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            payload_offset,
            bat_entries,
        )
    }

    fn create_bat_data(
        fixed: bool, bat_entries: u64, payload_offset: u64, block_size: u32,
    ) -> Vec<u8> {
        if fixed {
            let mut entries = vec![0u8; usize::try_from(bat_entries).unwrap_or(0) * BAT_ENTRY_SIZE];
            for i in 0..bat_entries {
                let offset = usize::try_from(i).unwrap_or(0) * BAT_ENTRY_SIZE;
                let payload_offset_mb = (payload_offset + i * u64::from(block_size)) / MiB;
                let state_and_offset = (payload_offset_mb << 20) | 6u64;
                entries[offset..offset + 8].copy_from_slice(&state_and_offset.to_le_bytes());
            }
            entries
        } else {
            vec![0u8; usize::try_from(bat_entries).unwrap_or(0) * BAT_ENTRY_SIZE]
        }
    }
}

pub struct OpenOptions {
    path: std::path::PathBuf,
    write: bool,
}

impl OpenOptions {
    pub const fn write(mut self) -> Self {
        self.write = true;
        self
    }

    pub fn finish(self) -> Result<File> {
        File::open_file(&self.path, self.write)
    }
}

pub struct CreateOptions {
    path: std::path::PathBuf,
    size: Option<u64>,
    fixed: bool,
    has_parent: bool,
    block_size: u32,
    logical_sector_size: u32,
}

impl CreateOptions {
    pub const fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub const fn fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    pub const fn has_parent(mut self, has_parent: bool) -> Self {
        self.has_parent = has_parent;
        self
    }

    pub const fn block_size(mut self, block_size: u32) -> Self {
        self.block_size = block_size;
        self
    }

    pub fn finish(self) -> Result<File> {
        let size = self
            .size
            .ok_or_else(|| Error::InvalidParameter("Virtual disk size is required".to_string()))?;

        File::create_file(
            &self.path,
            size,
            self.fixed,
            self.has_parent,
            self.block_size,
            self.logical_sector_size,
        )
    }
}

fn create_metadata(
    virtual_size: u64, block_size: u32, logical_sector_size: u32, fixed: bool, has_parent: bool,
    disk_id: Guid,
) -> Vec<u8> {
    use crate::common::metadata_guids;

    let mut data = Vec::with_capacity(METADATA_TABLE_SIZE);

    let entry_count: u16 = if has_parent { 6 } else { 5 };
    data.extend_from_slice(METADATA_SIGNATURE);
    data.extend_from_slice(&[0u8; 2]);
    data.extend_from_slice(&entry_count.to_le_bytes());
    data.extend_from_slice(&[0u8; 20]);

    let mut current_offset: u32 = u32::try_from(METADATA_TABLE_SIZE).unwrap_or(0);

    let fp_flags: u32 = u32::from(fixed) | (u32::from(has_parent) << 1);
    data.extend_from_slice(metadata_guids::FILE_PARAMETERS.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&0x04u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 8;

    data.extend_from_slice(metadata_guids::VIRTUAL_DISK_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 8;

    data.extend_from_slice(metadata_guids::VIRTUAL_DISK_ID.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 16;

    data.extend_from_slice(metadata_guids::LOGICAL_SECTOR_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 4;

    data.extend_from_slice(metadata_guids::PHYSICAL_SECTOR_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 4;

    if has_parent {
        data.extend_from_slice(metadata_guids::PARENT_LOCATOR.as_bytes());
        data.extend_from_slice(&current_offset.to_le_bytes());
        data.extend_from_slice(&24u32.to_le_bytes());
        data.extend_from_slice(&0x06u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
    }

    while data.len() < METADATA_TABLE_SIZE {
        data.push(0);
    }

    data.extend_from_slice(&block_size.to_le_bytes());
    data.extend_from_slice(&fp_flags.to_le_bytes());

    data.extend_from_slice(&virtual_size.to_le_bytes());

    data.extend_from_slice(disk_id.as_bytes());

    data.extend_from_slice(&logical_sector_size.to_le_bytes());

    data.extend_from_slice(&logical_sector_size.to_le_bytes());

    data
}

fn create_region_table(
    bat_offset: u64, bat_size: u64, metadata_offset: u64, metadata_size: u64,
) -> Vec<u8> {
    use crate::common::region_guids;

    let mut data = vec![0u8; REGION_TABLE_SIZE];

    data[0..4].copy_from_slice(REGION_TABLE_SIGNATURE);
    data[4..8].copy_from_slice(&[0; 4]);
    data[8..12].copy_from_slice(&2u32.to_le_bytes());
    data[12..16].copy_from_slice(&[0; 4]);

    data[16..32].copy_from_slice(region_guids::BAT_REGION.as_bytes());
    data[32..40].copy_from_slice(&bat_offset.to_le_bytes());
    data[40..44].copy_from_slice(&(u32::try_from(bat_size).unwrap_or(0_u32)).to_le_bytes());
    data[44..48].copy_from_slice(&1u32.to_le_bytes());

    data[48..64].copy_from_slice(region_guids::METADATA_REGION.as_bytes());
    data[64..72].copy_from_slice(&metadata_offset.to_le_bytes());
    data[72..76].copy_from_slice(&(u32::try_from(metadata_size).unwrap_or(0_u32)).to_le_bytes());
    data[76..80].copy_from_slice(&1u32.to_le_bytes());

    let checksum = crc32c::crc32c(&data);
    data[4..8].copy_from_slice(&checksum.to_le_bytes());

    data
}
