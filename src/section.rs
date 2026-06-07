use crate::error::{Error, Result};
use crate::file::File;

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
/// This struct holds a reference to a [`File`] and provides parsed views
/// of the header, BAT, metadata, and log sections on every call.
pub struct Sections<'a> {
    file: &'a File,
}

impl<'a> Sections<'a> {
    /// Create a new Sections bound to a file reference.
    pub(crate) fn new(file: &'a File) -> Self {
        Self { file }
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
    pub fn header(&self) -> Result<Header<'a>> {
        Header::new(self.file.header_buf())
    }

    /// Parse and return the BAT (Block Allocation Table) section view.
    ///
    /// Lazily loads the BAT region from the file and computes the chunk ratio
    /// from metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized, BAT loading fails,
    /// or metadata needed for chunk ratio is invalid.
    pub fn bat(&self) -> Result<Bat<'a>> {
        let bat_buf = self.file.bat_buf()?;
        let chunk_ratio = Self::compute_chunk_ratio(self.file)?;
        Ok(Bat::owned(bat_buf, chunk_ratio))
    }

    /// Parse and return the Metadata section view.
    ///
    /// Lazily loads the metadata region from the file.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized or metadata parsing fails.
    pub fn metadata(&self) -> Result<Metadata<'a>> {
        Metadata::new(self.file.metadata_buf()?)
    }

    /// Parse and return the Log section view.
    ///
    /// Lazily loads the log region from the file.
    ///
    /// # Errors
    ///
    /// Returns an error if sections are uninitialized or log parsing fails.
    pub fn log(&self) -> Result<Log<'a>> {
        Log::new(self.file.log_buf()?)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Compute the chunk ratio from metadata:
    /// `chunk_ratio = (2^23 * LogicalSectorSize) / BlockSize`.
    fn compute_chunk_ratio(file: &File) -> Result<u64> {
        let meta_buf = file.metadata_buf()?;
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
