use std::sync::OnceLock;

use crate::bat::Bat;
use crate::error::{Error, Result};
use crate::file::File;
use crate::header::Header;
use crate::log::Log;
use crate::metadata::Metadata;

/// Container for all VHDX sections (lazy-loaded).
///
/// This struct holds a reference to a [`File`] and provides lazy access
/// to parsed section views: header, BAT, metadata, and log.
pub struct Sections<'a> {
    file: Option<&'a File>,
    header_cache: OnceLock<Header<'static>>,
    bat_cache: OnceLock<Bat<'static>>,
    metadata_cache: OnceLock<Metadata<'static>>,
    log_cache: OnceLock<Log<'static>>,
}

impl<'a> Sections<'a> {
    /// Create a placeholder Sections (for use before lazy loading is wired up).
    pub(crate) const fn empty() -> Sections<'static> {
        Sections {
            file: None,
            header_cache: OnceLock::new(),
            bat_cache: OnceLock::new(),
            metadata_cache: OnceLock::new(),
            log_cache: OnceLock::new(),
        }
    }

    /// Create a new Sections bound to a file reference.
    #[allow(dead_code)]
    pub(crate) fn new(file: &'a File) -> Self {
        Self {
            file: Some(file),
            header_cache: OnceLock::new(),
            bat_cache: OnceLock::new(),
            metadata_cache: OnceLock::new(),
            log_cache: OnceLock::new(),
        }
    }

    // ------------------------------------------------------------------
    // Section accessors
    // ------------------------------------------------------------------

    /// Parse and return the Header section view.
    ///
    /// The header section includes the file type identifier, both VHDX headers,
    /// and both region tables.
    pub fn header(&self) -> Result<Header<'_>> {
        let file = self.file.ok_or_else(|| {
            Error::InvalidFile("Sections not initialized".into())
        })?;
        if let Some(cached) = self.header_cache.get() {
            return Ok(*cached);
        }
        let parsed = Header::new(file.header_buf())?;
        // SAFETY: `Sections` is stored inside `File` via `OnceLock`, so the
        // parsed header's underlying data lives as long as the `File`. The
        // `'static` is a contained fiction, just like the `Sections` transmute
        // in `File::sections()`.
        let static_parsed: Header<'static> = unsafe { std::mem::transmute(parsed) };
        let _ = self.header_cache.set(static_parsed);
        Ok(static_parsed)
    }

    /// Parse and return the BAT (Block Allocation Table) section view.
    ///
    /// Lazily loads the BAT region from the file and computes the chunk ratio
    /// from metadata.
    pub fn bat(&self) -> Result<Bat<'_>> {
        let file = self.file.ok_or_else(|| {
            Error::InvalidFile("Sections not initialized".into())
        })?;
        if let Some(cached) = self.bat_cache.get() {
            return Ok(*cached);
        }
        let bat_buf = file.bat_buf()?;
        let chunk_ratio = self.compute_chunk_ratio(file)?;
        let parsed = Bat::new(bat_buf, chunk_ratio);
        // SAFETY: same justification as `header()` above.
        let static_parsed: Bat<'static> = unsafe { std::mem::transmute(parsed) };
        let _ = self.bat_cache.set(static_parsed);
        Ok(static_parsed)
    }

    /// Parse and return the Metadata section view.
    ///
    /// Lazily loads the metadata region from the file.
    pub fn metadata(&self) -> Result<Metadata<'_>> {
        let file = self.file.ok_or_else(|| {
            Error::InvalidFile("Sections not initialized".into())
        })?;
        if let Some(cached) = self.metadata_cache.get() {
            return Ok(*cached);
        }
        let meta_buf = file.metadata_buf()?;
        let parsed = Metadata::new(meta_buf)?;
        // SAFETY: same justification as `header()` above.
        let static_parsed: Metadata<'static> = unsafe { std::mem::transmute(parsed) };
        let _ = self.metadata_cache.set(static_parsed);
        Ok(static_parsed)
    }

    /// Parse and return the Log section view.
    ///
    /// Lazily loads the log region from the file.
    pub fn log(&self) -> Result<Log<'_>> {
        let file = self.file.ok_or_else(|| {
            Error::InvalidFile("Sections not initialized".into())
        })?;
        if let Some(cached) = self.log_cache.get() {
            return Ok(*cached);
        }
        let log_buf = file.log_buf()?;
        let parsed = Log::new(log_buf)?;
        // SAFETY: same justification as `header()` above.
        let static_parsed: Log<'static> = unsafe { std::mem::transmute(parsed) };
        let _ = self.log_cache.set(static_parsed);
        Ok(static_parsed)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Compute the chunk ratio from metadata:
    /// `chunk_ratio = (2^23 * LogicalSectorSize) / BlockSize`.
    fn compute_chunk_ratio(&self, file: &File) -> Result<u64> {
        let meta_buf = file.metadata_buf()?;
        let metadata = Metadata::new(meta_buf)?;
        let items = metadata.items();
        let fp = items.file_parameters().map_err(|_| {
            Error::InvalidMetadata("FileParameters metadata item not found".into())
        })?;
        let block_size = fp.block_size() as u64;
        if block_size == 0 {
            return Err(Error::InvalidMetadata("block size must be non-zero".into()));
        }
        let logical_sector_size = items.logical_sector_size().map_err(|_| {
            Error::InvalidMetadata("LogicalSectorSize metadata item not found".into())
        })? as u64;
        Ok(crate::common::compute_chunk_ratio(block_size, logical_sector_size))
    }
}
