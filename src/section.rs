use crate::error::{Error, Result};
use crate::medium::Medium;
use std::io::{Read, Seek};
use std::sync::{Arc, OnceLock};

// Re-exports for convenience — `vhdx::section::*` mirrors the old section.rs API
pub use crate::header::{
    FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry, RegionTableHeader,
};

// BAT section types
pub use crate::bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};

// Metadata section types
pub use crate::metadata::{
    EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, Metadata, MetadataItems,
    MetadataTable, ParentLocator, StandardItems, TableEntry, TableHeader,
};

// Log section types
pub use crate::log::{
    DataDescriptor, DataSector, Descriptor, Entry, Log, LogEntryHeader, ZeroDescriptor,
};

/// Container for all VHDX sections.
///
/// This struct holds lightweight views of cached section data and provides
/// parsed views of the header, BAT, metadata, and log sections on every call.
pub struct Sections<'a, T = std::fs::File> {
    header: Arc<[u8]>,
    medium: &'a Medium<T>,
    bat: OnceLock<Arc<[u8]>>,
    metadata: OnceLock<Arc<[u8]>>,
    log: OnceLock<Arc<[u8]>>,
}

impl<'a, T> Sections<'a, T>
where
    T: Read + Seek,
{
    /// Create a new Sections view from cached section buffers.
    pub(crate) fn new(header: Arc<[u8]>, medium: &'a Medium<T>) -> Result<Self> {
        Header::new(&header)?;
        Ok(Self {
            header,
            medium,
            bat: OnceLock::new(),
            metadata: OnceLock::new(),
            log: OnceLock::new(),
        })
    }

    // ------------------------------------------------------------------
    // Section accessors
    // ------------------------------------------------------------------

    /// Parse and return the Header section view.
    ///
    /// The header section includes the file type identifier, both VHDX headers,
    /// and both region tables.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized or header parsing fails.
    pub fn header(&self) -> Result<Header<'_>> {
        Header::new(&self.header)
    }

    /// Parse and return the BAT (Block Allocation Table) section view.
    ///
    /// Parses the cached BAT region and uses the chunk ratio computed from the
    /// cached metadata buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized, BAT loading fails,
    /// or metadata needed for chunk ratio is invalid.
    pub fn bat(&self) -> Result<Bat<'_>> {
        if self.metadata.get().is_none() {
            let _ = self.metadata.set(self.medium.metadata_buf()?);
        }
        if self.bat.get().is_none() {
            let _ = self.bat.set(self.medium.bat_buf()?);
        }
        let metadata = self
            .metadata
            .get()
            .ok_or_else(|| Error::InvalidMetadata("metadata cache not loaded".into()))?;
        let chunk_ratio = Self::compute_chunk_ratio(metadata)?;
        let bat = self
            .bat
            .get()
            .ok_or_else(|| Error::InvalidFile("BAT cache not loaded".into()))?;
        Ok(Bat::new(bat, chunk_ratio))
    }

    /// Parse and return the Metadata section view.
    ///
    /// Parses the cached metadata region.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized or metadata parsing fails.
    pub fn metadata(&self) -> Result<Metadata<'_>> {
        if self.metadata.get().is_none() {
            let _ = self.metadata.set(self.medium.metadata_buf()?);
        }
        let metadata = self
            .metadata
            .get()
            .ok_or_else(|| Error::InvalidMetadata("metadata cache not loaded".into()))?;
        Metadata::new(metadata)
    }

    /// Parse and return the Log section view.
    ///
    /// Parses the cached log region.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized or log parsing fails.
    pub fn log(&self) -> Result<Log<'_>> {
        if self.log.get().is_none() {
            let _ = self.log.set(self.medium.log_buf()?);
        }
        let log = self
            .log
            .get()
            .ok_or_else(|| Error::InvalidFile("log cache not loaded".into()))?;
        Log::new(log)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Compute the chunk ratio from metadata:
    /// `chunk_ratio = (2^23 * LogicalSectorSize) / BlockSize`.
    fn compute_chunk_ratio(meta_buf: &[u8]) -> Result<u64> {
        let metadata = Metadata::new(meta_buf)?;
        let items = metadata.items();
        let fp = items
            .file_parameters()
            .map_err(|_| Error::InvalidMetadata("FileParameters metadata item not found".into()))?;
        let block_size = u64::from(fp.block_size());
        if block_size == 0 {
            return Err(Error::InvalidMetadata("block size must be non-zero".into()));
        }
        let logical_sector_size = u64::from(items.logical_sector_size().map_err(|_| {
            Error::InvalidMetadata("LogicalSectorSize metadata item not found".into())
        })?);
        Ok(crate::common::compute_chunk_ratio(
            block_size,
            logical_sector_size,
        ))
    }
}
