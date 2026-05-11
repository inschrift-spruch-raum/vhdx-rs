//! `gpt_disk_io` compatibility layer (optional, behind `gpt` feature).
//!
//! Provides [`VhdxBlockDevice`], a wrapper that adapts a [`crate::File`] to the
//! [`gpt_disk_io::BlockIo`] trait. This allows using the VHDX virtual disk as
//! a block device for GPT partition table operations via `gpt_disk_io`.
//!
//! # Usage
//!
//! ```ignore
//! use vhdx::{File, LogReplayPolicy};
//! use vhdx::gpt::VhdxBlockDevice;
//! use gpt_disk_io::BlockIo;
//!
//! // Open VHDX file
//! let file = File::open("disk.vhdx")
//!     .log_replay(LogReplayPolicy::Auto)
//!     .finish()?;
//!
//! // Wrap as block device
//! let mut block_dev = VhdxBlockDevice::new(file)?;
//!
//! // Now use with gpt_disk_io
//! println!("Block size: {:?}", block_dev.block_size());
//! println!("Num blocks: {}", block_dev.num_blocks()?);
//! ```

use std::fmt;

use gpt_disk_io::BlockIo;
use gpt_disk_types::{BlockSize, Lba};

use crate::File;
use crate::error::Error;

// ---------------------------------------------------------------------------
// VhdxBlockIoError
// ---------------------------------------------------------------------------

/// Error type for [`VhdxBlockDevice`] implementing [`BlockIo`].
///
/// Wraps the underlying [`crate::Error`] to satisfy the `BlockIo` trait's
/// associated `Error` type bounds (`Debug + Display + Send + Sync`).
#[derive(Debug)]
pub struct VhdxBlockIoError(pub Error);

impl fmt::Display for VhdxBlockIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for VhdxBlockIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

// ---------------------------------------------------------------------------
// VhdxBlockDevice
// ---------------------------------------------------------------------------

/// A block device adapter that wraps a VHDX [`File`] and implements
/// [`BlockIo`] from `gpt_disk_io`.
///
/// The block size reported to GPT is the VHDX **logical sector size**
/// (512 or 4096 bytes), not the VHDX payload block/chunk size. This maps
/// correctly to the LBA-based addressing used by GPT.
///
/// # Construction
///
/// Created via [`VhdxBlockDevice::new`], which extracts virtual disk
/// geometry from the VHDX metadata and caches it for efficient block
/// operations.
///
/// # Data plane
///
/// All reads and writes go through the VHDX library's sector-level IO
/// pipeline, which handles BAT resolution, block allocation, differencing
/// disk parent fallback, sector bitmap processing, and log replay overlay.
pub struct VhdxBlockDevice {
    file: File,
    sector_size: u32,
    block_size: BlockSize,
    num_blocks: u64,
}

impl VhdxBlockDevice {
    /// Create a new block device adapter from an opened VHDX file.
    ///
    /// Extracts the logical sector size and virtual disk size from the VHDX
    /// metadata to determine block geometry.
    ///
    /// # Errors
    ///
    /// Returns an error if the VHDX metadata cannot be read or parsed.
    pub fn new(file: File) -> Result<Self, Error> {
        let io = file.io()?;

        let sector_size = io.logical_sector_size();
        let block_size = BlockSize::new(sector_size).ok_or_else(|| {
            Error::InvalidMetadata(format!(
                "logical sector size {sector_size} is not a valid GPT block size (minimum 512)"
            ))
        })?;

        // Compute total logical blocks from virtual disk size.
        let sections = file.sections();
        let metadata = sections.metadata()?;
        let virtual_size = metadata.items().virtual_disk_size()?;
        let num_blocks = virtual_size / u64::from(sector_size);

        Ok(Self {
            file,
            sector_size,
            block_size,
            num_blocks,
        })
    }

    /// Access the underlying VHDX [`File`].
    ///
    /// Useful for VHDX-specific operations (validation, section inspection,
    /// etc.) that are not exposed through the `BlockIo` trait.
    #[must_use]
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Access the underlying VHDX [`File`] mutably.
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    /// Unwrap into the underlying VHDX [`File`].
    #[must_use]
    pub fn into_file(self) -> File {
        self.file
    }
}

impl BlockIo for VhdxBlockDevice {
    type Error = VhdxBlockIoError;

    fn block_size(&self) -> BlockSize {
        self.block_size
    }

    fn num_blocks(&mut self) -> Result<u64, Self::Error> {
        Ok(self.num_blocks)
    }

    fn read_blocks(&mut self, start_lba: Lba, dst: &mut [u8]) -> Result<(), Self::Error> {
        use std::io::Read;

        let lba = start_lba.to_u64();
        let sector_size = self.sector_size as usize;

        // Validate buffer alignment.
        assert!(
            dst.len().is_multiple_of(sector_size),
            "read_blocks: dst buffer length {} is not a multiple of sector size {}",
            dst.len(),
            sector_size,
        );

        let count = (dst.len() / sector_size) as u64;

        // Create a temporary IO context (reads from cached buffers, cheap after
        // the first access) and use its sector-level read pipeline.
        let io = self.file.io().map_err(VhdxBlockIoError)?;
        let mut sector = io.sector(lba, count).map_err(VhdxBlockIoError)?;

        sector
            .read_exact(dst)
            .map_err(|e| VhdxBlockIoError(Error::Io(e)))?;

        Ok(())
    }

    fn write_blocks(&mut self, start_lba: Lba, src: &[u8]) -> Result<(), Self::Error> {
        use std::io::Write;

        let lba = start_lba.to_u64();
        let sector_size = self.sector_size as usize;

        // Validate buffer alignment.
        assert!(
            src.len().is_multiple_of(sector_size),
            "write_blocks: src buffer length {} is not a multiple of sector size {}",
            src.len(),
            sector_size,
        );

        let count = (src.len() / sector_size) as u64;

        let io = self.file.io().map_err(VhdxBlockIoError)?;
        let mut sector = io.sector(lba, count).map_err(VhdxBlockIoError)?;

        sector
            .write_all(src)
            .map_err(|e| VhdxBlockIoError(Error::Io(e)))?;
        sector.flush().map_err(|e| VhdxBlockIoError(Error::Io(e)))?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.file
            .inner()
            .sync_all()
            .map_err(|e| VhdxBlockIoError(Error::Io(e)))?;
        Ok(())
    }
}
