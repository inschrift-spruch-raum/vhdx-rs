//! Sections module - manages all VHDX sections with lazy loading

use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom};

use crate::common::constants::*;
use crate::error::{Error, Result};
use crate::types::Guid;

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

/// Metadata Section wrapper
pub struct Metadata {
    inner: metadata::Metadata,
}

impl Metadata {
    /// Create from raw data
    pub fn new(data: Vec<u8>) -> Result<Self> {
        Ok(Self {
            inner: metadata::Metadata::new(data)?,
        })
    }

    /// Return the complete raw bytes
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    /// Access the Metadata Table
    pub fn table(&self) -> crate::sections::metadata::MetadataTable<'_> {
        self.inner.table()
    }

    /// Access the Metadata Items
    pub fn items(&self) -> MetadataItems<'_> {
        self.inner.items()
    }
}

/// Log Section wrapper
pub struct Log {
    inner: log::Log,
}

impl Log {
    /// Create from raw data
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: log::Log::new(data),
        }
    }

    /// Return the complete raw bytes
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    /// Get a log entry by index
    pub fn entry(&self, index: usize) -> Option<LogEntry<'_>> {
        self.inner.entry(index)
    }

    /// Get all valid log entries
    pub fn entries(&self) -> Vec<LogEntry<'_>> {
        self.inner.entries()
    }

    /// Check if log replay is required
    pub fn is_replay_required(&self) -> bool {
        self.inner.is_replay_required()
    }

    /// Replay log entries to recover from crash
    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        self.inner.replay(file)
    }
}

/// Sections container with lazy loading
///
/// Each section is loaded from file only when first accessed.
pub struct Sections {
    file: std::fs::File,

    // Cached sections
    header: RefCell<Option<Header>>,
    bat: RefCell<Option<Bat>>,
    metadata: RefCell<Option<Metadata>>,
    log: RefCell<Option<Log>>,

    // Section locations
    bat_offset: u64,
    bat_size: u64,
    metadata_offset: u64,
    metadata_size: u64,
    log_offset: u64,
    log_size: u64,

    // Calculated from metadata
    entry_count: u64,
}

impl Sections {
    /// Create a new Sections container
    pub fn new(
        file: std::fs::File,
        bat_offset: u64,
        bat_size: u64,
        metadata_offset: u64,
        metadata_size: u64,
        log_offset: u64,
        log_size: u64,
        entry_count: u64,
    ) -> Self {
        Self {
            file,
            header: RefCell::new(None),
            bat: RefCell::new(None),
            metadata: RefCell::new(None),
            log: RefCell::new(None),
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            entry_count,
        }
    }

    /// Access Header Section (lazy loading)
    ///
    /// The Header Section is always at offset 0 and is 1 MB in size.
    pub fn header(&self) -> Result<std::cell::Ref<'_, Header>> {
        if self.header.borrow().is_none() {
            let header_data = self.read_header_section()?;
            *self.header.borrow_mut() = Some(Header::new(header_data)?);
        }
        Ok(std::cell::Ref::map(self.header.borrow(), |h| {
            h.as_ref().unwrap()
        }))
    }

    /// Access BAT Section (lazy loading)
    pub fn bat(&self) -> Result<std::cell::Ref<'_, Bat>> {
        if self.bat.borrow().is_none() {
            let bat_data = self.read_section(self.bat_offset, self.bat_size as usize)?;
            *self.bat.borrow_mut() = Some(Bat::new(bat_data, self.entry_count)?);
        }
        Ok(std::cell::Ref::map(self.bat.borrow(), |b| {
            b.as_ref().unwrap()
        }))
    }

    /// Access Metadata Section (lazy loading)
    pub fn metadata(&self) -> Result<std::cell::Ref<'_, Metadata>> {
        if self.metadata.borrow().is_none() {
            let metadata_data =
                self.read_section(self.metadata_offset, self.metadata_size as usize)?;
            *self.metadata.borrow_mut() = Some(Metadata::new(metadata_data)?);
        }
        Ok(std::cell::Ref::map(self.metadata.borrow(), |m| {
            m.as_ref().unwrap()
        }))
    }

    /// Access Log Section (lazy loading)
    pub fn log(&self) -> Result<std::cell::Ref<'_, Log>> {
        if self.log.borrow().is_none() {
            let log_data = self.read_section(self.log_offset, self.log_size as usize)?;
            *self.log.borrow_mut() = Some(Log::new(log_data));
        }
        Ok(std::cell::Ref::map(self.log.borrow(), |l| {
            l.as_ref().unwrap()
        }))
    }

    /// Read the header section (1 MB at offset 0)
    fn read_header_section(&self) -> Result<Vec<u8>> {
        self.read_section(0, HEADER_SECTION_SIZE)
    }

    /// Read a section from file
    fn read_section(&self, offset: u64, size: usize) -> Result<Vec<u8>> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; size];
        file.read_exact(&mut data)?;
        Ok(data)
    }
}

/// Helper function to compute CRC-32C checksum
/// Used for headers and region tables
pub fn compute_crc32c(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

/// Compute CRC-32C with a field zeroed out
/// Used when verifying checksums where the checksum field itself is part of the data
pub fn crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32 {
    // Manually compute CRC with a zeroed-out section
    let mut data_copy = data.to_vec();
    for i in zero_offset..(zero_offset + zero_len).min(data_copy.len()) {
        data_copy[i] = 0;
    }
    crc32c::crc32c(&data_copy)
}
