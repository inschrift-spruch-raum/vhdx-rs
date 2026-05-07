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
use crate::error::{Error, Result};
use crate::file::File;
use crate::file::ReadSemanticsPolicy;
use crate::log_replay::ReplayOverlay;
use crate::metadata::Metadata;

// ---------------------------------------------------------------------------
// IO
// ---------------------------------------------------------------------------

/// Virtual disk sector-level I/O.
///
/// Created via [`IO::new`] by passing a file reference.
/// The IO struct resolves BAT entries, manages block offsets, and provides
/// the only path to sector-level reads and writes.
///
/// # Standard
///
/// MS-VHDX §2.5.1 — BAT entry state semantics for sector reads.
#[derive(Debug)]
pub struct IO<'a> {
    file: &'a File,
    block_size: u32,
    logical_sector_size: u32,
    #[allow(dead_code)]
    has_parent: bool,
    chunk_ratio: u64,
    #[allow(dead_code)]
    sectors_per_block: u64,
    max_sector: u64,
    /// In-memory replay overlay for serving post-replay data through the read path.
    overlay: Option<Arc<ReplayOverlay>>,
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

        let fp = items.file_parameters().map_err(|_| {
            Error::InvalidMetadata("FileParameters metadata item not found".into())
        })?;
        let block_size = fp.block_size();
        if block_size == 0 {
            return Err(Error::InvalidMetadata(
                "block size must be non-zero".into(),
            ));
        }
        let has_parent = fp.has_parent();

        let logical_sector_size = items
            .logical_sector_size()
            .ok()
            .unwrap_or(512);
        if logical_sector_size == 0 {
            return Err(Error::InvalidMetadata(
                "logical sector size must be non-zero".into(),
            ));
        }

        let virtual_size = items
            .virtual_disk_size()
            .map_err(|_| {
                Error::InvalidMetadata("VirtualDiskSize metadata item not found".into())
            })?;

        let sectors_per_block = block_size as u64 / logical_sector_size as u64;
        let max_sector = virtual_size / logical_sector_size as u64;

        // chunk_ratio = (2^23 * LogicalSectorSize) / BlockSize
        let chunk_ratio =
            (1u64 << 23) * logical_sector_size as u64 / block_size as u64;

        Ok(Self {
            file,
            block_size,
            logical_sector_size,
            has_parent,
            chunk_ratio,
            sectors_per_block,
            max_sector: max_sector.saturating_sub(1),
            overlay: file.replay_overlay_arc().cloned(),
            parent_file: RefCell::new(None),
            parent_path: RefCell::new(None),
        })
    }

    /// The size of one logical sector in bytes.
    pub(crate) fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    /// The payload block size in bytes.
    pub(crate) fn block_size(&self) -> u32 {
        self.block_size
    }

    /// Locate and return a [`Sector`] spanning `count` sectors starting at
    /// global sector number `start`.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidParameter`] if `count == 0` or `start + count` overflows.
    /// - [`Error::SectorOutOfBounds`] if the range exceeds the virtual disk.
    pub fn sector(&self, start: u64, count: u64) -> Result<Sector<'_>> {
        if count == 0 {
            return Err(Error::InvalidParameter("count must be >= 1".into()));
        }

        let end_sector = start.checked_add(count).ok_or_else(|| {
            Error::InvalidParameter("start + count overflow".into())
        })?;

        if end_sector - 1 > self.max_sector {
            return Err(Error::SectorOutOfBounds {
                sector: start,
                max: self.max_sector,
            });
        }

        Ok(Sector {
            io: self,
            file: self.file,
            start_sector: start,
            sector_count: count,
            logical_sector_size: self.logical_sector_size,
            block_size: self.block_size,
            chunk_ratio: self.chunk_ratio,
            pos: 0,
            range_bytes: (count as u64).checked_mul(self.logical_sector_size as u64)
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
    start_sector: u64,
    sector_count: u64,
    logical_sector_size: u32,
    block_size: u32,
    chunk_ratio: u64,
    pos: u64,
    range_bytes: u64,
    semantics: ReadSemanticsPolicy,
}

impl<'a> Sector<'a> {
    /// Set the read semantics policy for this sector range.
    ///
    /// Controls how Unmapped blocks are handled during reads:
    /// - [`ReadSemanticsPolicy::EffectiveDataPreferred`] (default): return zeros.
    /// - [`ReadSemanticsPolicy::RawDataPreferred`]: read raw on-disk data,
    ///   falling back to zeros on error.
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
    fn read_at(&self, buf: &mut [u8], byte_offset: u64) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let range_bytes = self.sector_count as u64 * lss as u64;

        // Empty read is a no-op
        if buf.is_empty() {
            return Ok(());
        }

        // Validate byte range
        let byte_end = byte_offset.checked_add(buf.len() as u64)
            .ok_or_else(|| Error::InvalidParameter("byte_offset + buf.len() overflow".into()))?;
        if byte_end > range_bytes {
            return Err(Error::InvalidParameter(format!(
                "byte range [{}, {}) exceeds sector range of {} bytes",
                byte_offset, byte_end, range_bytes
            )));
        }

        let start_byte = byte_offset as usize;
        let end_byte = start_byte + buf.len();

        let first_sector_rel = start_byte / lss;    // relative to start_sector
        let first_skip = start_byte % lss;           // bytes to skip in first sector
        let aligned_end = end_byte % lss == 0;

        // Fast path: sector-aligned start AND sector-aligned end
        if first_skip == 0 && aligned_end {
            let sectors_to_read = buf.len() / lss;
            return self.read_full_sectors(
                buf,
                self.start_sector + first_sector_rel as u64,
                sectors_to_read as u64,
            );
        }

        // Slow path: need to read full sectors and extract sub-range
        let last_sector_rel = (end_byte - 1) / lss;
        let affected_count = last_sector_rel - first_sector_rel + 1;
        let mut temp = vec![0u8; affected_count as usize * lss];
        self.read_full_sectors(
            &mut temp,
            self.start_sector + first_sector_rel as u64,
            affected_count as u64,
        )?;
        buf.copy_from_slice(&temp[first_skip..first_skip + buf.len()]);
        Ok(())
    }

    /// Read `sector_count` full sectors starting at absolute `start_sector` into `buf`.
    /// `buf.len()` must equal `sector_count * logical_sector_size`.
    /// Respects semantics policy for Unmapped blocks.
    fn read_full_sectors(&self, buf: &mut [u8], start_sector: u64, sector_count: u64) -> Result<()> {
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
            let bytes_this_round = sectors_this_round as usize * lss;

            let entry = self.resolve_bat_entry_for_block(block_idx)?;
            let state = entry.state()?;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent => {
                        self.read_block_range_from_file(
                            &entry, sector_in_block, sectors_this_round,
                            &mut buf[buf_offset..buf_offset + bytes_this_round],
                        )?;
                    }
                    PayloadBlockState::PartiallyPresent => {
                        self.read_partially_present_range(
                            &entry, block_idx, sector_in_block, sectors_this_round,
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
    /// already be allocated (FullyPresent or PartiallyPresent).
    fn write_at(&self, data: &[u8], byte_offset: u64) -> Result<()> {
        if !self.file.is_write() {
            return Err(Error::ReadOnly);
        }

        let lss = self.logical_sector_size as usize;
        let range_bytes = self.sector_count as u64 * lss as u64;

        // Empty write is a no-op
        if data.is_empty() {
            return Ok(());
        }

        // Validate byte range
        let byte_end = byte_offset.checked_add(data.len() as u64)
            .ok_or_else(|| Error::InvalidParameter("byte_offset + data.len() overflow".into()))?;
        if byte_end > range_bytes {
            return Err(Error::InvalidParameter(format!(
                "byte range [{}, {}) exceeds sector range of {} bytes",
                byte_offset, byte_end, range_bytes
            )));
        }

        let start_byte = byte_offset as usize;
        let end_byte = start_byte + data.len();

        let first_sector_rel = start_byte / lss;
        let first_skip = start_byte % lss;
        let aligned_end = end_byte % lss == 0;

        // Fast path: sector-aligned start AND sector-aligned end
        if first_skip == 0 && aligned_end {
            let sectors_to_write = data.len() / lss;
            return self.write_full_sectors(
                data,
                self.start_sector + first_sector_rel as u64,
                sectors_to_write as u64,
            );
        }

        // Slow path: read-modify-write
        let last_sector_rel = (end_byte - 1) / lss;
        let affected_count = last_sector_rel - first_sector_rel + 1;

        // 1. Read current data for affected sectors
        let mut temp = vec![0u8; affected_count as usize * lss];
        self.read_full_sectors(
            &mut temp,
            self.start_sector + first_sector_rel as u64,
            affected_count as u64,
        )?;

        // 2. Patch the byte range
        temp[first_skip..first_skip + data.len()].copy_from_slice(data);

        // 3. Write back all affected sectors
        self.write_full_sectors(
            &temp,
            self.start_sector + first_sector_rel as u64,
            affected_count as u64,
        )?;

        Ok(())
    }

    /// Write `sector_count` full sectors starting at absolute `start_sector`.
    /// `data.len()` must equal `sector_count * logical_sector_size`.
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
            let bytes_this_round = sectors_this_round as usize * lss;

            let entry = self.resolve_bat_entry_for_block(block_idx)?;
            let state = entry.state()?;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent => {
                        let file_offset = entry.file_offset_mb() * 1024 * 1024
                            + sector_in_block * lss as u64;
                        write_at(
                            self.file.inner(),
                            &data[buf_offset..buf_offset + bytes_this_round],
                            file_offset,
                        )?;
                    }
                    _ => return Err(Error::BlockNotPresent {
                        block_idx,
                        state: format!("cannot write to block in state {:?}", payload_state),
                    }),
                },
                BatState::SectorBitmap(_) => return Err(Error::BlockNotPresent {
                    block_idx,
                    state: "sector bitmap entry (expected payload)".into(),
                }),
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
        self.block_size as u64 / self.logical_sector_size as u64
    }

    /// Resolve the BAT entry for this sector's block.
    #[cfg(test)]
    fn resolve_bat_entry(&self) -> Result<crate::bat::BatEntry<'_>> {
        let block_idx = self.start_sector / self.sectors_per_block();
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
    /// `sector_count` is how many sectors to read, and `buf` receives the data.
    fn read_block_range_from_file(
        &self,
        entry: &crate::bat::BatEntry<'_>,
        sector_in_block: u64,
        _sector_count: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let file_offset = entry.file_offset_mb() * 1024 * 1024
            + sector_in_block * lss as u64;

        // Consult replay overlay first (per-block-span)
        if let Some(ref overlay) = self.io.overlay {
            let n = overlay.read(self.file.inner(), file_offset, buf)?;
            if n > 0 {
                return Ok(());
            }

            // T20: check physical file size gap
            let last_file_offset = overlay.last_file_offset();
            if last_file_offset > 0 && file_offset < last_file_offset {
                if let Ok(metadata) = self.file.inner().metadata() {
                    let physical_size = metadata.len();
                    if file_offset >= physical_size {
                        buf.fill(0);
                        return Ok(());
                    }
                }
            }
        }

        read_at(self.file.inner(), buf, file_offset)?;
        Ok(())
    }

    /// Read a range of sectors from a PartiallyPresent payload block.
    ///
    /// Each sector is checked against the sector bitmap: if the bit is set
    /// the sector is read from the child file, otherwise from the parent.
    fn read_partially_present_range(
        &self,
        entry: &crate::bat::BatEntry<'_>,
        block_idx: u64,
        start_sector_in_block: u64,
        sector_count: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;

        // Load sector bitmap for this block
        let stride = self.chunk_ratio + 1;
        let chunk_idx = block_idx / self.chunk_ratio;
        let sb_bat_idx = chunk_idx * stride + self.chunk_ratio;

        let bat_buf = self.file.bat_buf()?;
        let bat = Bat::new(bat_buf, self.chunk_ratio);
        let sb_entry = bat.entry(sb_bat_idx)?;

        let sb_state = sb_entry.sector_bitmap_state()
            .ok_or(Error::InvalidSectorBitmapState(sb_entry.raw_state()))?;
        if sb_state != SectorBitmapState::Present {
            return Err(Error::StateMismatch {
                state: sb_entry.raw_state(),
                description: "sector bitmap not Present for PartiallyPresent payload".into(),
            });
        }

        let sb_file_offset = sb_entry.file_offset_mb() * (1024 * 1024);
        let bitmap_size = 1024 * 1024;
        let mut bitmap = vec![0u8; bitmap_size];
        read_at(self.file.inner(), &mut bitmap, sb_file_offset)?;

        let spb = self.sectors_per_block();
        let block_in_chunk = block_idx % self.chunk_ratio;

        for i in 0..sector_count {
            let sib = start_sector_in_block + i;
            let sector_in_chunk = block_in_chunk * spb + sib;
            let byte_idx = (sector_in_chunk / 8) as usize;

            if byte_idx >= bitmap.len() {
                return Err(Error::InvalidMetadata(format!(
                    "sector bitmap index out of range: byte {byte_idx}"
                )));
            }

            let in_child = bitmap.view_bits::<Lsb0>()[sector_in_chunk as usize];
            let offset = i as usize * lss;

            if in_child {
                self.read_block_range_from_file(
                    entry, sib, 1, &mut buf[offset..offset + lss],
                )?;
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
    fn read_from_parent_sector(
        &self,
        global_sector: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let parent_path = self.resolve_parent_path()?;
        self.ensure_parent_open(&parent_path)?;

        let lss = self.logical_sector_size as usize;
        let parent_ref = self.io.parent_file.borrow();
        let parent = parent_ref.as_ref().ok_or_else(|| Error::ParentNotFound)?;

        let meta_buf = parent.metadata_buf()?;
        let meta = Metadata::new(meta_buf)?;
        let items = meta.items();
        let p_block_size = items.file_parameters()
            .map(|fp| fp.block_size())
            .unwrap_or(32 * 1024 * 1024);
        let p_lss = items.logical_sector_size().ok().unwrap_or(4096);
        let p_chunk_ratio = (1u64 << 23) * p_lss as u64 / p_block_size as u64;
        let p_sectors_per_block = p_block_size as u64 / p_lss as u64;

        let p_block_idx = global_sector / p_sectors_per_block;
        let p_sector_in_block = (global_sector % p_sectors_per_block) as u32;

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
                    let file_offset = p_entry.file_offset_mb() * 1024 * 1024
                        + p_sector_in_block as u64 * lss as u64;
                    read_at(parent.inner(), buf, file_offset)?;
                }
                _ => {
                    buf.fill(0);
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(_) => {
                buf.fill(0);
                Ok(())
            }
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

        let parent_path = match locator.resolve_parent_path() {
            Ok(p) => p,
            Err(_) => return Err(Error::ParentNotFound),
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
        std::ptr::eq(self.file as *const File, other.file as *const File)
            && self.start_sector == other.start_sector
            && self.sector_count == other.sector_count
    }
}

// ---------------------------------------------------------------------------
// std::io trait implementations — cursor-based I/O
// ---------------------------------------------------------------------------

impl io::Read for Sector<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.range_bytes {
            return Ok(0);  // EOF
        }
        let available = (self.range_bytes - self.pos) as usize;
        let to_read = buf.len().min(available);
        self.read_at(&mut buf[..to_read], self.pos)?;
        self.pos += to_read as u64;
        Ok(to_read)
    }
}

impl io::Write for Sector<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pos >= self.range_bytes {
            return Ok(0);  // EOF
        }
        let available = (self.range_bytes - self.pos) as usize;
        let to_write = buf.len().min(available);
        self.write_at(&buf[..to_write], self.pos)?;
        self.pos += to_write as u64;
        Ok(to_write)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())  // No buffering — writes go directly to file
    }
}

impl io::Seek for Sector<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let new_pos = match from {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                // offset is i64; negative = before end, positive = past end
                (self.range_bytes as i64)
                    .checked_add(offset)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or(0)
            }
            SeekFrom::Current(offset) => {
                (self.pos as i64)
                    .checked_add(offset)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or(0)
            }
        };
        self.pos = new_pos.min(self.range_bytes);
        Ok(self.pos)
    }
}

// ---------------------------------------------------------------------------
// Platform-specific pread/pwrite helpers
// ---------------------------------------------------------------------------

/// Read from a file at a specific offset without moving the cursor.
#[cfg(unix)]
fn read_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn read_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

/// Write to a file at a specific offset without moving the cursor.
#[cfg(unix)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(buf, offset)
}

#[cfg(windows)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(buf, offset)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::File as VhdxFile;
    use std::io::{Read, Write, Seek, SeekFrom, ErrorKind};

    /// Owns the temp directory and the VHDX file, ensuring cleanup on drop.
    struct TestContext {
        _dir: tempfile::TempDir,
        file: VhdxFile,
        overlay: Option<Arc<ReplayOverlay>>,
    }

    impl TestContext {
        /// Create a new IO context borrowing the owned file.
        fn io(&self) -> IO<'_> {
            let mut io = IO::new(&self.file).expect("create IO");
            io.overlay = self.overlay.clone();
            io
        }
    }

    /// Helper: create a small dynamic VHDX and return an IO for it.
    ///
    /// Uses File::create to produce a known-good test file with
    /// block_size=32MB, logical_sector_size=4096.
    fn create_test_io() -> TestContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.vhdx");

        VhdxFile::create(&path)
            .size(256 * 1024 * 1024) // 256 MB virtual
            .block_size(32 * 1024 * 1024)
            .logical_sector_size(4096)
            .finish()
            .expect("create test vhdx");

        let file = VhdxFile::open(&path)
            .finish()
            .expect("re-open test vhdx");

        TestContext { _dir: dir, file, overlay: None }
    }

    #[test]
    fn io_creation_from_test_file() {
        let ctx = create_test_io();
        let io = ctx.io();
        assert!(io.block_size > 0);
        assert_eq!(io.block_size, 32 * 1024 * 1024);
        assert!(io.logical_sector_size > 0);
        assert_eq!(io.logical_sector_size, 4096);
        assert!(!io.has_parent);
    }

    #[test]
    fn sector_out_of_bounds() {
        let ctx = create_test_io();
        let io = ctx.io();
        // start + count overflow → InvalidParameter
        let result = io.sector(u64::MAX, 1);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidParameter(..)
        ));
    }

    #[test]
    fn sector_zero_is_valid() {
        let ctx = create_test_io();
        let io = ctx.io();
        let result = io.sector(0, 1);
        assert!(result.is_ok(), "sector 0 failed: {:?}", result.err());
    }

    #[test]
    fn sector_read_returns_logical_sector_size() {
        let ctx = create_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("get sector 0");
        let mut buf = vec![0u8; 4096];
        sector
            .read_exact(&mut buf)
            .expect("read sector 0");
    }

    #[test]
    fn sector_read_byte_range_exceeds_range() {
        let ctx = create_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("get sector 0");
        let mut buf = [0u8; 4097]; // 1 byte too many for a single 4096-byte sector
        let n = sector.read(&mut buf).expect("should read what's available");
        assert_eq!(n, 4096, "reads max available");
    }

    #[test]
    fn sector_zero_read_is_all_zeros_for_dynamic_disk() {
        let ctx = create_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");
        let mut buf = vec![0xFFu8; 4096];
        sector
            .read_exact(&mut buf)
            .expect("read sector 0");
        // Dynamic disk: sector 0 is in a NotPresent block → should be zeros
        assert!(buf.iter().all(|&b| b == 0), "expected all zeros, got non-zero data");
    }

    #[test]
    fn sector_write_fails_on_read_only() {
        let ctx = create_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");
        let data = vec![0x42u8; 4096];
        let result = sector.write(&data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);
    }

    // -- Overlay read-through tests ----------------------------------------

    /// Helper: create a fixed VHDX so that blocks are FullyPresent,
    /// then attach a ReplayOverlay to the IO.
    fn create_fixed_io_with_overlay(
        overlay: ReplayOverlay,
    ) -> TestContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-fixed.vhdx");

        VhdxFile::create(&path)
            .size(4 * 1024 * 1024) // 4 MB virtual
            .block_size(1 * 1024 * 1024) // 1 MB blocks
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create fixed test vhdx");

        let file = VhdxFile::open(&path)
            .finish()
            .expect("re-open test vhdx");

        TestContext { _dir: dir, file, overlay: Some(Arc::new(overlay)) }
    }

    /// Helper: resolve the file offset for sector 0's payload data.
    fn sector_zero_file_offset(io: &IO<'_>) -> u64 {
        let sector = io.sector(0, 1).expect("sector 0");
        let entry = sector.resolve_bat_entry().expect("resolve BAT");
        entry.file_offset_mb() * 1024 * 1024
    }

    #[test]
    fn overlay_data_served_through_sector_read() {
        use std::collections::HashMap;

        // Build a minimal overlay with a known sector.
        let dir = tempfile::tempdir().expect("tempdir for baseline");
        let path = dir.path().join("base.vhdx");
        VhdxFile::create(&path)
            .size(4 * 1024 * 1024)
            .block_size(1 * 1024 * 1024)
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create baseline fixed vhdx");

        let baseline_file = VhdxFile::open(&path)
            .finish()
            .expect("open baseline");
        let baseline_ctx = TestContext { _dir: dir, file: baseline_file, overlay: None };
        let baseline_io = baseline_ctx.io();
        let payload_offset = sector_zero_file_offset(&baseline_io);

        // Construct overlay with a sector full of 0xAA at the payload offset.
        let mut sectors = HashMap::new();
        sectors.insert(payload_offset, vec![0xAAu8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let ctx = create_fixed_io_with_overlay(overlay);
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");
        let mut buf = vec![0u8; 4096];
        sector
            .read_exact(&mut buf)
            .expect("read sector 0 with overlay");
        assert!(
            buf.iter().all(|&b| b == 0xAA),
            "expected all 0xAA from overlay, got {:?}",
            &buf[..32]
        );
    }

    #[test]
    fn no_overlay_falls_through_to_file() {
        // Create a fixed VHDX — all blocks are FullyPresent, filled with zeros.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("no-overlay.vhdx");
        VhdxFile::create(&path)
            .size(4 * 1024 * 1024)
            .block_size(1 * 1024 * 1024)
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create fixed vhdx");

        let file = VhdxFile::open(&path)
            .finish()
            .expect("open fixed vhdx");
        let ctx = TestContext { _dir: dir, file, overlay: None };
        let io = ctx.io();
        // No overlay — should be None.
        assert!(io.overlay.is_none(), "expected no overlay");

        let mut sector = io.sector(0, 1).expect("sector 0");
        let mut buf = vec![0xFFu8; 4096];
        sector
            .read_exact(&mut buf)
            .expect("read sector 0");
        // Fixed disk: sector 0 is in a FullyPresent block, zero-filled on create.
        assert!(
            buf.iter().all(|&b| b == 0),
            "expected all zeros from file, got non-zero"
        );
    }

    #[test]
    fn overlay_zero_region_served_through_sector_read() {
        use std::collections::HashMap;

        // Build baseline to find payload offset.
        let dir = tempfile::tempdir().expect("tempdir for baseline");
        let path = dir.path().join("base-zero.vhdx");
        VhdxFile::create(&path)
            .size(4 * 1024 * 1024)
            .block_size(1 * 1024 * 1024)
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create baseline fixed vhdx");

        let baseline_file = VhdxFile::open(&path)
            .finish()
            .expect("open baseline");
        let baseline_ctx = TestContext { _dir: dir, file: baseline_file, overlay: None };
        let baseline_io = baseline_ctx.io();
        let payload_offset = sector_zero_file_offset(&baseline_io);

        // Construct overlay with a zero region covering sector 0.
        let overlay = ReplayOverlay::from_raw(
            HashMap::new(),
            vec![(payload_offset, 4096)],
        );

        let ctx = create_fixed_io_with_overlay(overlay);
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");
        let mut buf = vec![0xFFu8; 4096];
        sector
            .read_exact(&mut buf)
            .expect("read sector 0 with zero overlay");
        // Overlay has a zero region at this offset → should be zeros
        // even though the underlying file has actual data.
        assert!(
            buf.iter().all(|&b| b == 0),
            "expected all zeros from zero-region overlay"
        );
    }

    // -- Multi-sector tests -------------------------------------------------

    /// Helper: create a small fixed VHDX opened in **write** mode.
    ///
    /// 4 MB virtual, 1 MB block, 4096 logical sector size.
    /// Blocks are FullyPresent and zero-initialized (fixed disk).
    fn create_fixed_test_io_writable() -> TestContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-fixed-rw.vhdx");

        VhdxFile::create(&path)
            .size(4 * 1024 * 1024)
            .block_size(1 * 1024 * 1024)
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create fixed test vhdx");

        let file = VhdxFile::open(&path)
            .write()
            .finish()
            .expect("open writable");

        TestContext { _dir: dir, file, overlay: None }
    }

    #[test]
    fn multi_sector_read_within_single_block() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // 1 MB / 4096 = 256 sectors per block; 3 sectors fit in block 0.
        let mut sector = io.sector(0, 3).expect("sector(0,3)");
        let mut buf = vec![0u8; 3 * 4096];
        sector
            .read_exact(&mut buf)
            .expect("read 3 sectors");
        assert_eq!(buf.len(), 3 * 4096);
        // Fixed disk is zero-initialized.
        assert!(buf.iter().all(|&b| b == 0), "expected all zeros");
    }

    #[test]
    fn multi_sector_read_count_one_regression() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // Write a known pattern to sector 0.
        let mut sw = io.sector(0, 1)
            .expect("sector 0");
        sw.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw.write_all(&[0x42u8; 4096])
            .expect("write 0x42");

        let mut buf = vec![0u8; 4096];
        let mut sr = io.sector(0, 1)
            .expect("sector 0");
        sr.read_exact(&mut buf)
            .expect("read back");
        assert!(
            buf.iter().all(|&b| b == 0x42),
            "expected all 0x42, got {:?}",
            &buf[..32]
        );
    }

    #[test]
    fn multi_sector_write_count_one_regression() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sw = io.sector(0, 1)
            .expect("sector 0");
        sw.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw.write_all(&[0xAAu8; 4096])
            .expect("write 0xAA");

        let mut buf = vec![0u8; 4096];
        let mut sr = io.sector(0, 1)
            .expect("sector 0");
        sr.read_exact(&mut buf)
            .expect("read back");
        assert!(
            buf.iter().all(|&b| b == 0xAA),
            "expected all 0xAA, got {:?}",
            &buf[..32]
        );
    }

    #[test]
    fn multi_sector_read_buffer_size_mismatch() {
        let ctx = create_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 3).expect("sector(0,3)");
        let mut buf = vec![0u8; 4096]; // smaller than 3*4096
        let n = sector.read(&mut buf).expect("read from 3-sector range");
        assert_eq!(n, 4096, "reads partial from 3-sector range");
    }

    #[test]
    fn multi_sector_write_data_size_mismatch() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sector = io.sector(0, 2).expect("sector(0,2)");
        let data = vec![0u8; 4096]; // smaller than 2*4096
        let n = sector.write(&data).expect("write to 2-sector range");
        assert_eq!(n, 4096, "writes partial to 2-sector range");
    }

    #[test]
    fn multi_sector_count_zero_is_error() {
        let ctx = create_test_io();
        let io = ctx.io();
        let result = io.sector(0, 0);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), Error::InvalidParameter(..)),
            "expected InvalidParameter for count=0"
        );
    }

    #[test]
    fn multi_sector_start_plus_count_overflow() {
        let ctx = create_test_io();
        let io = ctx.io();
        let result = io.sector(u64::MAX, 2);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), Error::InvalidParameter(..)),
            "expected InvalidParameter for overflow"
        );
    }

    #[test]
    fn multi_sector_out_of_bounds_range() {
        let ctx = create_test_io();
        let io = ctx.io();
        // dynamic VHDX: 256 MB / 4096 = 65536 sectors, max_sector = 65535
        // requesting 70000 sectors from start=0 exceeds range
        let result = io.sector(0, 70000);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), Error::SectorOutOfBounds { .. }),
            "expected SectorOutOfBounds"
        );
    }

    #[test]
    fn multi_sector_read_spanning_block_boundary() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // 1 MB / 4096 = 256 sectors per block
        // Read sectors 254-257: spans block 0 (sectors 0-255) → block 1 (sectors 256-511)
        let mut sector = io.sector(254, 4).expect("sector(254,4)");
        let mut buf = vec![0xFFu8; 4 * 4096];
        sector
            .read_exact(&mut buf)
            .expect("read spanning boundary");
        // Fixed disk is zero-initialized
        assert!(
            buf.iter().all(|&b| b == 0),
            "expected all zeros across block boundary"
        );
    }

    #[test]
    fn multi_sector_write_spanning_block_boundary() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // 1 MB / 4096 = 256 sectors per block
        // Write to sectors 254-257: spans block 0 → block 1
        let data = vec![0x42u8; 4 * 4096];
        let mut sw = io.sector(254, 4)
            .expect("sector(254,4)");
        sw.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw.write_all(&data)
            .expect("write spanning boundary");

        // Read back and verify
        let mut buf = vec![0u8; 4 * 4096];
        let mut sr = io.sector(254, 4)
            .expect("sector(254,4)");
        sr.read_exact(&mut buf)
            .expect("read back spanning boundary");
        assert!(
            buf.iter().all(|&b| b == 0x42),
            "expected all 0x42 across block boundary, got {:?}",
            &buf[..32]
        );
    }

    // -- T14: Sector bitmap per-bit lookup correctness test ----------------

    /// Verify the sector bitmap lookup math used by `is_sector_in_child()`.
    ///
    /// This test validates the computation of byte_idx, bit_idx from
    /// (block_idx, sector_in_block) parameters using a known bitmap pattern,
    /// without needing an actual VHDX file.
    #[test]
    fn sector_bitmap_bit_lookup_correctness() {
        let block_size: u64 = 32 * 1024 * 1024;
        let logical_sector_size: u64 = 4096;
        let sectors_per_block = block_size / logical_sector_size; // 8192
        let chunk_ratio: u64 = (1u64 << 23) * logical_sector_size / block_size; // 1024
        let stride = chunk_ratio + 1; // 1025

        // Build a synthetic 1 MB bitmap with known patterns
        let mut bitmap = vec![0u8; 1024 * 1024];

        // Set specific bits to validate lookup:
        {
            let bits = bitmap.view_bits_mut::<Lsb0>();
            bits.set(0, true);      // Sector 0 (byte 0, bit 0)
            bits.set(7, true);      // Sector 7 (byte 0, bit 7)
            bits.set(8, true);      // Sector 8 (byte 1, bit 0)
            bits.set(1000, true);   // Sector 1000 (byte 125, bit 0)
        }

        // Test lookup for various (block_idx, sector_in_block) combinations.
        // sector_in_chunk = block_in_chunk * sectors_per_block + sector_in_block

        // Case 1: block_idx=0 (chunk 0), sector_in_block=0
        {
            let block_in_chunk = 0u64 % chunk_ratio; // 0
            let sector_in_chunk = block_in_chunk * sectors_per_block + 0u64; // 0
            let byte_idx = (sector_in_chunk / 8) as usize;
            let bit_idx = (sector_in_chunk % 8) as u8;
            assert_eq!(byte_idx, 0);
            assert_eq!(bit_idx, 0);
            let bits = bitmap.view_bits::<Lsb0>();
            assert!(bits[sector_in_chunk as usize], "sector 0 should be present");
        }

        // Case 2: block_idx=0, sector_in_block=7
        {
            let block_in_chunk = 0u64;
            let sector_in_chunk = block_in_chunk * sectors_per_block + 7u64; // 7
            let byte_idx = (sector_in_chunk / 8) as usize;
            let bit_idx = (sector_in_chunk % 8) as u8;
            assert_eq!(byte_idx, 0);
            assert_eq!(bit_idx, 7);
            let bits = bitmap.view_bits::<Lsb0>();
            assert!(bits[sector_in_chunk as usize], "sector 7 should be present");
        }

        // Case 3: block_idx=0, sector_in_block=8
        {
            let block_in_chunk = 0u64;
            let sector_in_chunk = block_in_chunk * sectors_per_block + 8u64; // 8
            let byte_idx = (sector_in_chunk / 8) as usize;
            let bit_idx = (sector_in_chunk % 8) as u8;
            assert_eq!(byte_idx, 1);
            assert_eq!(bit_idx, 0);
            let bits = bitmap.view_bits::<Lsb0>();
            assert!(bits[sector_in_chunk as usize], "sector 8 should be present");
        }

        // Case 4: block_idx=0, sector_in_block=1000
        {
            let block_in_chunk = 0u64;
            let sector_in_chunk = block_in_chunk * sectors_per_block + 1000u64; // 1000
            let byte_idx = (sector_in_chunk / 8) as usize;
            let bit_idx = (sector_in_chunk % 8) as u8;
            assert_eq!(byte_idx, 125);
            assert_eq!(bit_idx, 0);
            let bits = bitmap.view_bits::<Lsb0>();
            assert!(bits[sector_in_chunk as usize], "sector 1000 should be present");
        }

        // Case 5: block_idx=1 (chunk 0), sector_in_block=0
        // sector_in_chunk = 1 * 8192 + 0 = 8192 → byte 1024, bit 0 → not set
        {
            let block_in_chunk = 1u64 % chunk_ratio; // 1
            let sector_in_chunk = block_in_chunk * sectors_per_block + 0u64; // 8192
            let byte_idx = (sector_in_chunk / 8) as usize;
            let bit_idx = (sector_in_chunk % 8) as u8;
            assert_eq!(byte_idx, 1024);
            assert_eq!(bit_idx, 0);
            let bits = bitmap.view_bits::<Lsb0>();
            assert!(!bits[sector_in_chunk as usize], "block 1 sector 0 should NOT be present");
        }

        // Verify chunk_idx and sb_bat_idx computation
        let block_idx: u64 = 5;
        let chunk_idx = block_idx / chunk_ratio; // 0
        let sb_bat_idx = chunk_idx * stride + chunk_ratio; // 1024
        assert_eq!(chunk_idx, 0);
        assert_eq!(sb_bat_idx, 1024);

        let block_idx_2: u64 = 1024;
        let chunk_idx_2 = block_idx_2 / chunk_ratio; // 1
        let sb_bat_idx_2 = chunk_idx_2 * stride + chunk_ratio; // 2049
        assert_eq!(chunk_idx_2, 1);
        assert_eq!(sb_bat_idx_2, 2049);
    }

    // -----------------------------------------------------------------------
    // Byte-level read/write tests (T4)
    // -----------------------------------------------------------------------

    /// Helper: create a small fixed VHDX opened in **read-only** mode.
    fn create_fixed_test_io() -> TestContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-fixed-ro.vhdx");

        VhdxFile::create(&path)
            .size(4 * 1024 * 1024)
            .block_size(1 * 1024 * 1024)
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create fixed test vhdx");

        let file = VhdxFile::open(&path)
            .finish()
            .expect("open read-only");

        TestContext { _dir: dir, file, overlay: None }
    }

    // T4.1: byte_offset_zero_read_matches_full_sector
    #[test]
    fn byte_offset_zero_read_matches_full_sector() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // Write pattern to sector 0
        let mut sw = io.sector(0, 1)
            .expect("sector 0");
        sw.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw.write_all(&[0x42u8; 4096])
            .expect("write sector 0");

        // Read full sector via byte_offset=0
        let mut full_buf = vec![0u8; 4096];
        let mut sr = io.sector(0, 1)
            .expect("sector 0");
        sr.seek(SeekFrom::Start(0)).expect("seek to 0");
        sr.read_exact(&mut full_buf)
            .expect("read sector 0");
        assert_eq!(full_buf, [0x42u8; 4096]);

        // Read small slice via byte_offset=0
        let mut small_buf = [0u8; 10];
        let mut sr2 = io.sector(0, 1)
            .expect("sector 0");
        sr2.seek(SeekFrom::Start(0)).expect("seek to 0");
        sr2.read_exact(&mut small_buf)
            .expect("read 10 bytes");
        assert_eq!(small_buf, [0x42u8; 10]);

        // Both reads return same first 10 bytes
        assert_eq!(&full_buf[..10], &small_buf[..10]);
    }

    // T4.2: byte_offset_non_aligned_read_extracts_correct_bytes
    #[test]
    fn byte_offset_non_aligned_read_extracts_correct_bytes() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sw0 = io.sector(0, 1)
            .expect("sector 0");
        sw0.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw0.write_all(&[0x11u8; 4096])
            .expect("write");

        // Read 100 bytes at offset 50
        let mut buf = [0u8; 100];
        let mut sr = io.sector(0, 1)
            .expect("sector 0");
        sr.seek(SeekFrom::Start(50)).expect("seek to 50");
        sr.read_exact(&mut buf)
            .expect("read at offset 50");
        assert_eq!(buf, [0x11u8; 100]);

        // Write 0x11 to sector 1 so the cross-sector read is consistent
        let mut sw1 = io.sector(1, 1)
            .expect("sector 1");
        sw1.seek(SeekFrom::Start(0)).expect("seek to 0");
        sw1.write_all(&[0x11u8; 4096])
            .expect("write sector 1");

        // Read 50 bytes at offset 4090 (crosses into sector 1)
        let mut sector = io.sector(0, 2).expect("sector(0,2)");
        let mut cross_buf = [0u8; 50];
        sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
        sector.read_exact(&mut cross_buf).expect("read at offset 4090");
        assert_eq!(cross_buf, [0x11u8; 50]);
    }

    // T4.3: byte_offset_rmw_write_preserves_surrounding_bytes
    #[test]
    fn byte_offset_rmw_write_preserves_surrounding_bytes() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");

        // Write full sector 0 with 0xAA
        sector.seek(SeekFrom::Start(0)).expect("seek to 0");
        sector.write_all(&[0xAAu8; 4096]).expect("write full sector");

        // Write 10 bytes at offset 100
        sector.seek(SeekFrom::Start(100)).expect("seek to 100");
        sector.write_all(&[0xBBu8; 10]).expect("write 10 bytes at offset 100");

        // Read back full sector
        let mut full_buf = vec![0u8; 4096];
        sector.seek(SeekFrom::Start(0)).expect("seek to 0");
        sector.read_exact(&mut full_buf).expect("read full sector");

        // Verify before patch preserved
        assert_eq!(&full_buf[0..100], &[0xAAu8; 100], "before patch preserved");
        // Verify patch applied
        assert_eq!(&full_buf[100..110], &[0xBBu8; 10], "patch applied");
        // Verify after patch preserved
        assert_eq!(&full_buf[110..4096], &[0xAAu8; 3986], "after patch preserved");
    }

    // T4.4: byte_offset_cross_block_boundary_read
    #[test]
    fn byte_offset_cross_block_boundary_read() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // 1 MB / 4096 = 256 sectors per block
        // sector(254, 4) spans block 0 (sectors 0-255) and block 1 (sectors 256-511)
        let mut sector = io.sector(254, 4).expect("sector(254,4)");

        // Write known pattern to all 4 sectors
        let mut pattern = Vec::with_capacity(4 * 4096);
        for i in 0..(4 * 4096) {
            pattern.push((i % 256) as u8);
        }
        sector.seek(SeekFrom::Start(0)).expect("seek to 0");
        sector.write_all(&pattern).expect("write pattern");

        // Read a byte range that crosses the block boundary
        // Block boundary at byte offset: 2 * 4096 = 8192 (sector 256 is first of block 1)
        // Cross boundary: read 200 bytes starting at byte_offset = 8192 - 100 = 8092
        let mut buf = [0u8; 200];
        sector.seek(SeekFrom::Start(8092)).expect("seek to 8092");
        sector
            .read_exact(&mut buf)
            .expect("read crossing block boundary");
        assert_eq!(buf, &pattern[8092..8292], "cross-boundary read mismatch");
    }

    // T4.5: byte_offset_cross_block_boundary_write_rmw
    #[test]
    fn byte_offset_cross_block_boundary_write_rmw() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        // 1 MB / 4096 = 256 sectors per block
        let mut sector = io.sector(254, 4).expect("sector(254,4)");

        // Write initial data: all 0xDD
        let init = [0xDDu8; 4 * 4096];
        sector.seek(SeekFrom::Start(0)).expect("seek to 0");
        sector.write_all(&init).expect("write initial");

        // Write a small patch that crosses the block boundary
        // Block boundary at byte 8192; write starting at byte 8180, 30 bytes
        let patch = [0xEEu8; 30];
        sector.seek(SeekFrom::Start(8180)).expect("seek to 8180");
        sector
            .write_all(&patch)
            .expect("write cross-boundary patch");

        // Read back and verify
        let mut buf = vec![0u8; 4 * 4096];
        sector.seek(SeekFrom::Start(0)).expect("seek to 0");
        sector.read_exact(&mut buf).expect("read back");

        assert_eq!(&buf[0..8180], &[0xDDu8; 8180], "before patch preserved");
        assert_eq!(&buf[8180..8210], &[0xEEu8; 30], "patch applied");
        assert_eq!(
            &buf[8210..],
            &[0xDDu8; 4 * 4096 - 8210],
            "after patch preserved"
        );
    }

    // T4.6: byte_offset_validation_exceeds_range
    #[test]
    fn byte_offset_validation_exceeds_range() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0"); // 1 sector = 4096 bytes

        // Read at EOF
        sector.seek(SeekFrom::Start(4096)).expect("seek to EOF");
        let mut tiny = [0u8; 1];
        let n = sector.read(&mut tiny).expect("read at EOF");
        assert_eq!(n, 0, "read at EOF should return 0");

        // Partial read at end
        sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
        let mut buf = [0u8; 10];
        let n = sector.read(&mut buf).expect("read at 4090");
        assert_eq!(n, 6, "should read partial at end");

        // Same for write
        // Write at EOF
        sector.seek(SeekFrom::Start(4096)).expect("seek to EOF");
        let n = sector.write(b"x").expect("write at EOF");
        assert_eq!(n, 0, "write at EOF should return 0");

        // Write partial at end
        sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
        let n = sector.write(&[0xFFu8; 10]).expect("write partial at end");
        assert_eq!(n, 6, "should write partial at end");
    }

    // T4.7: byte_offset_empty_buf_is_noop
    #[test]
    fn byte_offset_empty_buf_is_noop() {
        let ctx = create_fixed_test_io_writable();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");

        // Empty read
        assert_eq!(sector.read(&mut []).expect("empty read"), 0, "empty read returns 0");
        // Empty write
        assert_eq!(sector.write(&[]).expect("empty write"), 0, "empty write returns 0");
    }

    // T4.8: byte_offset_write_to_read_only_returns_error
    #[test]
    fn byte_offset_write_to_read_only_returns_error() {
        let ctx = create_fixed_test_io();
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");

        let err = sector.write(&[0x42u8; 10]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied, "should be PermissionDenied error");
    }

    // T4.9: byte_offset_write_to_not_present_block_returns_error
    #[test]
    fn byte_offset_write_to_not_present_block_returns_error() {
        // create_test_io() is read-only; create a writable dynamic VHDX so the
        // write reaches the block-state check (blocks are NotPresent by default).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-dynamic-rw.vhdx");

        VhdxFile::create(&path)
            .size(256 * 1024 * 1024)
            .block_size(32 * 1024 * 1024)
            .logical_sector_size(4096)
            .finish()
            .expect("create dynamic test vhdx");

        let file = VhdxFile::open(&path)
            .write()
            .finish()
            .expect("open writable dynamic");
        let ctx = TestContext { _dir: dir, file, overlay: None };
        let io = ctx.io();
        let mut sector = io.sector(0, 1).expect("sector 0");

        let err = sector.write(&[0x42u8; 10]).unwrap_err();
        assert_eq!(
            err.kind(),
            ErrorKind::NotFound,
            "write to NotPresent block should be NotFound"
        );
    }
}
