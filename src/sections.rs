use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom};

use crate::common::constants::HEADER_SECTION_SIZE;
use crate::error::{Error, Result};

mod bat;
mod header;
mod log;
mod metadata;

pub use bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};
pub use header::{
    FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry, RegionTableHeader,
};
pub use log::{DataDescriptor, DataSector, Descriptor, LogEntry, LogEntryHeader, ZeroDescriptor};
pub use metadata::{
    EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, MetadataItems, MetadataTable,
    ParentLocator, TableEntry, TableHeader,
};

pub struct Metadata {
    inner: metadata::Metadata,
}

impl Metadata {
    pub fn new(data: Vec<u8>) -> Result<Self> {
        Ok(Self {
            inner: metadata::Metadata::new(data)?,
        })
    }

    #[must_use]
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    #[must_use]
    pub fn table(&self) -> crate::sections::metadata::MetadataTable<'_> {
        self.inner.table()
    }

    #[must_use]
    pub fn items(&self) -> MetadataItems<'_> {
        self.inner.items()
    }
}

pub struct Log {
    inner: log::Log,
}

impl Log {
    #[must_use]
    pub const fn new(data: Vec<u8>) -> Self {
        Self {
            inner: log::Log::new(data),
        }
    }

    #[must_use]
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    #[must_use]
    pub const fn entry(&self, index: usize) -> Option<LogEntry<'_>> {
        self.inner.entry(index)
    }

    #[must_use]
    pub fn entries(&self) -> Vec<LogEntry<'_>> {
        self.inner.entries()
    }

    #[must_use]
    pub fn is_replay_required(&self) -> bool {
        self.inner.is_replay_required()
    }

    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        self.inner.replay(file)
    }
}

pub struct SectionsConfig {
    pub file: std::fs::File,
    pub bat_offset: u64,
    pub bat_size: u64,
    pub metadata_offset: u64,
    pub metadata_size: u64,
    pub log_offset: u64,
    pub log_size: u64,
    pub entry_count: u64,
}

pub struct Sections {
    file: std::fs::File,

    header: RefCell<Option<Header>>,
    bat: RefCell<Option<Bat>>,
    metadata: RefCell<Option<Metadata>>,
    log: RefCell<Option<Log>>,

    bat_offset: u64,
    bat_size: u64,
    metadata_offset: u64,
    metadata_size: u64,
    log_offset: u64,
    log_size: u64,

    entry_count: u64,
}

impl Sections {
    #[must_use]
    pub fn new(config: SectionsConfig) -> Self {
        Self {
            file: config.file,
            header: RefCell::new(None),
            bat: RefCell::new(None),
            metadata: RefCell::new(None),
            log: RefCell::new(None),
            bat_offset: config.bat_offset,
            bat_size: config.bat_size,
            metadata_offset: config.metadata_offset,
            metadata_size: config.metadata_size,
            log_offset: config.log_offset,
            log_size: config.log_size,
            entry_count: config.entry_count,
        }
    }

    pub fn header(&self) -> Result<std::cell::Ref<'_, Header>> {
        if self.header.borrow().is_none() {
            let header_data = self.read_header_section()?;
            *self.header.borrow_mut() = Some(Header::new(header_data)?);
        }
        Ok(std::cell::Ref::map(self.header.borrow(), |h| {
            h.as_ref().unwrap()
        }))
    }

    pub fn bat(&self) -> Result<std::cell::Ref<'_, Bat>> {
        if self.bat.borrow().is_none() {
            let bat_size: usize = self.bat_size.try_into().map_err(|_| {
                Error::InvalidFile(format!("BAT size {} exceeds usize::MAX", self.bat_size))
            })?;
            let bat_data = self.read_section(self.bat_offset, bat_size)?;
            *self.bat.borrow_mut() = Some(Bat::new(bat_data, self.entry_count)?);
        }
        Ok(std::cell::Ref::map(self.bat.borrow(), |b| {
            b.as_ref().unwrap()
        }))
    }

    pub fn metadata(&self) -> Result<std::cell::Ref<'_, Metadata>> {
        if self.metadata.borrow().is_none() {
            let metadata_size: usize = self.metadata_size.try_into().map_err(|_| {
                Error::InvalidFile(format!(
                    "Metadata size {} exceeds usize::MAX",
                    self.metadata_size
                ))
            })?;
            let metadata_data = self.read_section(self.metadata_offset, metadata_size)?;
            *self.metadata.borrow_mut() = Some(Metadata::new(metadata_data)?);
        }
        Ok(std::cell::Ref::map(self.metadata.borrow(), |m| {
            m.as_ref().unwrap()
        }))
    }

    pub fn log(&self) -> Result<std::cell::Ref<'_, Log>> {
        if self.log.borrow().is_none() {
            let log_size: usize = self.log_size.try_into().map_err(|_| {
                Error::InvalidFile(format!("Log size {} exceeds usize::MAX", self.log_size))
            })?;
            let log_data = self.read_section(self.log_offset, log_size)?;
            *self.log.borrow_mut() = Some(Log::new(log_data));
        }
        Ok(std::cell::Ref::map(self.log.borrow(), |l| {
            l.as_ref().unwrap()
        }))
    }

    fn read_header_section(&self) -> Result<Vec<u8>> {
        self.read_section(0, HEADER_SECTION_SIZE)
    }

    fn read_section(&self, offset: u64, size: usize) -> Result<Vec<u8>> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; size];
        file.read_exact(&mut data)?;
        Ok(data)
    }
}

pub fn crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32 {
    let mut data_copy = data.to_vec();
    for i in zero_offset..(zero_offset + zero_len).min(data_copy.len()) {
        data_copy[i] = 0;
    }
    crc32c::crc32c(&data_copy)
}
