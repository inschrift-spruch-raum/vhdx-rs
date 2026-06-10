//! VHDX Log Replay Engine
//!
//! Implements MS-VHDX §2.3.3 active sequence detection and log replay.
//! The log is a circular buffer of variable-sized 4KB-aligned entries.
//! On open, the implementation must find the newest valid complete log
//! sequence (the "active sequence") and replay it before any payload I/O.

use std::collections::HashMap;
use std::io::{Seek, Write};

use crate::constants::SECTOR_SIZE;
use crate::error::{Error, Result};
use crate::log::{Descriptor, Entry, Log};
use crate::medium::write_all_at;
use crate::types::Guid;

// ---------------------------------------------------------------------------
// ActiveSequence
// ---------------------------------------------------------------------------

/// The detected active log sequence, containing entries in tail-to-head
/// (replay) order.
///
/// Returned by [`detect_active_sequence`] after scanning the log buffer.
#[derive(Debug)]
pub struct ActiveSequence<'a> {
    /// Entries ordered from tail (oldest) to head (newest).
    entries: Vec<LocatedEntry<'a>>,
    /// `FlushedFileOffset` from the head entry — used to detect truncation.
    flushed_file_offset: u64,
    /// `LastFileOffset` from the head entry — file must be at least this
    /// large after replay.
    last_file_offset: u64,
}

/// A single log entry together with its byte offset within the log buffer.
#[derive(Debug)]
pub struct LocatedEntry<'a> {
    pub(super) entry: Entry<'a>,
    _offset: usize,
}

impl<'a> ActiveSequence<'a> {
    /// Entries in replay order (tail → head).
    pub fn entries(&self) -> &[LocatedEntry<'a>] {
        &self.entries
    }

    /// The head entry's `FlushedFileOffset`.
    ///
    /// If the actual file size is smaller than this value, the file has
    /// been truncated and must not be opened.
    pub fn flushed_file_offset(&self) -> u64 {
        self.flushed_file_offset
    }

    /// The head entry's `LastFileOffset`.
    ///
    /// After replay, the file must be extended to at least this size.
    pub fn last_file_offset(&self) -> u64 {
        self.last_file_offset
    }

    /// Number of entries in the active sequence.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// ReplayOverlay
// ---------------------------------------------------------------------------

/// In-memory replay overlay built from an active log sequence.
///
/// Stores assembled data sectors (mapped by file offset) and zero-out
/// regions. Used by [`LogReplayPolicy::InMemoryOnReadOnly`] and
/// [`LogReplayPolicy::Auto`] on read-only opens to provide a consistent
/// post-replay view without modifying the underlying file.
#[derive(Debug)]
pub struct ReplayOverlay {
    /// Data sectors keyed by file offset (4KB-aligned).
    sectors: HashMap<u64, Vec<u8>>,
    /// Zero regions as `(file_offset, zero_length)` pairs.
    zeros: Vec<(u64, u64)>,
    /// File size after replay (from the head entry's `LastFileOffset`).
    last_file_offset: u64,
}

impl ReplayOverlay {
    /// The file size after replay is complete.
    pub fn last_file_offset(&self) -> u64 {
        self.last_file_offset
    }

    /// Read data through the replay overlay.
    ///
    /// Checks the overlay first: if the requested range overlaps with
    /// replayed data sectors or zero regions, those take priority over
    /// the on-disk file content.
    ///
    /// - Data sectors take priority: if `offset` falls within an overlaid
    ///   4 KB sector, the corresponding bytes are copied into `buf`.
    /// - Zero regions: if `offset` falls within a zero descriptor range,
    ///   `buf` is filled with zeroes.
    /// - Otherwise returns `0` to signal that the caller should read
    ///   from the underlying file instead.
    ///
    /// Returns the number of bytes written into `buf`.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }

        // --- Check data sectors first (highest priority) ---
        // Each sector is keyed by its 4 KB-aligned file offset and is
        // exactly 4096 bytes long.
        for (&sector_offset, sector_data) in &self.sectors {
            let sector_end = sector_offset
                + u64::try_from(sector_data.len()).expect("sector data length fits u64");
            if offset >= sector_offset && offset < sector_end {
                let in_sector = usize::try_from(offset - sector_offset)
                    .expect("sector-relative offset fits usize");
                let available = sector_data.len() - in_sector;
                let to_copy = available.min(buf.len());
                buf[..to_copy].copy_from_slice(&sector_data[in_sector..in_sector + to_copy]);
                return to_copy;
            }
        }

        // --- Check zero regions ---
        for &(zero_offset, zero_length) in &self.zeros {
            let zero_end = zero_offset + zero_length;
            if offset >= zero_offset && offset < zero_end {
                let remaining = usize::try_from(zero_length - (offset - zero_offset))
                    .expect("remaining zero length fits usize");
                let to_fill = remaining.min(buf.len());
                buf[..to_fill].fill(0);
                return to_fill;
            }
        }

        // --- No overlay data at this offset ---
        0
    }

    /// Apply overlay data and zero regions to an in-memory region buffer.
    ///
    /// Patches `region_data` (representing bytes at file offset `region_offset`)
    /// with data from overlaid sectors and zero-filled regions.
    ///
    /// - Data sectors take priority: if a byte range is covered by a data
    ///   sector, the sector bytes are copied into `region_data`.
    /// - Zero regions fill untouched bytes with zeroes. Bytes already written
    ///   by a data sector are NOT overwritten by zero regions.
    ///
    /// This is a pure in-memory patch operation — it does not read or write
    /// the underlying file.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    pub fn apply_to_region(&self, region_data: &mut [u8], region_offset: u64) {
        let region_end =
            region_offset + u64::try_from(region_data.len()).expect("region length fits u64");

        // Track which bytes have been written by data sectors so that zeros
        // don't overwrite them.
        let mut touched = vec![false; region_data.len()];

        // Step 1: apply data sectors
        for (&sector_offset, sector_data) in &self.sectors {
            let sector_end = sector_offset
                + u64::try_from(sector_data.len()).expect("sector data length fits u64");
            if sector_end > region_offset && sector_offset < region_end {
                let overlap_start = sector_offset.max(region_offset);
                let overlap_end = sector_end.min(region_end);
                let region_start = usize::try_from(overlap_start - region_offset)
                    .expect("region overlap start fits usize");
                let sector_start = usize::try_from(overlap_start - sector_offset)
                    .expect("sector overlap start fits usize");
                let len = usize::try_from(overlap_end - overlap_start)
                    .expect("overlap length fits usize");
                region_data[region_start..region_start + len]
                    .copy_from_slice(&sector_data[sector_start..sector_start + len]);
                for touched_byte in touched.iter_mut().skip(region_start).take(len) {
                    *touched_byte = true;
                }
            }
        }

        // Step 2: apply zero regions (skip bytes already touched by data sectors)
        for &(zero_offset, zero_length) in &self.zeros {
            let zero_end = zero_offset + zero_length;
            if zero_end > region_offset && zero_offset < region_end {
                let overlap_start = zero_offset.max(region_offset);
                let overlap_end = zero_end.min(region_end);
                let region_start = usize::try_from(overlap_start - region_offset)
                    .expect("region overlap start fits usize");
                let len = usize::try_from(overlap_end - overlap_start)
                    .expect("overlap length fits usize");
                for i in region_start..region_start + len {
                    if !touched[i] {
                        region_data[i] = 0;
                    }
                }
            }
        }
    }

    /// Return a reference to the sector map for testing/inspection.
    #[cfg(test)]
    pub(super) fn sectors(&self) -> &HashMap<u64, Vec<u8>> {
        &self.sectors
    }

    /// Return a reference to the zero regions for testing/inspection.
    #[cfg(test)]
    pub(super) fn zeros(&self) -> &[(u64, u64)] {
        &self.zeros
    }

    /// Construct a `ReplayOverlay` from raw sector and zero-region data.
    ///
    /// Intended for unit tests in other modules that need to verify
    /// overlay read-through behaviour without constructing a full log sequence.
    #[cfg(test)]
    pub(crate) fn from_raw(sectors: HashMap<u64, Vec<u8>>, zeros: Vec<(u64, u64)>) -> Self {
        Self {
            sectors,
            zeros,
            last_file_offset: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect the active log sequence using the algorithm from MS-VHDX §2.3.3.
///
/// Scans the circular log buffer to find the newest valid and complete
/// sequence of log entries. Returns [`Error::LogEntryCorrupted`] if no
/// valid sequence is found.
///
/// # Algorithm (MS-VHDX §2.3.3)
///
/// 1. Initialize candidate as empty (seq=0), current/old tail = 0.
/// 2. Set current sequence empty with head = current tail, seq = 0.
/// 3. Evaluate entries at head. If valid and sequence numbers are
///    consecutive, extend current sequence and repeat.
/// 4. If the head entry's tail is within the current sequence, the
///    sequence is valid.
/// 5. If valid and seq > candidate's seq, update candidate.
/// 6. If empty/invalid, advance tail by 4KB (wrap). Otherwise set tail
///    to head (wrap).
/// 7. If tail < old tail (wrapped), done. Otherwise go to step 2.
/// 8. If candidate empty → corrupt. Return the candidate.
///
/// # Errors
///
/// Returns [`Error::LogEntryCorrupted`] if the log buffer is empty or no
/// valid active sequence is found. Also propagates errors from
/// [`Log::entry_at`].
///
/// # Panics
///
/// Panics if `current_entries` is unexpectedly empty when accessing the
/// last element. The algorithm guarantees non-emptiness at the three
/// `.unwrap()` call sites, so this should not occur with well-formed data.
pub fn detect_active_sequence<'a>(log: &'a Log<'a>, log_guid: &Guid) -> Result<ActiveSequence<'a>> {
    let log_size = log.len();
    if log_size == 0 {
        return Err(Error::LogEntryCorrupted(
            "log buffer is empty, no active sequence".into(),
        ));
    }

    // Step 1: initialize candidate
    let mut candidate_entries: Vec<(usize, u64)> = Vec::new(); // (offset, seq)
    let mut candidate_head_seq: u64 = 0;
    let mut current_tail: usize = 0;
    let mut old_tail: usize = 0;

    loop {
        // Step 2: current sequence
        let mut current_entries: Vec<(usize, u64)> = Vec::new();
        let mut head: usize = current_tail;
        let mut current_seq: u64 = 0;

        // Step 3: extend current sequence
        loop {
            let parsed = try_validate_entry(log, head, log_guid);
            match parsed {
                Some((entry, seq)) => {
                    // Check sequence continuity
                    if !current_entries.is_empty() && seq != current_seq + 1 {
                        break; // non-consecutive
                    }
                    let entry_len = entry.header().entry_length() as usize;
                    current_entries.push((head, seq));
                    current_seq = seq;
                    // Advance head past this entry, wrapping at log_size
                    head = (head + entry_len) % log_size;
                    // Safety: if head wraps to 0, stop extending
                    // (prevents infinite loops on fully-filled buffer)
                    if head == 0 && !current_entries.is_empty() {
                        break;
                    }
                }
                None => break, // invalid entry
            }
        }

        // Step 4: check if current sequence is valid
        let is_valid = if current_entries.is_empty() {
            false
        } else {
            // Get the head entry's tail field
            let head_entry_offset = current_entries.last().unwrap().0;
            let head_entry = log.entry_at(head_entry_offset)?;
            let tail = head_entry.header().tail() as usize;
            // Check if tail offset matches any entry in current sequence
            current_entries.iter().any(|(off, _)| *off == tail)
        };

        // Step 5: update candidate
        if is_valid && current_seq > candidate_head_seq {
            candidate_entries.clone_from(&current_entries);
            candidate_head_seq = current_seq;
        }

        // Step 6: advance current_tail
        if current_entries.is_empty() || !is_valid {
            // Empty or invalid → advance by 4KB
            current_tail = (current_tail + SECTOR_SIZE as usize) % log_size;
        } else {
            // Valid → advance past the head entry
            let last_entry_offset = current_entries.last().unwrap().0;
            let last_entry = log.entry_at(last_entry_offset)?;
            let last_len = last_entry.header().entry_length() as usize;
            let next_head = (last_entry_offset + last_len) % log_size;
            // Wrap: if >= log_size, modulo
            current_tail = if next_head >= log_size {
                next_head % log_size
            } else {
                next_head
            };
        }

        // Step 7: check wrap condition
        if current_tail <= old_tail {
            break;
        }
        old_tail = current_tail;
    }

    // Step 8: check candidate
    if candidate_entries.is_empty() {
        return Err(Error::LogEntryCorrupted(
            "no valid active log sequence found".into(),
        ));
    }

    // Build ActiveSequence from candidate entries
    let head_entry_offset = candidate_entries.last().unwrap().0;
    let head_entry = log.entry_at(head_entry_offset)?;
    let flushed_file_offset = head_entry.header().flushed_file_offset();
    let last_file_offset = head_entry.header().last_file_offset();

    let mut entries = Vec::with_capacity(candidate_entries.len());
    for (offset, _seq) in &candidate_entries {
        let entry = log.entry_at(*offset)?;
        entries.push(LocatedEntry {
            entry,
            _offset: *offset,
        });
    }

    Ok(ActiveSequence {
        entries,
        flushed_file_offset,
        last_file_offset,
    })
}

/// Check whether a replayable log exists in the buffer.
///
/// Returns `true` if the log buffer is non-empty and contains a valid
/// active sequence matching the given `log_guid`.
///
/// This is used to determine whether [`LogReplayPolicy::Require`] must
/// reject the open.
pub fn has_pending_log(log: &Log<'_>, log_guid: &Guid) -> bool {
    // Quick check: if log_guid is all zeros, no log operations were ever
    // performed on this file.
    if log_guid.to_bytes() == [0u8; 16] {
        return false;
    }
    // Try to detect an active sequence.
    detect_active_sequence(log, log_guid).is_ok()
}

/// Build an in-memory replay overlay from an active log sequence.
///
/// Iterates entries in tail→head order, assembling data sectors and
/// recording zero regions. The resulting [`ReplayOverlay`] provides
/// a consistent post-replay view without writing to the underlying file.
///
/// Used by [`LogReplayPolicy::InMemoryOnReadOnly`] and
/// [`LogReplayPolicy::Auto`] on read-only opens.
///
/// # Errors
///
/// Returns [`Error::LogEntryCorrupted`] if a data descriptor has no
/// matching data sector. Also propagates errors from [`Entry::descriptor`].
pub fn build_replay_overlay(active: &ActiveSequence<'_>) -> Result<ReplayOverlay> {
    let mut sectors: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut zeros: Vec<(u64, u64)> = Vec::new();

    for located in active.entries() {
        let entry = &located.entry;
        let desc_count = entry.header().descriptor_count();

        // Collect data sectors once per entry for indexed access
        let data_sectors: Vec<_> = entry.data().collect();
        let mut data_idx = 0usize;

        for di in 0..desc_count as usize {
            let desc = entry.descriptor(di)?;
            match desc {
                Descriptor::Data(data_desc) => {
                    if data_idx >= data_sectors.len() {
                        return Err(Error::LogEntryCorrupted(format!(
                            "data descriptor {di} has no matching data sector ({} descriptors, {} sectors)",
                            desc_count,
                            data_sectors.len()
                        )));
                    }
                    let sector = &data_sectors[data_idx];
                    let file_offset = data_desc.file_offset();
                    sectors.insert(file_offset, sector.data().to_vec());
                    data_idx += 1;
                }
                Descriptor::Zero(zero_desc) => {
                    let file_offset = zero_desc.file_offset();
                    let zero_length = zero_desc.zero_length();
                    zeros.push((file_offset, zero_length));
                }
            }
        }
    }

    Ok(ReplayOverlay {
        sectors,
        zeros,
        last_file_offset: active.last_file_offset(),
    })
}

/// Replay the active log sequence directly to the file.
///
/// Writes each data descriptor's assembled sector and zeros out regions
/// specified by zero descriptors. After replay, extends the file to at
/// least `LastFileOffset`.
///
/// Used by [`LogReplayPolicy::Auto`] on writable opens.
///
/// # Errors
///
/// Returns I/O errors from [`Seek`], [`Write`], and [`std::fs::File::set_len`].
/// Returns [`Error::LogEntryCorrupted`] if a data descriptor has no matching
/// data sector. Also propagates errors from [`Entry::descriptor`].
///
/// # Panics
///
/// Panics if arithmetic overflow occurs during sector/offset conversion.
/// This should not happen with well-formed VHDX files.
pub fn replay_to_file<T>(file: &mut T, active: &ActiveSequence<'_>) -> Result<()>
where
    T: Seek + Write + crate::medium::SetLen,
{
    // Replay each entry in tail-to-head order
    for located in active.entries() {
        let entry = &located.entry;
        let desc_count = entry.header().descriptor_count();

        // Collect data sectors once per entry for indexed access
        let data_sectors: Vec<_> = entry.data().collect();
        let mut data_idx = 0usize;

        for di in 0..desc_count as usize {
            let desc = entry.descriptor(di)?;
            match desc {
                Descriptor::Data(data_desc) => {
                    if data_idx >= data_sectors.len() {
                        return Err(Error::LogEntryCorrupted(format!(
                            "data descriptor {di} has no matching data sector"
                        )));
                    }
                    let sector = &data_sectors[data_idx];
                    let file_offset = data_desc.file_offset();

                    write_all_at(file, file_offset, &sector.data())?;
                    data_idx += 1;
                }
                Descriptor::Zero(zero_desc) => {
                    let file_offset = zero_desc.file_offset();
                    let zero_length = usize::try_from(zero_desc.zero_length())
                        .expect("zero descriptor length fits usize");
                    let zero_buf = vec![0u8; (SECTOR_SIZE as usize).min(zero_length)];
                    let mut written: usize = 0;
                    while written < zero_length {
                        let chunk = zero_buf.len().min(zero_length - written);
                        write_all_at(
                            file,
                            file_offset + u64::try_from(written).expect("written count fits u64"),
                            &zero_buf[..chunk],
                        )?;
                        written += chunk;
                    }
                }
            }
        }
    }

    // Extend file to at least LastFileOffset
    crate::medium::SetLen::set_len(file, active.last_file_offset())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Try to parse and validate a log entry at the given byte offset.
///
/// Returns `Some((entry, sequence_number))` if:
/// - The entry parses successfully (valid signature, length)
/// - The `LogGuid` matches
/// - The CRC-32C checksum is correct
/// - The sequence number is > 0
///
/// Returns `None` if the entry is invalid (silently ignored by the
/// algorithm — not a hard error for invalid entries during scanning).
fn try_validate_entry<'a>(
    log: &'a Log<'a>, offset: usize, log_guid: &Guid,
) -> Option<(Entry<'a>, u64)> {
    let entry = log.entry_at(offset).ok()?;

    // LogGuid must match
    if entry.header().log_guid() != *log_guid {
        return None;
    }

    // CRC-32C must be valid
    if entry.verify_checksum().is_err() {
        return None;
    }

    let seq = entry.header().sequence_number();
    if seq == 0 {
        return None;
    }

    Some((entry, seq))
}
