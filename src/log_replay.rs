//! VHDX Log Replay Engine
//!
//! Implements MS-VHDX §2.3.3 active sequence detection and log replay.
//! The log is a circular buffer of variable-sized 4KB-aligned entries.
//! On open, the implementation must find the newest valid complete log
//! sequence (the "active sequence") and replay it before any payload I/O.

use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};

use crate::constants::SECTOR_SIZE;
use crate::error::{Error, Result};
use crate::log::{Descriptor, Entry, Log};
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
    entry: Entry<'a>,
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
    pub fn read(&self, _file: &std::fs::File, offset: u64, buf: &mut [u8]) -> usize {
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
    fn sectors(&self) -> &HashMap<u64, Vec<u8>> {
        &self.sectors
    }

    /// Return a reference to the zero regions for testing/inspection.
    #[cfg(test)]
    fn zeros(&self) -> &[(u64, u64)] {
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
pub fn replay_to_file(file: &std::fs::File, active: &ActiveSequence<'_>) -> Result<()> {
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

                    // Seek and write the assembled sector
                    let f = file;
                    (&mut &*f).seek(SeekFrom::Start(file_offset))?;
                    (&mut &*f).write_all(&sector.data())?;
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
                        let f = file;
                        (&mut &*f).seek(SeekFrom::Start(
                            file_offset + u64::try_from(written).expect("written count fits u64"),
                        ))?;
                        (&mut &*f).write_all(&zero_buf[..chunk])?;
                        written += chunk;
                    }
                }
            }
        }
    }

    // Extend file to at least LastFileOffset
    file.set_len(active.last_file_offset())?;

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{SIGNATURE_DATA, SIGNATURE_DESC, SIGNATURE_LOGE, SIGNATURE_ZERO};
    use crc32c::crc32c;
    use std::io::Read;

    /// Build a complete log entry buffer (header + descriptors + data sectors).
    fn build_log_entry(
        seq: u64,
        tail_offset: u32,
        desc_specs: &[(bool, u64, u64)], // (is_data, file_offset, extra) — extra = 0 for data, zero_length for zero
        fill_byte: u8,
        log_guid: &Guid,
    ) -> Vec<u8> {
        let header_size = 64;
        let desc_count = u32::try_from(desc_specs.len()).expect("descriptor count fits u32");

        // Build descriptors
        let mut desc_bytes = Vec::new();
        for &(is_data, file_offset, extra) in desc_specs {
            let mut d = [0u8; 32];
            if is_data {
                d[0..4].copy_from_slice(&SIGNATURE_DESC.into_inner().to_le_bytes());
                d[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // trailing
                d[8..16].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes()); // leading
                d[16..24].copy_from_slice(&file_offset.to_le_bytes());
                d[24..32].copy_from_slice(&seq.to_le_bytes());
            } else {
                d[0..4].copy_from_slice(&SIGNATURE_ZERO.into_inner().to_le_bytes());
                d[4..8].copy_from_slice(&0u32.to_le_bytes()); // reserved
                d[8..16].copy_from_slice(&extra.to_le_bytes()); // zero_length
                d[16..24].copy_from_slice(&file_offset.to_le_bytes());
                d[24..32].copy_from_slice(&seq.to_le_bytes());
            }
            desc_bytes.extend_from_slice(&d);
        }

        // Build data sectors (one per data descriptor)
        let data_desc_count = desc_specs.iter().filter(|(is_data, _, _)| *is_data).count();
        let mut data_bytes = Vec::new();
        for _ in 0..data_desc_count {
            let mut s = [0u8; 4096];
            s[0..4].copy_from_slice(&SIGNATURE_DATA.into_inner().to_le_bytes());
            s[4..8].copy_from_slice(
                &u32::try_from(seq >> 32)
                    .expect("upper sequence bits fit u32")
                    .to_le_bytes(),
            );
            for b in &mut s[8..4092] {
                *b = fill_byte;
            }
            s[4092..4096].copy_from_slice(
                &u32::try_from(seq & u64::from(u32::MAX))
                    .expect("lower sequence bits fit u32")
                    .to_le_bytes(),
            );
            data_bytes.extend_from_slice(&s);
        }

        // Calculate descriptor sectors
        let desc_sectors = if desc_bytes.len() + header_size <= SECTOR_SIZE.into() {
            1
        } else {
            let overflow = desc_bytes.len() + header_size - SECTOR_SIZE as usize;
            1 + overflow.div_ceil(SECTOR_SIZE.into())
        };
        let desc_sector_bytes = desc_sectors * SECTOR_SIZE as usize;
        let total = desc_sector_bytes + data_bytes.len();
        let total_aligned = total.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;

        let mut buf = vec![0u8; total_aligned];

        // Header
        buf[0..4].copy_from_slice(&SIGNATURE_LOGE.into_inner().to_le_bytes());
        buf[8..12].copy_from_slice(
            &u32::try_from(total_aligned)
                .expect("total_aligned fits u32")
                .to_le_bytes(),
        );
        buf[12..16].copy_from_slice(&tail_offset.to_le_bytes());
        buf[16..24].copy_from_slice(&seq.to_le_bytes());
        buf[24..28].copy_from_slice(&desc_count.to_le_bytes());
        buf[32..48].copy_from_slice(&log_guid.to_bytes());
        buf[48..56].copy_from_slice(&0x1_0000_0000u64.to_le_bytes()); // FlushedFileOffset
        buf[56..64].copy_from_slice(&0x2_0000_0000u64.to_le_bytes()); // LastFileOffset

        // Descriptors
        buf[header_size..header_size + desc_bytes.len()].copy_from_slice(&desc_bytes);

        // Data sectors
        if !data_bytes.is_empty() {
            buf[desc_sector_bytes..desc_sector_bytes + data_bytes.len()]
                .copy_from_slice(&data_bytes);
        }

        // CRC-32C
        let checksum = crc32c(&buf);
        buf[4..8].copy_from_slice(&checksum.to_le_bytes());

        buf
    }

    fn test_log_guid() -> Guid {
        Guid::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ])
    }

    /// Build a log buffer containing multiple entries.
    fn build_log_buffer(entries: Vec<Vec<u8>>) -> Vec<u8> {
        let mut buf = Vec::new();
        for e in entries {
            buf.extend_from_slice(&e);
        }
        // Pad to 4KB alignment
        while buf.len() % SECTOR_SIZE as usize != 0 {
            buf.push(0);
        }
        buf
    }

    // -----------------------------------------------------------------------
    // Active sequence detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn empty_log_no_sequence() {
        let buf = vec![0u8; SECTOR_SIZE as usize * 4]; // empty (no valid entries)
        let log = Log::new(&buf).unwrap();
        let guid = test_log_guid();
        assert!(detect_active_sequence(&log, &guid).is_err());
    }

    #[test]
    fn single_entry_self_tail() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);

        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active.flushed_file_offset(), 0x1_0000_0000);
        assert_eq!(active.last_file_offset(), 0x2_0000_0000);
    }

    #[test]
    fn three_entry_sequence() {
        let guid = test_log_guid();
        // Three entries forming a sequence: seq 1, 2, 3
        // Entry 1: tail=0 (self), Entry 2: tail=0, Entry 3: tail=0
        let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
        let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);
        let buf = build_log_buffer(vec![e1, e2, e3]);

        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        assert_eq!(active.len(), 3);

        // Verify entry sequence numbers
        let seqs: Vec<u64> = active
            .entries()
            .iter()
            .map(|e| e.entry.header().sequence_number())
            .collect();
        assert_eq!(seqs, vec![1, 2, 3]);
    }

    #[test]
    fn multiple_sequences_picks_newest() {
        let guid = test_log_guid();
        // Old sequence: 1, 2, 3
        let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
        let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

        // New sequence: 10, 11, 12 (head entry 12 has tail pointing to entry 10's offset)
        let e10 = build_log_entry(10, 0, &[(true, 0x4000, 0)], 0xDD, &guid);
        let e11 = build_log_entry(11, 0, &[(true, 0x5000, 0)], 0xEE, &guid);
        let e12 = build_log_entry(12, 0, &[(true, 0x6000, 0)], 0xFF, &guid);

        let mut entries = vec![e1, e2, e3, e10, e11, e12];
        // Fix tail of e12 to point to e10's offset
        // We need to know e10's offset. Let's calculate it.
        let e1_len = entries[0].len();
        let e2_len = entries[1].len();
        let e3_len = entries[2].len();
        let e10_offset = e1_len + e2_len + e3_len;
        // Rewrite e12's tail to point to e10
        let e12_idx = 5;
        entries[e12_idx][12..16].copy_from_slice(
            &u32::try_from(e10_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        // Recompute CRC for e12 (zero CRC field before recomputing)
        entries[e12_idx][4..8].copy_from_slice(&0u32.to_le_bytes());
        let e12_crc = crc32c(&entries[e12_idx]);
        entries[e12_idx][4..8].copy_from_slice(&e12_crc.to_le_bytes());

        let buf = build_log_buffer(entries);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        let seqs: Vec<u64> = active
            .entries()
            .iter()
            .map(|e| e.entry.header().sequence_number())
            .collect();
        assert_eq!(seqs, vec![10, 11, 12], "should pick newest sequence");
    }

    #[test]
    fn guid_mismatch_entry_ignored() {
        let guid = test_log_guid();
        let other_guid = Guid::from_bytes([0xFFu8; 16]);

        let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &other_guid); // wrong GUID
        let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

        let buf = build_log_buffer(vec![e1, e2, e3]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        // Only entry 1 should be in the sequence (e2 has wrong GUID, breaks chain)
        assert_eq!(active.len(), 1);
        assert_eq!(active.entries()[0].entry.header().sequence_number(), 1);
    }

    #[test]
    fn checksum_failure_entry_ignored() {
        let guid = test_log_guid();
        let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let mut e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);

        // Corrupt e2's CRC so it gets ignored during scanning
        e2[100] ^= 0xFF;

        let buf = build_log_buffer(vec![e1, e2]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        // Only e1 should be in the active sequence — e2 is ignored due to CRC failure
        assert_eq!(active.len(), 1);
        assert_eq!(active.entries()[0].entry.header().sequence_number(), 1);
    }

    #[test]
    fn has_pending_log_zero_guid() {
        let guid = test_log_guid();
        let zero_guid = Guid::from_bytes([0u8; 16]);
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);

        let log = Log::new(&buf).unwrap();
        // With zero GUID, should report no pending log
        assert!(!has_pending_log(&log, &zero_guid));
    }

    #[test]
    fn has_pending_log_with_sequence() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);

        let log = Log::new(&buf).unwrap();
        assert!(has_pending_log(&log, &guid));
    }

    #[test]
    fn has_pending_log_empty_buffer() {
        let guid = test_log_guid();
        let buf = vec![0u8; SECTOR_SIZE as usize * 4];
        let log = Log::new(&buf).unwrap();
        assert!(!has_pending_log(&log, &guid));
    }

    // -----------------------------------------------------------------------
    // Replay overlay tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_overlay_single_data_descriptor() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        let overlay = build_replay_overlay(&active).unwrap();

        assert_eq!(overlay.last_file_offset(), 0x2_0000_0000);
        assert!(overlay.sectors().contains_key(&0x1000));
        assert_eq!(overlay.sectors()[&0x1000].len(), 4096);

        // Verify the assembled sector: LeadingBytes(8) + Data(4084) + TrailingBytes(4)
        let sector = &overlay.sectors()[&0x1000];
        // Leading bytes: 0x0102030405060708
        assert_eq!(&sector[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        // Middle 4084 bytes: fill_byte 0xAA
        assert_eq!(sector[8], 0xAA);
        assert_eq!(sector[4091], 0xAA);
        // Trailing bytes: 0xDEADBEEF
        assert_eq!(&sector[4092..4096], &0xDEAD_BEEFu32.to_le_bytes());
    }

    #[test]
    fn build_overlay_zero_descriptor() {
        let guid = test_log_guid();
        let entry = build_log_entry(
            1,
            0,
            &[(false, 0x5000, 0x2000)], // zero: offset 0x5000, length 0x2000
            0,
            &guid,
        );
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        let overlay = build_replay_overlay(&active).unwrap();

        assert_eq!(overlay.zeros().len(), 1);
        assert_eq!(overlay.zeros()[0], (0x5000, 0x2000));
        assert!(overlay.sectors().is_empty());
    }

    #[test]
    fn build_overlay_mixed_descriptors() {
        let guid = test_log_guid();
        let entry = build_log_entry(
            1,
            0,
            &[
                (true, 0x1000, 0),       // data at 0x1000
                (false, 0x5000, 0x2000), // zero at 0x5000, len 0x2000
                (true, 0x2000, 0),       // data at 0x2000
            ],
            0xCC,
            &guid,
        );
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        let overlay = build_replay_overlay(&active).unwrap();

        assert_eq!(overlay.sectors().len(), 2);
        assert!(overlay.sectors().contains_key(&0x1000));
        assert!(overlay.sectors().contains_key(&0x2000));
        assert_eq!(overlay.zeros().len(), 1);
        assert_eq!(overlay.zeros()[0], (0x5000, 0x2000));
    }

    #[test]
    fn active_sequence_entries_are_in_replay_order() {
        let guid = test_log_guid();
        let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
        let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);
        let buf = build_log_buffer(vec![e1, e2, e3]);

        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        // Replay order must be tail→head (1, 2, 3)
        let seqs: Vec<u64> = active
            .entries()
            .iter()
            .map(|e| e.entry.header().sequence_number())
            .collect();
        assert_eq!(seqs, vec![1, 2, 3]);
    }

    #[test]
    fn entry_with_no_descriptors_replays_ok() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[], 0, &guid);
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        let overlay = build_replay_overlay(&active).unwrap();
        assert_eq!(overlay.sectors().len(), 0);
        assert_eq!(overlay.zeros().len(), 0);
    }

    // -----------------------------------------------------------------------
    // replay_to_file tests (write to temp file)
    // -----------------------------------------------------------------------

    #[test]
    fn replay_to_file_writes_data() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replay_test.vhdx");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();

        // Pre-size to avoid set_len on non-extended regions
        file.set_len(0x2_0000_0000).unwrap();

        replay_to_file(&file, &active).unwrap();

        // Read back the written sector
        let mut read_buf = [0u8; 4096];
        let mut f = std::fs::File::open(&path).unwrap();
        f.seek(SeekFrom::Start(0x1000)).unwrap();
        f.read_exact(&mut read_buf).unwrap();

        // Verify assembled sector
        assert_eq!(&read_buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(read_buf[8], 0xAA);
    }

    #[test]
    fn active_sequence_flushed_and_last_file_offsets() {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        assert_eq!(active.flushed_file_offset(), 0x1_0000_0000);
        assert_eq!(active.last_file_offset(), 0x2_0000_0000);
    }

    #[test]
    fn candidate_with_higher_seq_always_wins() {
        let guid = test_log_guid();
        // Build three separate 1-entry sequences with different seq numbers
        let e5 = build_log_entry(5, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
        let mut e10 = build_log_entry(10, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
        let mut e100 = build_log_entry(100, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

        // Fix tail offsets so each entry points to itself (self-tail)
        // e5 is at offset 0, tail=0 is already correct
        let e5_len = e5.len();
        let e10_offset = e5_len;
        let e100_offset = e5_len + e10.len();

        // Fix e10 tail to point to itself (zero CRC field before recomputing)
        e10[12..16].copy_from_slice(
            &u32::try_from(e10_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        e10[4..8].copy_from_slice(&0u32.to_le_bytes());
        let e10_crc = crc32c(&e10);
        e10[4..8].copy_from_slice(&e10_crc.to_le_bytes());

        // Fix e100 tail to point to itself (zero CRC field before recomputing)
        e100[12..16].copy_from_slice(
            &u32::try_from(e100_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        e100[4..8].copy_from_slice(&0u32.to_le_bytes());
        let e100_checksum = crc32c(&e100);
        e100[4..8].copy_from_slice(&e100_checksum.to_le_bytes());

        // Place them in buffer; each with self-tail pointing to its own offset
        let buf = build_log_buffer(vec![e5, e10, e100]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();

        // Should pick the 1-entry sequence with seq=100
        assert_eq!(active.len(), 1);
        assert_eq!(active.entries()[0].entry.header().sequence_number(), 100);
    }

    // -----------------------------------------------------------------------
    // ReplayOverlay::read() tests
    // -----------------------------------------------------------------------

    /// Helper: create a dummy `File` for `read()` calls (the parameter is unused).
    fn dummy_file() -> std::fs::File {
        tempfile::tempfile().unwrap()
    }

    /// Helper: build an overlay from a single entry with the given descriptors.
    fn build_overlay_from_descs(desc_specs: &[(bool, u64, u64)], fill_byte: u8) -> ReplayOverlay {
        let guid = test_log_guid();
        let entry = build_log_entry(1, 0, desc_specs, fill_byte, &guid);
        let buf = build_log_buffer(vec![entry]);
        let log = Log::new(&buf).unwrap();
        let active = detect_active_sequence(&log, &guid).unwrap();
        build_replay_overlay(&active).unwrap()
    }

    #[test]
    fn read_returns_overlay_data_at_sector_offset() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
        let file = dummy_file();

        // Read at the exact sector offset
        let mut buf = [0u8; 4096];
        let n = overlay.read(&file, 0x1000, &mut buf);
        assert_eq!(n, 4096);

        // Verify assembled sector content: LeadingBytes + fill + TrailingBytes
        assert_eq!(&buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(buf[8], 0xAA);
        assert_eq!(buf[4091], 0xAA);
        assert_eq!(&buf[4092..4096], &0xDEAD_BEEFu32.to_le_bytes());
    }

    #[test]
    fn read_returns_zero_for_non_overlaid_offset() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
        let file = dummy_file();

        let mut buf = [0u8; 64];
        let n = overlay.read(&file, 0x9000, &mut buf);
        assert_eq!(n, 0, "should return Ok(0) for non-overlaid offset");
    }

    #[test]
    fn read_at_mid_sector_offset() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xBB);
        let file = dummy_file();

        // Read starting 100 bytes into the sector
        let mut buf = [0u8; 200];
        let n = overlay.read(&file, 0x1000 + 100, &mut buf);
        assert_eq!(n, 200);

        // The assembled sector has fill_byte 0xBB at indices 8..4092,
        // so at file offset 0x1000+100 = sector byte 100, which is in
        // the middle fill region (byte 100 > 8).
        assert_eq!(buf[0], 0xBB);
    }

    #[test]
    fn read_partial_buf_smaller_than_sector() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xCC);
        let file = dummy_file();

        // Only read 10 bytes
        let mut buf = [0u8; 10];
        let n = overlay.read(&file, 0x1000, &mut buf);
        assert_eq!(n, 10);

        // First 8 bytes are leading bytes
        assert_eq!(&buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        // Next 2 bytes are fill byte
        assert_eq!(buf[8], 0xCC);
        assert_eq!(buf[9], 0xCC);
    }

    #[test]
    fn read_near_end_of_sector() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xDD);
        let file = dummy_file();

        // Read starting 2 bytes before the sector end
        let offset = 0x1000 + 4094;
        let mut buf = [0u8; 64];
        let n = overlay.read(&file, offset, &mut buf);

        // Only 2 bytes remain in the sector
        assert_eq!(n, 2);
        // Last 4 bytes of sector are trailing bytes 0xDEADBEEF
        // At offset 4094, we get bytes 4094..4096 = last 2 bytes of trailing
        let trailing = 0xDEAD_BEEFu32.to_le_bytes();
        assert_eq!(&buf[0..2], &trailing[2..4]);
    }

    #[test]
    fn read_at_sector_boundary_returns_zero() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xEE);
        let file = dummy_file();

        // Exactly at the sector end — no more data
        let mut buf = [0u8; 64];
        let n = overlay.read(&file, 0x1000 + 4096, &mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn read_zero_region_returns_zeroes() {
        let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
        let file = dummy_file();

        let mut buf = [0xFFu8; 256];
        let n = overlay.read(&file, 0x5000, &mut buf);
        assert_eq!(n, 256);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn read_zero_region_mid_offset() {
        let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
        let file = dummy_file();

        // Read 100 bytes starting 1000 bytes into the zero region
        let mut buf = [0xFFu8; 100];
        let n = overlay.read(&file, 0x5000 + 1000, &mut buf);
        assert_eq!(n, 100);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn read_zero_region_beyond_returns_zero() {
        let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
        let file = dummy_file();

        let mut buf = [0u8; 64];
        let n = overlay.read(&file, 0x5000 + 0x2000, &mut buf);
        assert_eq!(n, 0, "beyond zero region should return Ok(0)");
    }

    #[test]
    fn read_empty_buf() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
        let file = dummy_file();

        let mut buf = [];
        let n = overlay.read(&file, 0x1000, &mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn read_data_sector_overlaps_zero_region_at_different_offset() {
        // Data sector at 0x1000, zero region at 0x5000 — they don't overlap
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0), (false, 0x5000, 0x2000)], 0xAA);
        let file = dummy_file();

        // Read from data sector
        let mut buf = [0u8; 8];
        let n = overlay.read(&file, 0x1000, &mut buf);
        assert_eq!(n, 8);

        // Read from zero region
        buf.fill(0xFF);
        let n = overlay.read(&file, 0x5000, &mut buf);
        assert_eq!(n, 8);
        assert!(buf.iter().all(|&b| b == 0));

        // Read from neither
        buf.fill(0xFF);
        let n = overlay.read(&file, 0x3000, &mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn read_with_multiple_sectors() {
        let overlay = build_overlay_from_descs(&[(true, 0x1000, 0), (true, 0x3000, 0)], 0xAA);
        let file = dummy_file();

        // First sector at 0x1000
        let mut buf = [0u8; 8];
        let n = overlay.read(&file, 0x1000, &mut buf);
        assert_eq!(n, 8);

        // Second sector at 0x3000
        let n = overlay.read(&file, 0x3000, &mut buf);
        assert_eq!(n, 8);

        // Between sectors (0x2000) — no overlay
        let n = overlay.read(&file, 0x2000, &mut buf);
        assert_eq!(n, 0);
    }

    // -----------------------------------------------------------------------
    // ReplayOverlay::apply_to_region() tests
    // -----------------------------------------------------------------------

    /// Data sector fully inside region → bytes copied, rest unchanged.
    #[test]
    fn apply_region_data_sector_fully_inside() {
        let mut sectors = HashMap::new();
        sectors.insert(0x1000, vec![0xAAu8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let mut region = vec![0xFFu8; 0x3000]; // [0, 0x3000)
        overlay.apply_to_region(&mut region, 0);

        // Bytes [0x1000..0x2000) should be 0xAA
        assert!(region[0x1000..0x2000].iter().all(|&b| b == 0xAA));
        // Bytes before sector unchanged
        assert!(region[0..0x1000].iter().all(|&b| b == 0xFF));
        // Bytes after sector unchanged
        assert!(region[0x2000..0x3000].iter().all(|&b| b == 0xFF));
    }

    /// Data sector partially overlapping region start.
    #[test]
    fn apply_region_data_sector_partial_overlap() {
        let mut sectors = HashMap::new();
        sectors.insert(0x0000, vec![0xBBu8; 4096]); // sector [0, 0x1000)
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let mut region = vec![0xFFu8; 0x800]; // [0x800, 0x1000)
        overlay.apply_to_region(&mut region, 0x800);

        // Region [0x800..0x1000) → sector bytes at offsets 0x800..0x1000
        assert!(region.iter().all(|&b| b == 0xBB));
    }

    /// Data sector entirely outside region → no change.
    #[test]
    fn apply_region_data_sector_outside() {
        let mut sectors = HashMap::new();
        sectors.insert(0x5000, vec![0xCCu8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let mut region = vec![0xFFu8; 0x1000]; // [0, 0x1000)
        overlay.apply_to_region(&mut region, 0);

        // No bytes should change
        assert!(region.iter().all(|&b| b == 0xFF));
    }

    /// Zero region applies zeros to the region.
    #[test]
    fn apply_region_zero_fills_zeros() {
        let overlay = ReplayOverlay::from_raw(HashMap::new(), vec![(0x2000, 0x1000)]);

        let mut region = vec![0xFFu8; 0x5000]; // [0, 0x5000)
        overlay.apply_to_region(&mut region, 0);

        // Bytes [0x2000..0x3000) should be zero
        assert!(region[0x2000..0x3000].iter().all(|&b| b == 0));
        // Bytes before zero region unchanged
        assert!(region[0..0x2000].iter().all(|&b| b == 0xFF));
        // Bytes after zero region unchanged
        assert!(region[0x3000..0x5000].iter().all(|&b| b == 0xFF));
    }

    /// Data sector takes priority over zero region at same offset.
    #[test]
    fn apply_region_data_priority_over_zero() {
        let mut sectors = HashMap::new();
        sectors.insert(0x1000, vec![0xDDu8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![(0x1000, 0x1000)]);

        let mut region = vec![0xFFu8; 0x3000]; // [0, 0x3000)
        overlay.apply_to_region(&mut region, 0);

        // Data sector wins: [0x1000..0x2000) should be 0xDD, NOT zero
        assert!(region[0x1000..0x2000].iter().all(|&b| b == 0xDD));
        // Byte at 0x2000 should still be 0xFF (zero region was [0x1000, 0x1000), ends at 0x2000)
        assert_eq!(region[0x2000], 0xFF);
    }

    /// Multiple data sectors and one zero region handled correctly.
    #[test]
    fn apply_region_multiple_overlapping_entries() {
        let mut sectors = HashMap::new();
        sectors.insert(0x1000, vec![0x11u8; 4096]);
        sectors.insert(0x3000, vec![0x22u8; 4096]);
        let overlay = ReplayOverlay::from_raw(
            sectors,
            vec![(0x2000, 0x2000)], // zero region [0x2000, 0x4000)
        );

        let mut region = vec![0xFFu8; 0x5000]; // [0, 0x5000)
        overlay.apply_to_region(&mut region, 0);

        // First data sector: [0x1000..0x2000) = 0x11
        assert!(region[0x1000..0x2000].iter().all(|&b| b == 0x11));
        // Zero region [0x2000..0x3000) = 0 (not covered by data)
        assert!(region[0x2000..0x3000].iter().all(|&b| b == 0));
        // Second data sector: [0x3000..0x4000) = 0x22 (overrides zero)
        assert!(region[0x3000..0x4000].iter().all(|&b| b == 0x22));
        // [0x4000..0x5000) unchanged
        assert!(region[0x4000..0x5000].iter().all(|&b| b == 0xFF));
    }

    /// Empty overlay → no change to region.
    #[test]
    fn apply_region_empty_overlay() {
        let overlay = ReplayOverlay::from_raw(HashMap::new(), vec![]);

        let mut region = vec![0xFFu8; 0x1000];
        overlay.apply_to_region(&mut region, 0);

        assert!(region.iter().all(|&b| b == 0xFF));
    }

    /// Single byte overlap between sector and region.
    #[test]
    fn apply_region_single_byte_overlap() {
        let mut sectors = HashMap::new();
        sectors.insert(0x1000, vec![0xEEu8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let mut region = vec![0xFFu8; 2]; // [0xFFF, 0x1001)
        overlay.apply_to_region(&mut region, 0xFFF);

        // region[0] = file offset 0xFFF (before sector) → unchanged
        assert_eq!(region[0], 0xFF);
        // region[1] = file offset 0x1000 (start of sector) → gets 0xEE
        assert_eq!(region[1], 0xEE);
    }

    /// Region offset exactly at sector boundary.
    #[test]
    fn apply_region_offset_at_sector_boundary() {
        let mut sectors = HashMap::new();
        sectors.insert(0x1000, vec![0x77u8; 4096]);
        let overlay = ReplayOverlay::from_raw(sectors, vec![]);

        let mut region = vec![0xFFu8; 0x1000]; // [0x1000, 0x2000)
        overlay.apply_to_region(&mut region, 0x1000);

        // Entire region should be filled with sector data
        assert!(region.iter().all(|&b| b == 0x77));
    }
}
