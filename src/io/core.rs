//! IO module: sector-level read/write operations for the virtual disk.
//!
//! This is the **sole data-plane entry point**. All virtual disk payload reads
//! and writes must go through [`IO::sector`] → [`Sector`] implementing
//! [`std::io::Read`], [`std::io::Write`], and [`std::io::Seek`].
//! Direct reads via [`File::inner`](crate::file::File::inner) are forbidden
//! for payload data-plane access.
//!
//! # Differencing disk support
//!
//! For differencing (child) disks:
//! - Sector bitmap blocks are checked for [`PayloadBlockState::PartiallyPresent`].
//! - Sectors not present in the child fall back to the parent disk.
//! - The parent file is opened lazily and cached.
//!
//! # Standard
//!
//! MS-VHDX §2.5.1 (BAT state semantics for payload blocks)

use bitvec::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use std::io::{self, SeekFrom};

use crate::bat::{Bat, BatState, PayloadBlockState, SectorBitmapState};
use crate::constants::MIB;
use crate::error::{Error, Result};
use crate::file::File;
use crate::file::ReadSemanticsPolicy;
use crate::log_replay::ReplayOverlay;
use crate::metadata::Metadata;

use super::platform::{read_at, write_at};

// ---------------------------------------------------------------------------
// IO
// ---------------------------------------------------------------------------

/// Virtual disk sector-level I/O.
///
/// Constructed internally from a file reference.
/// The IO struct resolves BAT entries, manages block offsets, and provides
/// the only path to sector-level reads and writes.
///
/// # Standard
///
/// MS-VHDX §2.5.1 — BAT entry state semantics for sector reads.
#[derive(Debug)]
pub struct IO<'a> {
    file: &'a File,
    pub(super) block_size: u32,
    pub(super) logical_sector_size: u32,
    chunk_ratio: u64,
    max_sector: u64,
    /// In-memory replay overlay for serving post-replay data through the read path.
    pub(super) overlay: Option<Arc<ReplayOverlay>>,
    /// Cached parent VHDX file (lazily opened for differencing disks).
    parent_file: RefCell<Option<File>>,
    /// Resolved parent path (cached after first resolution).
    parent_path: RefCell<Option<PathBuf>>,
}

impl<'a> IO<'a> {
    /// Create a new IO context from a file reference.
    ///
    /// Loads metadata from the file to extract block size, sector sizes,
    /// parent status, and chunk ratio.
    pub(crate) fn new(file: &'a File) -> Result<Self> {
        let meta_buf = file.metadata_buf()?;
        let metadata = Metadata::new(meta_buf)?;
        let items = metadata.items();

        let fp = items
            .file_parameters()
            .map_err(|_| Error::InvalidMetadata("FileParameters metadata item not found".into()))?;
        let block_size = fp.block_size();
        if block_size == 0 {
            return Err(Error::InvalidMetadata("block size must be non-zero".into()));
        }

        let logical_sector_size = items.logical_sector_size().ok().unwrap_or(512);
        if logical_sector_size == 0 {
            return Err(Error::InvalidMetadata(
                "logical sector size must be non-zero".into(),
            ));
        }

        let virtual_size = items.virtual_disk_size().map_err(|_| {
            Error::InvalidMetadata("VirtualDiskSize metadata item not found".into())
        })?;

        let max_sector = virtual_size / u64::from(logical_sector_size);

        // chunk_ratio = (2^23 * LogicalSectorSize) / BlockSize
        let chunk_ratio = (1u64 << 23) * u64::from(logical_sector_size) / u64::from(block_size);

        Ok(Self {
            file,
            block_size,
            logical_sector_size,
            chunk_ratio,
            max_sector: max_sector.saturating_sub(1),
            overlay: file.replay_overlay_arc().cloned(),
            parent_file: RefCell::new(None),
            parent_path: RefCell::new(None),
        })
    }

    /// The size of one logical sector in bytes.
    pub fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    /// The payload block size in bytes.
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// Locate and return a [`Sector`] spanning `count` sectors starting at
    /// global sector number `start`.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidParameter`] if `count == 0`, `start + count` overflows,
    ///   or `count * logical_sector_size` overflows.
    /// - [`Error::SectorOutOfBounds`] if the range exceeds the virtual disk.
    pub fn sector(&self, start: u64, count: u64) -> Result<Sector<'_>> {
        if count == 0 {
            return Err(Error::InvalidParameter("count must be >= 1".into()));
        }

        let end_sector = start
            .checked_add(count)
            .ok_or_else(|| Error::InvalidParameter("start + count overflow".into()))?;

        if end_sector - 1 > self.max_sector {
            return Err(Error::SectorOutOfBounds {
                sector: start,
                max: self.max_sector,
            });
        }

        Ok(Sector {
            io: self,
            file: self.file,
            start,
            count,
            logical_sector_size: self.logical_sector_size,
            block_size: self.block_size,
            chunk_ratio: self.chunk_ratio,
            pos: 0,
            range_bytes: count
                .checked_mul(u64::from(self.logical_sector_size))
                .ok_or_else(|| Error::InvalidParameter("sector_count * lss overflow".into()))?,
            semantics: ReadSemanticsPolicy::default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Sector
// ---------------------------------------------------------------------------

/// A handle to one or more logical sectors within a virtual disk block.
///
/// Created by [`IO::sector`].
#[derive(Clone, Debug)]
pub struct Sector<'a> {
    io: &'a IO<'a>,
    file: &'a File,
    start: u64,
    count: u64,
    logical_sector_size: u32,
    block_size: u32,
    chunk_ratio: u64,
    pos: u64,
    range_bytes: u64,
    semantics: ReadSemanticsPolicy,
}

impl Sector<'_> {
    /// Set the read semantics policy for this sector range.
    ///
    /// Controls how Unmapped blocks are handled during reads:
    /// - [`ReadSemanticsPolicy::EffectiveDataPreferred`] (default): return zeros.
    /// - [`ReadSemanticsPolicy::RawDataPreferred`]: read raw on-disk data,
    ///   falling back to zeros on error.
    #[must_use]
    pub fn semantics(mut self, policy: ReadSemanticsPolicy) -> Self {
        self.semantics = policy;
        self
    }

    /// Read data from this sector range at the given byte offset.
    ///
    /// `byte_offset` is relative to the first sector in this range (0-based).
    /// The resulting byte range `[byte_offset, byte_offset + buf.len())` must
    /// fit within `sector_count * logical_sector_size`.
    ///
    /// Respects the sector's semantics policy.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn read_at(&self, buf: &mut [u8], byte_offset: u64) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let range_bytes = self.count * lss as u64;

        // Empty read is a no-op
        if buf.is_empty() {
            return Ok(());
        }

        // Validate byte range
        let byte_end = byte_offset
            .checked_add(buf.len() as u64)
            .ok_or_else(|| Error::InvalidParameter("byte_offset + buf.len() overflow".into()))?;
        if byte_end > range_bytes {
            return Err(Error::InvalidParameter(format!(
                "byte range [{byte_offset}, {byte_end}) exceeds sector range of {range_bytes} bytes"
            )));
        }

        let start_byte = usize::try_from(byte_offset)
            .map_err(|_| Error::InvalidParameter("byte_offset does not fit usize".into()))?;
        let end_byte = start_byte + buf.len();

        let first_sector_rel = start_byte / lss; // relative to start_sector
        let first_skip = start_byte % lss; // bytes to skip in first sector
        let aligned_end = end_byte.is_multiple_of(lss);

        // Fast path: sector-aligned start AND sector-aligned end
        if first_skip == 0 && aligned_end {
            let sectors_to_read = buf.len() / lss;
            return self.read_full_sectors(
                buf,
                self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
                u64::try_from(sectors_to_read).expect("sector count fits u64"),
            );
        }

        // Slow path: need to read full sectors and extract sub-range
        let last_sector_rel = (end_byte - 1) / lss;
        let affected_count = last_sector_rel - first_sector_rel + 1;
        let mut temp = vec![0u8; affected_count * lss];
        self.read_full_sectors(
            &mut temp,
            self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
            u64::try_from(affected_count).expect("sector count fits u64"),
        )?;
        buf.copy_from_slice(&temp[first_skip..first_skip + buf.len()]);
        Ok(())
    }

    /// Read `sector_count` full sectors starting at absolute `start_sector` into `buf`.
    /// `buf.len()` must equal `sector_count * logical_sector_size`.
    /// Respects semantics policy for Unmapped blocks.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn read_full_sectors(
        &self, buf: &mut [u8], start_sector: u64, sector_count: u64,
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let spb = self.sectors_per_block();
        let mut buf_offset = 0usize;
        let mut current_sector = start_sector;
        let mut remaining = sector_count;

        while remaining > 0 {
            let block_idx = current_sector / spb;
            let sector_in_block = current_sector % spb;
            let remaining_in_block = spb - sector_in_block;
            let sectors_this_round = remaining.min(remaining_in_block);
            let bytes_this_round =
                usize::try_from(sectors_this_round).expect("sector count fits usize") * lss;

            let entry = self.resolve_bat_entry_for_block(block_idx)?;
            let state = entry.state()?;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent => {
                        self.read_block_range_from_file(
                            &entry,
                            sector_in_block,
                            sectors_this_round,
                            &mut buf[buf_offset..buf_offset + bytes_this_round],
                        )?;
                    }
                    PayloadBlockState::PartiallyPresent => {
                        self.read_partially_present_range(
                            &entry,
                            block_idx,
                            sector_in_block,
                            sectors_this_round,
                            &mut buf[buf_offset..buf_offset + bytes_this_round],
                        )?;
                    }
                    PayloadBlockState::Unmapped => {
                        if self.semantics == ReadSemanticsPolicy::RawDataPreferred {
                            if self
                                .read_block_range_from_file(
                                    &entry,
                                    sector_in_block,
                                    sectors_this_round,
                                    &mut buf[buf_offset..buf_offset + bytes_this_round],
                                )
                                .is_err()
                            {
                                buf[buf_offset..buf_offset + bytes_this_round].fill(0);
                            }
                        } else {
                            buf[buf_offset..buf_offset + bytes_this_round].fill(0);
                        }
                    }
                    PayloadBlockState::Zero
                    | PayloadBlockState::NotPresent
                    | PayloadBlockState::Undefined => {
                        buf[buf_offset..buf_offset + bytes_this_round].fill(0);
                    }
                },
                BatState::SectorBitmap(_) => {
                    return Err(Error::BlockNotPresent {
                        block_idx,
                        state: "sector bitmap entry (expected payload)".into(),
                    });
                }
            }

            buf_offset += bytes_this_round;
            current_sector += sectors_this_round;
            remaining -= sectors_this_round;
        }

        Ok(())
    }

    /// Write data to this sector range at the given byte offset.
    ///
    /// `byte_offset` is relative to the first sector in this range (0-based).
    /// The resulting byte range `[byte_offset, byte_offset + data.len())` must
    /// fit within `sector_count * logical_sector_size`. Each affected block must
    /// already be allocated (`FullyPresent` or `PartiallyPresent`).
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn write_at(&self, data: &[u8], byte_offset: u64) -> Result<()> {
        if !self.file.is_write() {
            return Err(Error::ReadOnly);
        }

        let lss = self.logical_sector_size as usize;
        let range_bytes = self.count * lss as u64;

        // Empty write is a no-op
        if data.is_empty() {
            return Ok(());
        }

        // Validate byte range
        let byte_end = byte_offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| Error::InvalidParameter("byte_offset + data.len() overflow".into()))?;
        if byte_end > range_bytes {
            return Err(Error::InvalidParameter(format!(
                "byte range [{byte_offset}, {byte_end}) exceeds sector range of {range_bytes} bytes"
            )));
        }

        let start_byte = usize::try_from(byte_offset)
            .map_err(|_| Error::InvalidParameter("byte_offset does not fit usize".into()))?;
        let end_byte = start_byte + data.len();

        let first_sector_rel = start_byte / lss;
        let first_skip = start_byte % lss;
        let aligned_end = end_byte.is_multiple_of(lss);

        // Fast path: sector-aligned start AND sector-aligned end
        if first_skip == 0 && aligned_end {
            let sectors_to_write = data.len() / lss;
            return self.write_full_sectors(
                data,
                self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
                u64::try_from(sectors_to_write).expect("sector count fits u64"),
            );
        }

        // Slow path: read-modify-write
        let last_sector_rel = (end_byte - 1) / lss;
        let affected_count = last_sector_rel - first_sector_rel + 1;

        // 1. Read current data for affected sectors
        let mut temp = vec![0u8; affected_count * lss];
        self.read_full_sectors(
            &mut temp,
            self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
            u64::try_from(affected_count).expect("sector count fits u64"),
        )?;

        // 2. Patch the byte range
        temp[first_skip..first_skip + data.len()].copy_from_slice(data);

        // 3. Write back all affected sectors
        self.write_full_sectors(
            &temp,
            self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
            u64::try_from(affected_count).expect("sector count fits u64"),
        )?;

        Ok(())
    }

    /// Write `sector_count` full sectors starting at absolute `start_sector`.
    /// `data.len()` must equal `sector_count * logical_sector_size`.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn write_full_sectors(&self, data: &[u8], start_sector: u64, sector_count: u64) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let spb = self.sectors_per_block();
        let mut buf_offset = 0usize;
        let mut current_sector = start_sector;
        let mut remaining = sector_count;

        while remaining > 0 {
            let block_idx = current_sector / spb;
            let sector_in_block = current_sector % spb;
            let remaining_in_block = spb - sector_in_block;
            let sectors_this_round = remaining.min(remaining_in_block);
            let bytes_this_round =
                usize::try_from(sectors_this_round).expect("sector count fits usize") * lss;

            let entry = self.resolve_bat_entry_for_block(block_idx)?;
            let state = entry.state()?;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent => {
                        let file_offset =
                            entry.file_offset_mb() * u64::from(MIB) + sector_in_block * lss as u64;
                        write_at(
                            self.file.inner(),
                            &data[buf_offset..buf_offset + bytes_this_round],
                            file_offset,
                        )?;
                    }
                    _ => {
                        return Err(Error::BlockNotPresent {
                            block_idx,
                            state: format!("cannot write to block in state {payload_state:?}"),
                        });
                    }
                },
                BatState::SectorBitmap(_) => {
                    return Err(Error::BlockNotPresent {
                        block_idx,
                        state: "sector bitmap entry (expected payload)".into(),
                    });
                }
            }

            buf_offset += bytes_this_round;
            current_sector += sectors_this_round;
            remaining -= sectors_this_round;
        }

        Ok(())
    }

    // -- Internal helpers ---------------------------------------------------

    /// Number of logical sectors per payload block.
    fn sectors_per_block(&self) -> u64 {
        u64::from(self.block_size) / u64::from(self.logical_sector_size)
    }

    /// Resolve the BAT entry for this sector's block.
    #[cfg(test)]
    pub(super) fn resolve_bat_entry(&self) -> Result<crate::bat::BatEntry<'_>> {
        let block_idx = self.start / self.sectors_per_block();
        self.resolve_bat_entry_for_block(block_idx)
    }

    /// Resolve the BAT entry for a specific block index.
    fn resolve_bat_entry_for_block(&self, block_idx: u64) -> Result<crate::bat::BatEntry<'_>> {
        let bat_buf = self.file.bat_buf()?;
        let bat = Bat::new(bat_buf, self.chunk_ratio);
        let bat_array_idx = block_idx + block_idx / self.chunk_ratio;
        bat.entry(bat_array_idx)
    }

    /// Read a contiguous range of sectors from a payload block in the file.
    ///
    /// `sector_in_block` is the offset of the first sector within the block,
    /// `buf` determines the amount of data to read (its length sets the read size),
    /// and `_sector_count` is currently unused.
    fn read_block_range_from_file(
        &self, entry: &crate::bat::BatEntry<'_>, sector_in_block: u64, _sector_count: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let file_offset = entry.file_offset_mb() * u64::from(MIB) + sector_in_block * lss as u64;

        // Consult replay overlay first (per-block-span)
        if let Some(ref overlay) = self.io.overlay {
            let n = overlay.read(self.file.inner(), file_offset, buf);
            if n > 0 {
                return Ok(());
            }

            // T20: check physical file size gap
            let last_file_offset = overlay.last_file_offset();
            if last_file_offset > 0
                && file_offset < last_file_offset
                && let Ok(metadata) = self.file.inner().metadata()
            {
                let physical_size = metadata.len();
                if file_offset >= physical_size {
                    buf.fill(0);
                    return Ok(());
                }
            }
        }

        read_at(self.file.inner(), buf, file_offset)?;
        Ok(())
    }

    /// Read a range of sectors from a `PartiallyPresent` payload block.
    ///
    /// Each sector is checked against the sector bitmap: if the bit is set
    /// the sector is read from the child file, otherwise from the parent.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn read_partially_present_range(
        &self, entry: &crate::bat::BatEntry<'_>, block_idx: u64, start_sector_in_block: u64,
        sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;

        // Load sector bitmap for this block
        let stride = self.chunk_ratio + 1;
        let chunk_idx = block_idx / self.chunk_ratio;
        let sb_bat_idx = chunk_idx * stride + self.chunk_ratio;

        let bat_buf = self.file.bat_buf()?;
        let bat = Bat::new(bat_buf, self.chunk_ratio);
        let sb_entry = bat.entry(sb_bat_idx)?;

        let sb_state = sb_entry
            .sector_bitmap_state()
            .ok_or(Error::InvalidSectorBitmapState(sb_entry.raw_state()))?;
        if sb_state != SectorBitmapState::Present {
            return Err(Error::StateMismatch {
                state: sb_entry.raw_state(),
                description: "sector bitmap not Present for PartiallyPresent payload".into(),
            });
        }

        let sb_file_offset = sb_entry.file_offset_mb() * u64::from(MIB);
        let bitmap_size = MIB as usize;
        let mut bitmap = vec![0u8; bitmap_size];
        read_at(self.file.inner(), &mut bitmap, sb_file_offset)?;

        let spb = self.sectors_per_block();
        let block_in_chunk = block_idx % self.chunk_ratio;

        for i in 0..sector_count {
            let sib = start_sector_in_block + i;
            let sector_in_chunk = block_in_chunk * spb + sib;
            let byte_idx =
                usize::try_from(sector_in_chunk / 8).expect("bitmap byte index fits usize");

            if byte_idx >= bitmap.len() {
                return Err(Error::InvalidMetadata(format!(
                    "sector bitmap index out of range: byte {byte_idx}"
                )));
            }

            let in_child = bitmap.view_bits::<Lsb0>()
                [usize::try_from(sector_in_chunk).expect("bitmap bit index fits usize")];
            let offset = usize::try_from(i).expect("sector offset fits usize") * lss;

            if in_child {
                self.read_block_range_from_file(entry, sib, 1, &mut buf[offset..offset + lss])?;
            } else {
                self.read_from_parent_sector(
                    block_idx * spb + start_sector_in_block + i,
                    &mut buf[offset..offset + lss],
                )?;
            }
        }

        Ok(())
    }

    /// Read a single sector from the parent disk at the given global sector number.
    ///
    /// Opens and caches the parent file on first access. Falls back to zeros
    /// if the sector is not available in the parent.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn read_from_parent_sector(&self, global_sector: u64, buf: &mut [u8]) -> Result<()> {
        let parent_path = self.resolve_parent_path()?;
        self.ensure_parent_open(&parent_path)?;

        let lss = self.logical_sector_size as usize;
        let parent_ref = self.io.parent_file.borrow();
        let parent = parent_ref.as_ref().ok_or(Error::ParentNotFound)?;

        let meta_buf = parent.metadata_buf()?;
        let meta = Metadata::new(meta_buf)?;
        let items = meta.items();
        let p_block_size = items
            .file_parameters()
            .map_or(32 * MIB, |fp| fp.block_size());
        let p_lss = items.logical_sector_size().ok().unwrap_or(4096);
        let p_chunk_ratio = (1u64 << 23) * u64::from(p_lss) / u64::from(p_block_size);
        let p_sectors_per_block = u64::from(p_block_size) / u64::from(p_lss);

        let p_block_idx = global_sector / p_sectors_per_block;
        let p_sector_in_block = u32::try_from(global_sector % p_sectors_per_block)
            .expect("parent sector offset fits u32");

        let p_bat_buf = parent.bat_buf()?;
        let p_bat = Bat::new(p_bat_buf, p_chunk_ratio);
        let p_bat_array_idx = p_block_idx + p_block_idx / p_chunk_ratio;

        let result: Result<()> = (|| {
            let p_entry = p_bat.entry(p_bat_array_idx)?;
            if p_entry.is_sector_bitmap() {
                buf.fill(0);
                return Ok(());
            }
            match p_entry.state()? {
                BatState::Payload(PayloadBlockState::FullyPresent) => {
                    let file_offset = p_entry.file_offset_mb() * u64::from(MIB)
                        + u64::from(p_sector_in_block) * lss as u64;
                    read_at(parent.inner(), buf, file_offset)?;
                }
                _ => {
                    buf.fill(0);
                }
            }
            Ok(())
        })();

        if let Ok(()) = result {
            Ok(())
        } else {
            buf.fill(0);
            Ok(())
        }
    }

    /// Resolve the parent path from the child's parent locator metadata.
    fn resolve_parent_path(&self) -> Result<PathBuf> {
        {
            let cached = self.io.parent_path.borrow();
            if let Some(ref p) = *cached {
                return Ok(p.clone());
            }
        }

        let meta_buf = self.io.file.metadata_buf()?;
        let meta = Metadata::new(meta_buf)?;
        let items = meta.items();
        let locator = items.parent_locator().map_err(|_| Error::ParentNotFound)?;

        let Ok(parent_path) = locator.resolve_parent_path() else {
            return Err(Error::ParentNotFound);
        };

        *self.io.parent_path.borrow_mut() = Some(parent_path.clone());
        Ok(parent_path)
    }

    /// Open the parent VHDX file if not already cached.
    fn ensure_parent_open(&self, parent_path: &PathBuf) -> Result<()> {
        if self.io.parent_file.borrow().is_some() {
            return Ok(());
        }

        let parent_file = crate::file::File::open(parent_path)
            .log_replay(crate::file::LogReplayPolicy::Require)
            .finish()
            .map_err(|_| Error::ParentNotFound)?;

        *self.io.parent_file.borrow_mut() = Some(parent_file);
        Ok(())
    }
}

// PartialEq cannot be derived because File does not implement PartialEq.
impl PartialEq for Sector<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(
            std::ptr::from_ref::<File>(self.file),
            std::ptr::from_ref::<File>(other.file),
        ) && self.start == other.start
            && self.count == other.count
    }
}

// ---------------------------------------------------------------------------
// std::io trait implementations — cursor-based I/O
// ---------------------------------------------------------------------------

impl io::Read for Sector<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.range_bytes {
            return Ok(0); // EOF
        }
        let available = usize::try_from(self.range_bytes - self.pos).unwrap_or(usize::MAX);
        let to_read = buf.len().min(available);
        self.read_at(&mut buf[..to_read], self.pos)?;
        self.pos += to_read as u64;
        Ok(to_read)
    }
}

impl io::Write for Sector<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pos >= self.range_bytes {
            return Ok(0); // EOF
        }
        let available = usize::try_from(self.range_bytes - self.pos).unwrap_or(usize::MAX);
        let to_write = buf.len().min(available);
        self.write_at(&buf[..to_write], self.pos)?;
        self.pos += to_write as u64;
        Ok(to_write)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // No buffering — writes go directly to file
    }
}

impl io::Seek for Sector<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let new_pos = match from {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                // offset is i64; negative = before end, positive = past end
                i64::try_from(self.range_bytes)
                    .ok()
                    .and_then(|v| v.checked_add(offset))
                    .and_then(|v| u64::try_from(v.max(0)).ok())
                    .unwrap_or(0)
            }
            SeekFrom::Current(offset) => i64::try_from(self.pos)
                .ok()
                .and_then(|v| v.checked_add(offset))
                .and_then(|v| u64::try_from(v.max(0)).ok())
                .unwrap_or(0),
        };
        self.pos = new_pos.min(self.range_bytes);
        Ok(self.pos)
    }
}
