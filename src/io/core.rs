//! IO module: sector-level read/write operations for the virtual disk.
//!
//! This is the **sole data-plane entry point**. All virtual disk payload reads
//! and writes must go through [`IO::sector`] → [`Sector`] implementing
//! [`std::io::Read`], [`std::io::Write`], and [`std::io::Seek`].
//! Direct reads via [`Medium::get_ref`](crate::medium::Medium::get_ref) are forbidden
//! for payload data-plane access.
//!
//! # Differencing disk support
//!
//! For differencing (child) disks:
//! - Sector bitmap blocks are checked for [`PayloadBlockState::PartiallyPresent`].
//! - Sectors not present in the child fall back to the parent disk.
//! - The parent medium is resolved lazily and cached.
//!
//! # Standard
//!
//! MS-VHDX §2.5.1 (BAT state semantics for payload blocks)

use bitvec::prelude::*;
use std::sync::Arc;

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::bat::{Bat, BatState, PayloadBlockState, SectorBitmapState};
use crate::constants::MIB;
use crate::error::{Error, Result};
use crate::log_replay::ReplayOverlay;
use crate::medium::ReadSemanticsPolicy;
use crate::medium::{
    Len, Medium, ParentMedium, ParentRequest, ParentResolver, SetLen, SyncData, read_exact_at,
};
use crate::metadata::Metadata;

const PARENT_READ_AHEAD_BYTES: usize = 4 * MIB as usize;

fn parent_medium_range_buffer_len(sector_count: u64, logical_sector_size: u32) -> Result<usize> {
    usize::try_from(sector_count)
        .ok()
        .and_then(|count| count.checked_mul(logical_sector_size as usize))
        .ok_or_else(|| {
            Error::InvalidParameter("sector_count * logical sector size overflow".into())
        })
}

fn parent_medium_read_ahead_sector_count(
    start_sector: u64, sector_count: u64, logical_sector_size: u32, max_sector: u64,
) -> Result<u64> {
    let sectors_per_window = (PARENT_READ_AHEAD_BYTES / logical_sector_size as usize).max(1) as u64;
    let requested = sector_count.max(sectors_per_window);
    let remaining = max_sector
        .checked_add(1)
        .and_then(|sector_count| sector_count.checked_sub(start_sector))
        .ok_or_else(|| Error::InvalidParameter("parent read-ahead sector range overflow".into()))?;
    Ok(requested.min(remaining))
}

// ---------------------------------------------------------------------------
// IO
// ---------------------------------------------------------------------------

/// Virtual disk sector-level I/O.
///
/// Constructed internally from a mutable medium reference.
/// The IO struct resolves BAT entries, manages block offsets, and provides
/// the only path to sector-level reads and writes.
///
/// # Standard
///
/// MS-VHDX §2.5.1 — BAT entry state semantics for sector reads.
pub struct IO<'a, T = std::fs::File> {
    pub(super) file: &'a mut Medium<T>,
    pub(super) block_size: u32,
    pub(super) logical_sector_size: u32,
    pub(super) chunk_ratio: u64,
    max_sector: u64,
    pub(super) has_parent: bool,
    /// In-memory replay overlay for serving post-replay data through the read path.
    pub(super) overlay: Option<Arc<ReplayOverlay>>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedBatEntry {
    pub(super) state: BatState,
    file_offset_mb: u64,
}

impl ResolvedBatEntry {
    pub(super) fn file_offset_mb(&self) -> u64 {
        self.file_offset_mb
    }
}

impl<'a, T> IO<'a, T>
where
    T: Read + Seek,
{
    /// Create a new IO context from a medium reference.
    ///
    /// Loads metadata from the file to extract block size, sector sizes,
    /// parent status, and chunk ratio.
    pub(crate) fn new(file: &'a mut Medium<T>) -> Result<Self> {
        let overlay = file.replay_overlay_arc().cloned();
        let meta_buf = file.metadata_buf()?.to_vec();
        let metadata = Metadata::new(&meta_buf)?;
        let items = metadata.items();

        let fp = items
            .file_parameters()
            .map_err(|_| Error::InvalidMetadata("FileParameters metadata item not found".into()))?;
        let block_size = fp.block_size();
        let has_parent = fp.has_parent();
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
            has_parent,
            overlay,
        })
    }

    /// Locate and return a [`Sector`] spanning `count` sectors starting at
    /// global sector number `start`.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidParameter`] if `count == 0`, `start + count` overflows,
    ///   or `count * logical_sector_size` overflows.
    /// - [`Error::SectorOutOfBounds`] if the range exceeds the virtual disk.
    pub fn sector<'io>(&'io mut self, start: u64, count: u64) -> Result<Sector<'io, 'a, T>> {
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

        let logical_sector_size = self.logical_sector_size;
        let block_size = self.block_size;
        let chunk_ratio = self.chunk_ratio;
        let range_bytes = count
            .checked_mul(u64::from(logical_sector_size))
            .ok_or_else(|| Error::InvalidParameter("sector_count * lss overflow".into()))?;

        Ok(Sector {
            io: self,
            start,
            count,
            logical_sector_size,
            block_size,
            chunk_ratio,
            pos: 0,
            range_bytes,
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
pub struct Sector<'io, 'medium, T = std::fs::File> {
    pub(super) io: &'io mut IO<'medium, T>,
    pub(super) start: u64,
    pub(super) count: u64,
    pub(super) logical_sector_size: u32,
    pub(super) block_size: u32,
    pub(super) chunk_ratio: u64,
    pub(super) pos: u64,
    pub(super) range_bytes: u64,
    pub(super) semantics: ReadSemanticsPolicy,
}

impl<'medium, T> Sector<'_, 'medium, T>
where
    T: Read + Seek,
{
    pub(super) fn io(&self) -> &IO<'medium, T> {
        self.io
    }

    pub(super) fn io_mut(&mut self) -> &mut IO<'medium, T> {
        self.io
    }

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
    fn read_at(&mut self, buf: &mut [u8], byte_offset: u64) -> Result<()> {
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
    pub(super) fn read_full_sectors(
        &mut self, buf: &mut [u8], start_sector: u64, sector_count: u64,
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
            let state = entry.state;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent => {
                        self.read_block_range_from_file(
                            entry.file_offset_mb(),
                            sector_in_block,
                            sectors_this_round,
                            &mut buf[buf_offset..buf_offset + bytes_this_round],
                        )?;
                    }
                    PayloadBlockState::PartiallyPresent => {
                        self.read_partially_present_range(
                            entry,
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
                                    entry.file_offset_mb(),
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
                    PayloadBlockState::NotPresent if self.io().has_parent => {
                        self.read_parent_range(
                            block_idx,
                            sector_in_block,
                            sectors_this_round,
                            &mut buf[buf_offset..buf_offset + bytes_this_round],
                        )?;
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

    // -- Internal helpers ---------------------------------------------------

    /// Number of logical sectors per payload block.
    pub(super) fn sectors_per_block(&self) -> u64 {
        u64::from(self.block_size) / u64::from(self.logical_sector_size)
    }

    pub(super) fn sector_bitmap_bat_index(&self, block_idx: u64) -> u64 {
        let stride = self.chunk_ratio + 1;
        let chunk_idx = block_idx / self.chunk_ratio;
        chunk_idx * stride + self.chunk_ratio
    }

    fn read_parent_range(
        &mut self, block_idx: u64, start_sector_in_block: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        let spb = self.sectors_per_block();
        self.read_from_parent_range(block_idx * spb + start_sector_in_block, sector_count, buf)
    }

    /// Resolve the BAT entry for this sector's block.
    #[cfg(test)]
    pub(super) fn resolve_bat_entry(&mut self) -> Result<ResolvedBatEntry> {
        let block_idx = self.start / self.sectors_per_block();
        self.resolve_bat_entry_for_block(block_idx)
    }

    /// Resolve the BAT entry for a specific block index.
    pub(super) fn resolve_bat_entry_for_block(
        &mut self, block_idx: u64,
    ) -> Result<ResolvedBatEntry> {
        let bat_buf = self.io_mut().file.bat_buf()?;
        let bat = Bat::new(&bat_buf, self.chunk_ratio);
        let bat_array_idx = block_idx + block_idx / self.chunk_ratio;
        let entry = bat.entry(bat_array_idx)?;
        Ok(ResolvedBatEntry {
            state: entry.state()?,
            file_offset_mb: entry.file_offset_mb(),
        })
    }

    /// Read a contiguous range of sectors from a payload block in the file.
    ///
    /// `sector_in_block` is the offset of the first sector within the block,
    /// `buf` determines the amount of data to read (its length sets the read size),
    /// and `_sector_count` is currently unused.
    fn read_block_range_from_file(
        &mut self, file_offset_mb: u64, sector_in_block: u64, _sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let file_offset = file_offset_mb * u64::from(MIB) + sector_in_block * lss as u64;

        // Consult replay overlay first (per-block-span)
        if let Some(ref overlay) = self.io().overlay {
            let n = overlay.read(file_offset, buf);
            if n > 0 {
                return Ok(());
            }

            // T20: check physical file size gap
            let last_file_offset = overlay.last_file_offset();
            if last_file_offset > 0 && file_offset < last_file_offset {
                buf.fill(0);
                return Ok(());
            }
        }

        read_exact_at(self.io_mut().file.inner_mut(), file_offset, buf)?;
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
        &mut self, entry: ResolvedBatEntry, block_idx: u64, start_sector_in_block: u64,
        sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;

        // Load sector bitmap for this block
        let stride = self.chunk_ratio + 1;
        let chunk_idx = block_idx / self.chunk_ratio;
        let sb_bat_idx = chunk_idx * stride + self.chunk_ratio;

        let bat_buf = self.io_mut().file.bat_buf()?;
        let bat = Bat::new(&bat_buf, self.chunk_ratio);
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
        read_exact_at(self.io_mut().file.inner_mut(), sb_file_offset, &mut bitmap)?;

        let spb = self.sectors_per_block();
        let block_in_chunk = block_idx % self.chunk_ratio;
        let bits = bitmap.view_bits::<Lsb0>();

        let mut i = 0;
        while i < sector_count {
            let sib = start_sector_in_block + i;
            let sector_in_chunk = block_in_chunk * spb + sib;
            let byte_idx =
                usize::try_from(sector_in_chunk / 8).expect("bitmap byte index fits usize");

            if byte_idx >= bitmap.len() {
                return Err(Error::InvalidMetadata(format!(
                    "sector bitmap index out of range: byte {byte_idx}"
                )));
            }

            let in_child =
                bits[usize::try_from(sector_in_chunk).expect("bitmap bit index fits usize")];
            let offset = usize::try_from(i).expect("sector offset fits usize") * lss;

            if in_child {
                self.read_block_range_from_file(
                    entry.file_offset_mb(),
                    sib,
                    1,
                    &mut buf[offset..offset + lss],
                )?;
                i += 1;
            } else {
                let mut run_len = 1;
                while i + run_len < sector_count {
                    let run_sib = start_sector_in_block + i + run_len;
                    let run_sector_in_chunk = block_in_chunk * spb + run_sib;
                    let run_byte_idx = usize::try_from(run_sector_in_chunk / 8)
                        .expect("bitmap byte index fits usize");
                    if run_byte_idx >= bitmap.len() {
                        return Err(Error::InvalidMetadata(format!(
                            "sector bitmap index out of range: byte {run_byte_idx}"
                        )));
                    }

                    let run_in_child = bits[usize::try_from(run_sector_in_chunk)
                        .expect("bitmap bit index fits usize")];
                    if run_in_child {
                        break;
                    }
                    run_len += 1;
                }

                let run_bytes =
                    usize::try_from(run_len).expect("sector run length fits usize") * lss;
                self.read_from_parent_range(
                    block_idx * spb + start_sector_in_block + i,
                    run_len,
                    &mut buf[offset..offset + run_bytes],
                )?;
                i += run_len;
            }
        }

        Ok(())
    }

    /// Read a range of sectors from the parent disk at the given global sector number.
    ///
    /// Resolves and caches the parent medium on first access.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn read_from_parent_range(
        &mut self, global_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        self.validate_parent_range_buffer(sector_count, buf)?;

        {
            let cache = self
                .io()
                .file
                .parent_read_cache
                .lock()
                .map_err(|_| Error::InvalidFile("parent read cache lock poisoned".into()))?;
            if cache.copy_if_contains(global_sector, sector_count, self.logical_sector_size, buf) {
                return Ok(());
            }
        }

        self.ensure_parent_resolved()?;
        let read_ahead_sectors =
            self.parent_read_ahead_sector_count(global_sector, sector_count)?;
        let read_ahead_len =
            Self::parent_range_buffer_len(read_ahead_sectors, self.logical_sector_size)?;
        let mut read_ahead = vec![0; read_ahead_len];

        {
            let mut parent_ref = self
                .io()
                .file
                .resolved_parent
                .lock()
                .map_err(|_| Error::InvalidFile("parent medium lock poisoned".into()))?;
            let parent = parent_ref.as_mut().ok_or(Error::ParentResolverRequired)?;
            let parent_lss = parent.logical_sector_size()?;
            if parent_lss != self.logical_sector_size {
                return Err(Error::ParentSectorSizeMismatch {
                    child: self.logical_sector_size,
                    parent: parent_lss,
                });
            }
            parent.read_sector_range(global_sector, read_ahead_sectors, &mut read_ahead)?;
        }

        buf.copy_from_slice(&read_ahead[..buf.len()]);

        let mut cache = self
            .io()
            .file
            .parent_read_cache
            .lock()
            .map_err(|_| Error::InvalidFile("parent read cache lock poisoned".into()))?;
        cache.replace(
            global_sector,
            read_ahead_sectors,
            self.logical_sector_size,
            read_ahead,
        );
        Ok(())
    }

    fn validate_parent_range_buffer(&self, sector_count: u64, buf: &[u8]) -> Result<()> {
        let expected_len = Self::parent_range_buffer_len(sector_count, self.logical_sector_size)?;
        if buf.len() != expected_len {
            return Err(Error::InvalidParameter(format!(
                "parent sector range buffer length must equal sector_count * logical sector size: got {}, expected {expected_len}",
                buf.len()
            )));
        }
        Ok(())
    }

    fn parent_range_buffer_len(sector_count: u64, logical_sector_size: u32) -> Result<usize> {
        usize::try_from(sector_count)
            .ok()
            .and_then(|count| count.checked_mul(logical_sector_size as usize))
            .ok_or_else(|| {
                Error::InvalidParameter("sector_count * logical sector size overflow".into())
            })
    }

    fn parent_read_ahead_sector_count(&self, global_sector: u64, sector_count: u64) -> Result<u64> {
        let sectors_per_window =
            (PARENT_READ_AHEAD_BYTES / self.logical_sector_size as usize).max(1) as u64;
        let requested = sector_count.max(sectors_per_window);
        let remaining = self
            .io()
            .max_sector
            .checked_add(1)
            .and_then(|sector_count| sector_count.checked_sub(global_sector))
            .ok_or_else(|| {
                Error::InvalidParameter("parent read-ahead sector range overflow".into())
            })?;
        Ok(requested.min(remaining))
    }

    fn ensure_parent_resolved(&mut self) -> Result<()> {
        if self
            .io()
            .file
            .resolved_parent
            .lock()
            .map_err(|_| Error::InvalidFile("parent medium lock poisoned".into()))?
            .is_some()
        {
            return Ok(());
        }

        let meta_buf = self.io_mut().file.metadata_buf()?.to_vec();
        let meta = Metadata::new(&meta_buf)?;
        let items = meta.items();
        let locator = items.parent_locator().map_err(|_| Error::ParentNotFound)?;
        let expected_data_write_guid = locator
            .entries()
            .find_map(|entry| {
                let kv_data = locator.key_value_data();
                let key = entry.key(kv_data).ok()?;
                if key == "parent_linkage" {
                    let value = entry.value(kv_data).ok()?;
                    crate::types::Guid::parse_braced(&value).ok()
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                Error::InvalidParentLocator("parent_linkage missing or invalid".into())
            })?;
        let child_virtual_disk_size = items.virtual_disk_size()?;
        let request = ParentRequest {
            locator,
            expected_data_write_guid,
            child_logical_sector_size: self.logical_sector_size,
            child_virtual_disk_size,
        };
        let mut parent = {
            let mut resolver_ref = self
                .io()
                .file
                .parent_resolver
                .lock()
                .map_err(|_| Error::InvalidFile("parent resolver lock poisoned".into()))?;
            let resolver = resolver_ref.as_mut().ok_or(Error::ParentResolverRequired)?;
            resolver.resolve_parent(request)?
        };
        if parent.data_write_guid()? != expected_data_write_guid {
            return Err(Error::ParentLocatorGuidMismatch {
                expected: expected_data_write_guid,
                actual: parent.data_write_guid()?,
            });
        }
        let parent_lss = parent.logical_sector_size()?;
        if parent_lss != self.logical_sector_size {
            return Err(Error::ParentSectorSizeMismatch {
                child: self.logical_sector_size,
                parent: parent_lss,
            });
        }
        let mut parent_ref = self
            .io()
            .file
            .resolved_parent
            .lock()
            .map_err(|_| Error::InvalidFile("parent medium lock poisoned".into()))?;
        if parent_ref.is_none() {
            *parent_ref = Some(parent);
        }
        Ok(())
    }
}

impl<T> Medium<T>
where
    T: Read + Seek + Send,
{
    fn read_effective_parent_medium_range_with_cache(
        &mut self, start_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        if sector_count == 0 {
            return Err(Error::InvalidParameter("sector_count must be >= 1".into()));
        }

        let logical_sector_size = self.logical_sector_size()?;
        let expected_len = parent_medium_range_buffer_len(sector_count, logical_sector_size)?;
        if buf.len() != expected_len {
            return Err(Error::InvalidParameter(format!(
                "parent sector range buffer length must equal sector_count * logical sector size: got {}, expected {expected_len}",
                buf.len()
            )));
        }

        {
            let cache = self.parent_effective_read_cache.lock().map_err(|_| {
                Error::InvalidFile("parent effective read cache lock poisoned".into())
            })?;
            if cache.copy_if_contains(start_sector, sector_count, logical_sector_size, buf) {
                return Ok(());
            }
        }

        let (read_ahead_sectors, mut read_ahead) = {
            let mut io = self.io()?;
            let read_ahead_sectors = parent_medium_read_ahead_sector_count(
                start_sector,
                sector_count,
                logical_sector_size,
                io.max_sector,
            )?;
            let read_ahead_len =
                parent_medium_range_buffer_len(read_ahead_sectors, logical_sector_size)?;
            let mut read_ahead = vec![0; read_ahead_len];
            let mut sector = io
                .sector(start_sector, read_ahead_sectors)?
                .semantics(ReadSemanticsPolicy::EffectiveDataPreferred);
            sector.read_exact(&mut read_ahead)?;
            (read_ahead_sectors, read_ahead)
        };

        buf.copy_from_slice(&read_ahead[..buf.len()]);

        let mut cache = self
            .parent_effective_read_cache
            .lock()
            .map_err(|_| Error::InvalidFile("parent effective read cache lock poisoned".into()))?;
        cache.replace(
            start_sector,
            read_ahead_sectors,
            logical_sector_size,
            std::mem::take(&mut read_ahead),
        );
        Ok(())
    }
}

impl<T> ParentMedium for Medium<T>
where
    T: Read + Seek + Send,
{
    fn data_write_guid(&mut self) -> Result<crate::types::Guid> {
        let header_buf = self.header_buf_arc()?;
        let header = crate::header::Header::new(&header_buf)?;
        Ok(header.header(0)?.data_write_guid())
    }

    fn logical_sector_size(&mut self) -> Result<u32> {
        let meta_buf = self.metadata_buf()?;
        let meta = Metadata::new(&meta_buf)?;
        meta.items().logical_sector_size()
    }

    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> Result<()> {
        self.read_effective_parent_medium_range_with_cache(sector, 1, buf)
    }

    fn read_sector_range(
        &mut self, start_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        self.read_effective_parent_medium_range_with_cache(start_sector, sector_count, buf)
    }
}

impl<F, T> ParentResolver for F
where
    F: Fn(ParentRequest<'_>) -> Result<Medium<T>> + Send + 'static,
    T: Read + Seek + Send + 'static,
{
    fn resolve_parent(&mut self, request: ParentRequest<'_>) -> Result<Box<dyn ParentMedium>> {
        Ok(Box::new(std::cell::RefCell::new(self(request)?)))
    }
}

impl<T> std::fmt::Debug for Sector<'_, '_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sector")
            .field("start", &self.start)
            .field("count", &self.count)
            .field("logical_sector_size", &self.logical_sector_size)
            .field("block_size", &self.block_size)
            .field("chunk_ratio", &self.chunk_ratio)
            .field("pos", &self.pos)
            .field("range_bytes", &self.range_bytes)
            .field("semantics", &self.semantics)
            .finish_non_exhaustive()
    }
}

// PartialEq cannot be derived because Medium does not implement PartialEq.
impl<T> PartialEq for Sector<'_, '_, T> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.io, other.io) && self.start == other.start && self.count == other.count
    }
}

// ---------------------------------------------------------------------------
// std::io trait implementations — cursor-based I/O
// ---------------------------------------------------------------------------

impl<T> io::Read for Sector<'_, '_, T>
where
    T: Read + Seek,
{
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

impl<T> io::Write for Sector<'_, '_, T>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
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

impl<T> io::Seek for Sector<'_, '_, T> {
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
