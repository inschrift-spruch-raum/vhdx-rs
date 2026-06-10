//! `gpt_disk_io` compatibility layer (optional, behind `gpt` feature).
//!
//! Provides [`VhdxBlockDevice`], a wrapper that adapts a [`crate::Medium`] to the
//! [`gpt_disk_io::BlockIo`] trait. This allows using the VHDX virtual disk as
//! a block device for GPT partition table operations via `gpt_disk_io`.
//!
//! # Usage
//!
//! ```ignore
//! use vhdx::{LogReplayPolicy, Medium};
//! use vhdx::gpt::VhdxBlockDevice;
//! use gpt_disk_io::BlockIo;
//!
//! // Open VHDX Medium
//! let inner = std::fs::OpenOptions::new()
//!     .read(true)
//!     .write(true)
//!     .open("disk.vhdx")?;
//! let medium = Medium::open(inner)
//!     .log_replay(LogReplayPolicy::Auto)
//!     .finish()?;
//!
//! // Wrap as block device
//! let mut block_dev = VhdxBlockDevice::new(medium)?;
//!
//! // Now use with gpt_disk_io
//! println!("Block size: {:?}", block_dev.block_size());
//! println!("Num blocks: {}", block_dev.num_blocks()?);
//! ```

use std::fmt;
use std::io::{Read, Seek, Write};

use gpt_disk_io::BlockIo;
use gpt_disk_types::{BlockSize, Lba};

use crate::Medium;
use crate::error::Error;
use crate::medium::{Len, SetLen, SyncData};

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

/// A block device adapter that wraps a VHDX [`Medium`] and implements
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
pub struct VhdxBlockDevice<T = std::fs::File> {
    medium: Medium<T>,
    sector_size: u32,
    block_size: BlockSize,
    num_blocks: u64,
}

impl<T> VhdxBlockDevice<T> {
    /// Create a new block device adapter from an opened VHDX Medium.
    ///
    /// Extracts the logical sector size and virtual disk size from the VHDX
    /// metadata to determine block geometry.
    ///
    /// # Errors
    ///
    /// Returns an error if the VHDX metadata cannot be read or parsed.
    pub fn new(medium: Medium<T>) -> Result<Self, Error>
    where
        T: Read + Seek,
    {
        let (sector_size, virtual_size) = {
            let sections = medium.sections()?;
            let metadata = sections.metadata()?;
            let items = metadata.items();
            (items.logical_sector_size()?, items.virtual_disk_size()?)
        };

        let block_size = BlockSize::new(sector_size).ok_or_else(|| {
            Error::InvalidMetadata(format!(
                "logical sector size {sector_size} is not a valid GPT block size (minimum 512)"
            ))
        })?;

        // Compute total logical blocks from virtual disk size.
        let num_blocks = virtual_size / u64::from(sector_size);

        Ok(Self {
            medium,
            sector_size,
            block_size,
            num_blocks,
        })
    }

    /// Access the underlying VHDX [`Medium`].
    ///
    /// Useful for VHDX-specific operations (validation, section inspection,
    /// etc.) that are not exposed through the `BlockIo` trait.
    #[must_use]
    pub fn medium(&self) -> &Medium<T> {
        &self.medium
    }

    /// Access the underlying VHDX [`Medium`] mutably.
    pub fn medium_mut(&mut self) -> &mut Medium<T> {
        &mut self.medium
    }

    /// Unwrap into the underlying VHDX [`Medium`].
    #[must_use]
    pub fn into_medium(self) -> Medium<T> {
        self.medium
    }
}

impl<T> BlockIo for VhdxBlockDevice<T>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
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
        let mut io = self.medium.io().map_err(VhdxBlockIoError)?;
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

        let mut io = self.medium.io().map_err(VhdxBlockIoError)?;
        let mut sector = io.sector(lba, count).map_err(VhdxBlockIoError)?;

        sector
            .write_all(src)
            .map_err(|e| VhdxBlockIoError(Error::Io(e)))?;
        sector.flush().map_err(|e| VhdxBlockIoError(Error::Io(e)))?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.medium
            .inner_mut()
            .sync_data()
            .map_err(|e| VhdxBlockIoError(Error::Io(e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_device_accepts_cursor_backed_medium() {
        let cursor = std::io::Cursor::new(Vec::new());
        let medium = Medium::create(cursor)
            .size(16 * 1024 * 1024)
            .finish()
            .expect("create cursor-backed medium");

        let block_device = VhdxBlockDevice::new(medium).expect("wrap cursor-backed medium");
        let medium = block_device.into_medium();
        let _cursor = medium.into_inner();
    }
}
