//! Core file types: [`File`], [`OpenOptions`], [`CreateOptions`], and opening policies.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use crate::constants::{
    BAT_REGION_GUID, KNOWN_METADATA_GUIDS, KNOWN_REGION_GUIDS, METADATA_REGION_GUID, MIB,
};
use crate::error::{Error, Result};
use crate::header::Header;
use crate::io::platform::write_at;
use crate::log_replay::ReplayOverlay;
use crate::section::Sections;
use crate::types::Guid;

use super::{CreateOptions, LogReplayPolicy, OpenOptions};

// Signatures are written as byte literals (b"head", b"regi", etc.) to avoid
// endianness issues — they are byte-strings, not numeric values.

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check whether a GUID corresponds to a known region type.
pub(crate) fn is_known_region_guid(guid: &Guid) -> bool {
    KNOWN_REGION_GUIDS.contains(guid)
}

/// Check whether a GUID corresponds to a known metadata item type.
pub(crate) fn is_known_metadata_guid(guid: &Guid) -> bool {
    KNOWN_METADATA_GUIDS.contains(guid)
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

/// An opened VHDX file.
///
/// Obtain via [`File::open`] followed by [`OpenOptions::finish`], or via
/// [`File::create`] followed by [`CreateOptions::finish`].
pub struct File {
    pub(super) inner: std::fs::File,
    pub(super) path: PathBuf,
    /// First 1 MB of the file, buffered for header section parsing.
    pub(super) header_buf: Vec<u8>,
    /// Cached BAT region data (lazy-loaded by [`File::bat_buf`]).
    pub(super) bat_buf: RwLock<Option<Vec<u8>>>,
    /// Cached metadata region data (lazy-loaded by [`File::metadata_buf`]).
    pub(super) metadata_buf: OnceLock<Vec<u8>>,
    /// Cached log region data (lazy-loaded by [`File::log_buf`]).
    pub(super) log_buf: OnceLock<Vec<u8>>,
    /// Whether the file was opened with write access.
    pub(super) write: bool,
    /// Strict validation mode.
    pub(super) strict: bool,
    /// Configured log replay policy.
    pub(super) log_replay_policy: LogReplayPolicy,
    /// In-memory replay overlay (for `InMemoryOnReadOnly` / Auto read-only).
    pub(super) replay_overlay: Option<Arc<ReplayOverlay>>,
    /// Cached validator buffer: assembled region data at correct file offsets.
    pub(super) validator_buf: OnceLock<Vec<u8>>,
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("path", &self.path)
            .field("write", &self.write)
            .field("strict", &self.strict)
            .field("log_replay_policy", &self.log_replay_policy)
            .finish_non_exhaustive()
    }
}

impl File {
    // -- Open ---------------------------------------------------------------

    /// Begin opening an existing VHDX file (read-only by default).
    ///
    /// Returns an [`OpenOptions`] builder for chaining configuration.
    /// Call [`OpenOptions::finish`] to complete the open operation.
    ///
    /// # Standard
    ///
    /// docs/Standard/MS-VHDX-只读扩展标准.md (read-only semantic boundary)
    pub fn open(path: impl AsRef<Path>) -> OpenOptions {
        OpenOptions {
            path: path.as_ref().to_owned(),
            write: false,
            strict: true,
            log_replay_policy: LogReplayPolicy::Require,
        }
    }

    // -- Create -------------------------------------------------------------

    /// Begin creating a new VHDX file at `path`.
    pub fn create(path: impl AsRef<Path>) -> CreateOptions {
        CreateOptions {
            path: path.as_ref().to_path_buf(),
            virtual_size: 0,
            fixed: false,
            block_size: 32 * 1024 * 1024, // 32 MB default
            logical_sector_size: 4096,
            physical_sector_size: 4096,
            parent_path: None,
        }
    }

    // -- Accessors ----------------------------------------------------------

    /// The filesystem path of this VHDX file.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Return a `Sections` container for this file.
    pub fn sections(&self) -> Sections<'_> {
        Sections::new(self)
    }

    /// Return the underlying OS file handle.
    ///
    /// Intended for diagnostics only. Must not be used for virtual disk
    /// payload data-plane reads or writes.
    pub fn inner(&self) -> &std::fs::File {
        &self.inner
    }

    /// Whether the file was opened with write access.
    pub(crate) fn is_write(&self) -> bool {
        self.write
    }

    /// The configured strict mode flag.
    pub(crate) fn is_strict(&self) -> bool {
        self.strict
    }

    /// The configured log replay policy.
    #[cfg(test)]
    pub(crate) fn log_replay_policy(&self) -> LogReplayPolicy {
        self.log_replay_policy
    }

    /// Return a shared reference to the replay overlay, if one was built.
    ///
    /// Used by [`IO::new`] to populate the overlay field for the data plane.
    pub(crate) fn replay_overlay_arc(&self) -> Option<&Arc<ReplayOverlay>> {
        self.replay_overlay.as_ref()
    }

    /// Return a new `SpecValidator` for structural validation.
    ///
    /// The validator borrows from the file's internal `validator_buf` cache,
    /// which contains all region data assembled at their correct file offsets,
    /// and uses the file's strict mode setting.
    ///
    /// # Panics
    ///
    /// Panics if file metadata/header buffers are internally inconsistent.
    pub fn validator(&self) -> crate::validation::SpecValidator<'_> {
        crate::validation::SpecValidator::from_file(self)
    }

    /// Return a new `IO` handle for sector-level reads and writes.
    ///
    /// This is the **sole data-plane entry point**. All virtual disk payload
    /// reads and writes must go through the returned `IO` object.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata is missing, malformed, or contains invalid
    /// parameters (e.g. zero block size, missing `FileParameters`, missing
    /// `VirtualDiskSize`, zero sector size).
    pub fn io(&self) -> Result<crate::io::IO<'_>> {
        crate::io::IO::new(self)
    }

    /// Access the buffered header data (first 1 MB).
    pub(crate) fn header_buf(&self) -> &[u8] {
        &self.header_buf
    }

    /// Lazy-load the BAT region data from disk.
    ///
    /// Reads the BAT region using the offset and length stored in the
    /// header's region table. Subsequent calls return the cached buffer.
    ///
    /// Thread-safe: under concurrent access, both threads may load from disk
    /// but only one result is cached; the other is silently discarded. The
    /// returned buffer is always valid regardless of which thread wins.
    pub(crate) fn bat_buf(&self) -> Result<Vec<u8>> {
        if let Some(buf) = self
            .bat_buf
            .read()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))?
            .as_ref()
        {
            return Ok(buf.clone());
        }

        let data = self.read_region_with_overlay(BAT_REGION_GUID, Self::read_bat_region)?;

        *self
            .bat_buf
            .write()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))? =
            Some(data.clone());

        Ok(data)
    }

    pub(crate) fn write_bat_entry(&self, bat_array_idx: u64, raw_entry: [u8; 8]) -> Result<()> {
        let header = Header::new(&self.header_buf)?;
        let rt = header.region_table(0)?;
        let bat_region = rt
            .entries()
            .find(|entry| entry.guid() == BAT_REGION_GUID)
            .ok_or_else(|| Error::InvalidFile("BAT region not found in region table".into()))?;
        let entry_offset = bat_array_idx
            .checked_mul(8)
            .and_then(|offset| bat_region.file_offset().checked_add(offset))
            .ok_or_else(|| Error::InvalidParameter("BAT entry offset overflow".into()))?;

        Self::write_all_at(&self.inner, &raw_entry, entry_offset)?;

        if let Some(buf) = self
            .bat_buf
            .write()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))?
            .as_mut()
        {
            let cache_offset = usize::try_from(bat_array_idx)
                .map_err(|_| Error::InvalidParameter("BAT index does not fit usize".into()))?
                .checked_mul(8)
                .ok_or_else(|| Error::InvalidParameter("BAT cache offset overflow".into()))?;
            let cache_end = cache_offset
                .checked_add(8)
                .ok_or_else(|| Error::InvalidParameter("BAT cache end overflow".into()))?;
            if cache_end > buf.len() {
                return Err(Error::InvalidParameter(
                    "BAT entry index exceeds cached BAT region".into(),
                ));
            }
            buf[cache_offset..cache_end].copy_from_slice(&raw_entry);
        }

        Ok(())
    }

    fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> Result<()> {
        while !buf.is_empty() {
            let written = write_at(file, buf, offset)?;
            if written == 0 {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write complete buffer",
                )));
            }
            offset += u64::try_from(written).expect("written byte count fits u64");
            buf = &buf[written..];
        }
        Ok(())
    }

    /// Lazy-load the Metadata region data from disk.
    ///
    /// Reads the metadata region using the offset and length stored in the
    /// header's region table. Subsequent calls return the cached buffer.
    ///
    /// Thread-safe: under concurrent access, both threads may load from disk
    /// but only one result is cached; the other is silently discarded. The
    /// returned buffer is always valid regardless of which thread wins.
    pub(crate) fn metadata_buf(&self) -> Result<&[u8]> {
        // Fast path: already cached
        if let Some(buf) = self.metadata_buf.get() {
            return Ok(&buf[..]);
        }

        let data =
            self.read_region_with_overlay(METADATA_REGION_GUID, Self::read_metadata_region)?;

        // Thread-safe set: if another thread already set it, silently drop ours
        let _ = self.metadata_buf.set(data);

        // Return cached value (either from us or from the racing thread)
        Ok(self.metadata_buf.get().unwrap().as_slice())
    }

    fn read_region_with_overlay(
        &self, region_guid: Guid, read_region: fn(&Self) -> Result<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let mut data = read_region(self)?;
        if self.replay_overlay.is_none() {
            return Ok(data);
        }

        let header = Header::new(&self.header_buf)?;
        let rt = header.region_table(0)?;
        if let Some(entry) = rt.entries().find(|entry| entry.guid() == region_guid) {
            self.apply_replay_overlay(&mut data, entry.file_offset());
        }
        Ok(data)
    }
    /// Lazy-load the Log region data from disk.
    ///
    /// Reads the log region using the offset and length stored in the
    /// VHDX header structure. Subsequent calls return the cached buffer.
    ///
    /// # Panics
    ///
    /// Panics if the `OnceLock::set` operation fails (should not occur in
    /// single-threaded usage).
    pub(crate) fn log_buf(&self) -> Result<&[u8]> {
        if let Some(buf) = self.log_buf.get() {
            return Ok(&buf[..]);
        }
        let mut data = self.read_log_region()?;
        // Apply replay overlay if present
        if self.replay_overlay.is_some() {
            let header = Header::new(&self.header_buf)?;
            let current = header.header(0)?;
            self.apply_replay_overlay(&mut data, current.log_offset());
        }
        self.log_buf
            .set(data)
            .unwrap_or_else(|_| unreachable!("log_buf already initialized"));
        Ok(self.log_buf.get().unwrap().as_slice())
    }

    /// Return a reference to the cached validator data buffer.
    ///
    /// Lazily assembles all cached region buffers (header, log, BAT, metadata)
    /// at their correct absolute file offsets into a contiguous view.
    pub(crate) fn validator_buf(&self) -> &[u8] {
        self.validator_buf
            .get_or_init(|| self.build_validator_buf())
            .as_slice()
    }

    /// Build a contiguous buffer with all regions at correct file offsets.
    ///
    /// Parses the header section to discover region offsets, then reads each
    /// cached region buffer (header, log, BAT, metadata) and copies it into a
    /// contiguous zero-filled buffer at its absolute file offset. Regions that
    /// cannot be loaded are silently omitted.
    ///
    /// # Panics
    ///
    /// Panics if internal offset conversions from validated on-disk structures
    /// overflow `usize`. This should not happen with well-formed VHDX files.
    fn build_validator_buf(&self) -> Vec<u8> {
        // Parse header to find region offsets
        let Ok(header) = Header::new(&self.header_buf) else {
            return self.header_buf.clone();
        };
        let Ok(current) = header.header(0) else {
            return self.header_buf.clone();
        };
        let Ok(rt) = header.region_table(0) else {
            return self.header_buf.clone();
        };

        let log_offset = usize::try_from(current.log_offset()).unwrap();
        let log_length = usize::try_from(current.log_length()).unwrap();
        let header_log_guid = current.log_guid();

        // Determine maximum extent across all regions
        let mut max_end = (MIB as usize).max(log_offset + log_length);
        for entry in rt.entries() {
            let end = usize::try_from(entry.file_offset()).unwrap()
                + usize::try_from(entry.length()).unwrap();
            max_end = max_end.max(end);
        }

        let mut buf = vec![0u8; max_end];

        // Copy header at offset 0
        let header_len = self.header_buf.len().min(MIB as usize);
        buf[..header_len].copy_from_slice(&self.header_buf[..header_len]);

        // Copy log region at log_offset.
        // Skip if the header's log GUID is all zeros — this indicates that
        // no log was ever written, and including non-zero data from the file
        // would cause the validator to report a GUID mismatch.
        let has_zero_log_guid = header_log_guid.to_bytes() == [0u8; 16];
        if log_offset > 0
            && log_length > 0
            && !has_zero_log_guid
            && let Ok(log_data) = self.log_buf()
        {
            let copy_len = log_data.len().min(log_length);
            let end = log_offset + copy_len;
            if end <= max_end {
                buf[log_offset..end].copy_from_slice(&log_data[..copy_len]);
            }
        }

        // Copy BAT and Metadata regions at their region-table offsets
        for entry in rt.entries() {
            let guid = entry.guid();
            let offset = usize::try_from(entry.file_offset()).unwrap();
            let length = usize::try_from(entry.length()).unwrap();

            let region_data: &[u8] = if guid == BAT_REGION_GUID {
                &self.bat_buf().unwrap_or_default()
            } else if guid == METADATA_REGION_GUID {
                self.metadata_buf().unwrap_or(&[])
            } else {
                continue;
            };

            if !region_data.is_empty() {
                let copy_len = region_data.len().min(length);
                let end = offset + copy_len;
                if end <= max_end {
                    buf[offset..end].copy_from_slice(&region_data[..copy_len]);
                }
            }
        }

        buf
    }

    // -- Region readers (private helpers) ------------------------------------

    /// Read the BAT region from the file using the region table.
    fn read_bat_region(&self) -> Result<Vec<u8>> {
        let header = Header::new(&self.header_buf)?;
        let rt = header.region_table(0)?;
        for entry in rt.entries() {
            if entry.guid() == BAT_REGION_GUID {
                let offset = entry.file_offset();
                let length = entry.length() as usize;
                let mut buf = vec![0u8; length];
                let mut reader = &self.inner;
                reader.seek(SeekFrom::Start(offset))?;
                reader.read_exact(&mut buf)?;
                return Ok(buf);
            }
        }
        Err(Error::InvalidFile(
            "BAT region not found in region table".into(),
        ))
    }

    /// Read the Metadata region from the file using the region table.
    fn read_metadata_region(&self) -> Result<Vec<u8>> {
        let header = Header::new(&self.header_buf)?;
        let rt = header.region_table(0)?;
        for entry in rt.entries() {
            if entry.guid() == METADATA_REGION_GUID {
                let offset = entry.file_offset();
                let length = entry.length() as usize;
                let mut buf = vec![0u8; length];
                let mut reader = &self.inner;
                reader.seek(SeekFrom::Start(offset))?;
                reader.read_exact(&mut buf)?;
                return Ok(buf);
            }
        }
        Err(Error::InvalidFile(
            "Metadata region not found in region table".into(),
        ))
    }

    /// Read the Log region from the file using header-specified offset/length.
    fn read_log_region(&self) -> Result<Vec<u8>> {
        let header = Header::new(&self.header_buf)?;
        let h = header.header(0)?;
        let offset = h.log_offset();
        let length = h.log_length() as usize;
        let mut buf = vec![0u8; length];
        let mut reader = &self.inner;
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// If a replay overlay exists, apply it to the given region buffer.
    fn apply_replay_overlay(&self, region_data: &mut [u8], region_offset: u64) {
        if let Some(ref overlay) = self.replay_overlay {
            overlay.apply_to_region(region_data, region_offset);
        }
    }
}
