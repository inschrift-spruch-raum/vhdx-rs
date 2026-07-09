//! Core medium types: [`Medium`], [`OpenOptions`], [`CreateOptions`], and opening policies.

use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use crate::constants::{
    BAT_REGION_GUID, HEADER_BUFFER_SIZE, KNOWN_METADATA_GUIDS, KNOWN_REGION_GUIDS,
    METADATA_REGION_GUID, MIB,
};
use crate::error::{Error, Result};
use crate::header::Header;
use crate::log_replay::ReplayOverlay;
use crate::section::Sections;
use crate::types::Guid;

use super::{CreateOptions, LogReplayPolicy, OpenOptions, ParentMedium, ParentResolver, ReadOnly};

pub(crate) fn read_exact_at<T>(inner: &mut T, offset: u64, buf: &mut [u8]) -> std::io::Result<()>
where
    T: Read + Seek,
{
    inner.seek(SeekFrom::Start(offset))?;
    inner.read_exact(buf)
}

pub(crate) fn write_all_at<T>(inner: &mut T, offset: u64, buf: &[u8]) -> std::io::Result<()>
where
    T: Write + Seek,
{
    inner.seek(SeekFrom::Start(offset))?;
    inner.write_all(buf)
}

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
// Medium
// ---------------------------------------------------------------------------

/// An opened VHDX medium.
///
/// Obtain via [`Medium::open`] followed by [`OpenOptions::finish`], or via
/// [`Medium::create`] followed by [`CreateOptions::finish`].
pub struct Medium<T = std::fs::File> {
    pub(super) inner: Mutex<T>,
    /// First 1 MB of the file, buffered for header section parsing.
    pub(super) header_buf: RwLock<Option<CacheEntry>>,
    /// Cached BAT region data (lazy-loaded by [`Medium::bat_buf`]).
    pub(super) bat_buf: RwLock<Option<CacheEntry>>,
    /// Cached metadata region data (lazy-loaded by [`Medium::metadata_buf`]).
    pub(super) metadata_buf: RwLock<Option<CacheEntry>>,
    /// Cached log region data (lazy-loaded by [`Medium::log_buf`]).
    pub(super) log_buf: RwLock<Option<CacheEntry>>,
    /// Monotonic cache epoch bumped by structured writes that affect regions.
    pub(super) generation: AtomicU64,
    /// Whether the file was opened with write access.
    pub(super) write: bool,
    /// Strict validation mode.
    pub(super) strict: bool,
    /// Configured log replay policy.
    pub(super) log_replay_policy: LogReplayPolicy,
    /// In-memory replay overlay (for `InMemoryOnReadOnly` / Auto read-only).
    pub(super) replay_overlay: Option<Arc<ReplayOverlay>>,
    /// Resolver used for differencing disk parent reads.
    pub(crate) parent_resolver: Mutex<Option<Box<dyn ParentResolver + Send>>>,
    /// Resolved differencing disk parent cache.
    pub(crate) resolved_parent: Mutex<Option<Box<dyn ParentMedium>>>,
    /// Read-ahead cache for sequential parent-backed differencing disk reads.
    pub(crate) parent_read_cache: Mutex<ParentReadCache>,
    /// Read-ahead cache for effective sectors served while this medium is used as a parent.
    pub(crate) parent_effective_read_cache: Mutex<ParentReadCache>,
    /// Cached validator buffer: assembled region data at correct file offsets.
    pub(super) validator_buf: RwLock<Option<CacheEntry>>,
}

#[derive(Clone)]
pub(crate) struct CacheEntry {
    pub(super) generation: u64,
    pub(super) bytes: Arc<[u8]>,
}

impl CacheEntry {
    pub(super) fn new(generation: u64, bytes: Arc<[u8]>) -> Self {
        Self { generation, bytes }
    }

    fn valid_bytes(&self, generation: u64) -> Option<Arc<[u8]>> {
        (self.generation == generation).then(|| Arc::clone(&self.bytes))
    }
}

#[derive(Default)]
pub(crate) struct ParentReadCache {
    logical_sector_size: u32,
    start_sector: u64,
    sector_count: u64,
    bytes: Vec<u8>,
}

impl ParentReadCache {
    pub(crate) fn copy_if_contains(
        &self, start_sector: u64, sector_count: u64, logical_sector_size: u32, buf: &mut [u8],
    ) -> bool {
        if self.logical_sector_size != logical_sector_size || self.sector_count == 0 {
            return false;
        }

        let Some(end_sector) = start_sector.checked_add(sector_count) else {
            return false;
        };
        let Some(cache_end_sector) = self.start_sector.checked_add(self.sector_count) else {
            return false;
        };
        if start_sector < self.start_sector || end_sector > cache_end_sector {
            return false;
        }

        let Some((byte_offset, byte_len)) =
            self.byte_range(start_sector, sector_count, logical_sector_size)
        else {
            return false;
        };
        let Some(byte_end) = byte_offset.checked_add(byte_len) else {
            return false;
        };
        if byte_end > self.bytes.len() || buf.len() != byte_len {
            return false;
        }

        buf.copy_from_slice(&self.bytes[byte_offset..byte_end]);
        true
    }

    pub(crate) fn replace(
        &mut self, start_sector: u64, sector_count: u64, logical_sector_size: u32, bytes: Vec<u8>,
    ) {
        self.logical_sector_size = logical_sector_size;
        self.start_sector = start_sector;
        self.sector_count = sector_count;
        self.bytes = bytes;
    }

    pub(crate) fn clear(&mut self) {
        self.logical_sector_size = 0;
        self.start_sector = 0;
        self.sector_count = 0;
        self.bytes.clear();
    }

    fn byte_range(
        &self, start_sector: u64, sector_count: u64, logical_sector_size: u32,
    ) -> Option<(usize, usize)> {
        let sector_offset = start_sector.checked_sub(self.start_sector)?;
        let logical_sector_size = usize::try_from(logical_sector_size).ok()?;
        let byte_offset = usize::try_from(sector_offset)
            .ok()?
            .checked_mul(logical_sector_size)?;
        let byte_len = usize::try_from(sector_count)
            .ok()?
            .checked_mul(logical_sector_size)?;
        Some((byte_offset, byte_len))
    }
}

/// Read-only guard for the caller-provided underlying medium.
pub struct InnerRef<'a, T> {
    guard: MutexGuard<'a, T>,
}

impl<T> Deref for InnerRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T> std::fmt::Debug for Medium<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Medium")
            .field("write", &self.write)
            .field("strict", &self.strict)
            .field("log_replay_policy", &self.log_replay_policy)
            .finish_non_exhaustive()
    }
}

impl<T> Medium<T> {
    /// Borrow the caller-provided underlying medium.
    ///
    /// # Panics
    ///
    /// Panics if the internal medium lock is poisoned.
    pub fn get_ref(&self) -> InnerRef<'_, T> {
        InnerRef {
            guard: self.inner.lock().expect("medium inner lock poisoned"),
        }
    }

    /// Mutably borrow the caller-provided underlying medium.
    ///
    /// # Warning
    ///
    /// Direct mutation can invalidate cached header, BAT, metadata, log, and
    /// validator views. Prefer the crate's structured APIs for VHDX data-plane
    /// and metadata operations.
    ///
    /// # Panics
    ///
    /// Panics if the internal medium lock is poisoned.
    pub fn get_mut(&mut self) -> &mut T {
        self.invalidate_all_caches();
        self.inner.get_mut().expect("medium inner lock poisoned")
    }

    pub(crate) fn inner_mut(&mut self) -> &mut T {
        self.inner.get_mut().expect("medium inner lock poisoned")
    }

    fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn invalidate_all_caches(&self) {
        self.bump_generation();
        if let Ok(mut cache) = self.header_buf.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.bat_buf.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.metadata_buf.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.log_buf.write() {
            *cache = None;
        }
        if let Ok(mut cache) = self.validator_buf.write() {
            *cache = None;
        }
        if let Ok(mut parent) = self.resolved_parent.lock() {
            *parent = None;
        }
        if let Ok(mut cache) = self.parent_read_cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.parent_effective_read_cache.lock() {
            cache.clear();
        }
    }

    /// Consume this VHDX wrapper and return the underlying medium.
    pub fn into_inner(self) -> T {
        self.inner
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether the file was opened with write access.
    pub(crate) fn is_write(&self) -> bool {
        self.write
    }

    /// Return a shared reference to the replay overlay, if one was built.
    pub(crate) fn replay_overlay_arc(&self) -> Option<&Arc<ReplayOverlay>> {
        self.replay_overlay.as_ref()
    }

    /// Begin creating a new VHDX on a caller-provided medium.
    pub fn create(inner: T) -> CreateOptions<T> {
        CreateOptions {
            inner: Some(inner),
            virtual_size: 0,
            fixed: false,
            block_size: 32 * 1024 * 1024,
            logical_sector_size: 4096,
            physical_sector_size: 4096,
            parent: None,
        }
    }

    /// Begin opening an existing VHDX medium (read-only by default).
    ///
    /// Returns an options builder for chaining configuration.
    /// Call `finish` to complete the open operation.
    pub fn open(inner: T) -> OpenOptions<T, ReadOnly> {
        OpenOptions {
            inner,
            strict: true,
            log_replay_policy: LogReplayPolicy::Require,
            parent_resolver: None,
            _mode: std::marker::PhantomData,
        }
    }
}

impl<T> Medium<T>
where
    T: Read + Seek,
{
    /// Return a new `IO` handle for sector-level reads and writes.
    ///
    /// # Errors
    ///
    /// Returns an error if VHDX metadata needed to construct the I/O view is invalid.
    pub fn io(&mut self) -> Result<crate::io::IO<'_, T>> {
        crate::io::IO::new(self)
    }
}

impl<T> Medium<T>
where
    T: Read + Write + Seek,
{
    pub(crate) fn write_bat_entry(&mut self, bat_array_idx: u64, raw_entry: [u8; 8]) -> Result<()> {
        let header_buf = self.header_buf_arc()?;
        let header = Header::new(&header_buf)?;
        let rt = header.region_table(0)?;
        let bat_region = rt
            .entries()
            .find(|entry| entry.guid() == BAT_REGION_GUID)
            .ok_or_else(|| Error::InvalidFile("BAT region not found in region table".into()))?;
        let entry_offset = bat_array_idx
            .checked_mul(8)
            .and_then(|offset| bat_region.file_offset().checked_add(offset))
            .ok_or_else(|| Error::InvalidParameter("BAT entry offset overflow".into()))?;

        write_all_at(self.inner_mut(), entry_offset, &raw_entry)?;
        let generation = self.bump_generation();

        let mut bat_cache = self
            .bat_buf
            .write()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))?;
        if let Some(entry) = bat_cache.as_ref() {
            let cache_offset = usize::try_from(bat_array_idx)
                .map_err(|_| Error::InvalidParameter("BAT index does not fit usize".into()))?
                .checked_mul(8)
                .ok_or_else(|| Error::InvalidParameter("BAT cache offset overflow".into()))?;
            let cache_end = cache_offset
                .checked_add(8)
                .ok_or_else(|| Error::InvalidParameter("BAT cache end overflow".into()))?;
            if cache_end > entry.bytes.len() {
                return Err(Error::InvalidParameter(
                    "BAT entry index exceeds cached BAT region".into(),
                ));
            }
            let mut updated = entry.bytes.to_vec();
            updated[cache_offset..cache_end].copy_from_slice(&raw_entry);
            *bat_cache = Some(CacheEntry::new(generation, Arc::from(updated)));
        }

        *self
            .validator_buf
            .write()
            .map_err(|_| Error::InvalidFile("validator cache lock poisoned".into()))? = None;

        Ok(())
    }
}

impl<T> Medium<T>
where
    T: Read + Seek,
{
    // -- Accessors ----------------------------------------------------------

    /// Return a `Sections` container for this file.
    ///
    /// # Errors
    ///
    /// Returns an error if the header section cannot be loaded or parsed.
    pub fn sections(&self) -> Result<Sections<'_, T>> {
        Sections::new(self.header_buf_arc()?, self)
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

    /// Return a new `SpecValidator` for structural validation.
    ///
    /// The validator owns an `Arc<[u8]>` snapshot from the file's
    /// `validator_buf` cache. The snapshot contains all region data assembled
    /// at their correct file offsets and uses the file's strict mode setting.
    ///
    /// # Panics
    ///
    /// Panics if file metadata/header buffers are internally inconsistent.
    ///
    /// # Errors
    ///
    /// Returns an error if the VHDX structure cannot be loaded for validation.
    pub fn validator(&mut self) -> Result<crate::validation::SpecValidator> {
        crate::validation::SpecValidator::from_file(self)
    }

    /// Access the buffered header data (first 1 MB) as a stable snapshot.
    pub(crate) fn header_buf_arc(&self) -> Result<Arc<[u8]>> {
        let generation = self.current_generation();
        if let Some(entry) = self
            .header_buf
            .read()
            .map_err(|_| Error::InvalidFile("header cache lock poisoned".into()))?
            .as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }

        let mut buf = vec![0u8; HEADER_BUFFER_SIZE];
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| Error::InvalidFile("medium inner lock poisoned".into()))?;
            read_exact_at(&mut *inner, 0, &mut buf)?;
        }
        let buf = Arc::<[u8]>::from(buf);
        let mut cache = self
            .header_buf
            .write()
            .map_err(|_| Error::InvalidFile("header cache lock poisoned".into()))?;
        if let Some(entry) = cache.as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }
        *cache = Some(CacheEntry::new(generation, Arc::clone(&buf)));
        Ok(buf)
    }

    /// Lazy-load the BAT region data from disk.
    ///
    /// Reads the BAT region using the offset and length stored in the
    /// header's region table. Subsequent calls return the cached buffer.
    ///
    /// Thread-safe: under concurrent access, both threads may load from disk
    /// but only one result is cached; the other is silently discarded. The
    /// returned buffer is always valid regardless of which thread wins.
    pub(crate) fn bat_buf(&self) -> Result<Arc<[u8]>> {
        let generation = self.current_generation();
        if let Some(entry) = self
            .bat_buf
            .read()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))?
            .as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }

        let data = self.read_region_with_overlay(BAT_REGION_GUID, Self::read_bat_region)?;
        let data = Arc::<[u8]>::from(data);

        let mut cache = self
            .bat_buf
            .write()
            .map_err(|_| Error::InvalidFile("BAT cache lock poisoned".into()))?;
        if let Some(entry) = cache.as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }
        *cache = Some(CacheEntry::new(generation, Arc::clone(&data)));

        Ok(data)
    }

    /// Lazy-load the Metadata region data from disk.
    ///
    /// Reads the metadata region using the offset and length stored in the
    /// header's region table. Subsequent calls return the cached buffer.
    ///
    /// Thread-safe: under concurrent access, both threads may load from disk
    /// but only one result is cached; the other is silently discarded. The
    /// returned buffer is always valid regardless of which thread wins.
    pub(crate) fn metadata_buf(&self) -> Result<Arc<[u8]>> {
        let generation = self.current_generation();
        if let Some(entry) = self
            .metadata_buf
            .read()
            .map_err(|_| Error::InvalidFile("metadata cache lock poisoned".into()))?
            .as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }

        let data =
            self.read_region_with_overlay(METADATA_REGION_GUID, Self::read_metadata_region)?;
        let data = Arc::<[u8]>::from(data);
        let mut cache = self
            .metadata_buf
            .write()
            .map_err(|_| Error::InvalidFile("metadata cache lock poisoned".into()))?;
        if let Some(entry) = cache.as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }
        *cache = Some(CacheEntry::new(generation, Arc::clone(&data)));
        Ok(data)
    }

    fn read_region_with_overlay(
        &self, region_guid: Guid, read_region: fn(&Self) -> Result<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let mut data = read_region(self)?;
        if self.replay_overlay.is_none() {
            return Ok(data);
        }

        let header_buf = self.header_buf_arc()?;
        let header = Header::new(&header_buf)?;
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
    pub(crate) fn log_buf(&self) -> Result<Arc<[u8]>> {
        let generation = self.current_generation();
        if let Some(entry) = self
            .log_buf
            .read()
            .map_err(|_| Error::InvalidFile("log cache lock poisoned".into()))?
            .as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }

        let mut data = self.read_log_region()?;
        if self.replay_overlay.is_some() {
            let header_buf = self.header_buf_arc()?;
            let header = Header::new(&header_buf)?;
            let current = header.header(0)?;
            self.apply_replay_overlay(&mut data, current.log_offset());
        }
        let data = Arc::<[u8]>::from(data);
        let mut cache = self
            .log_buf
            .write()
            .map_err(|_| Error::InvalidFile("log cache lock poisoned".into()))?;
        if let Some(entry) = cache.as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }
        *cache = Some(CacheEntry::new(generation, Arc::clone(&data)));
        Ok(data)
    }

    /// Return an owned snapshot of the validator data buffer.
    ///
    /// Lazily assembles all cached region buffers (header, log, BAT, metadata)
    /// at their correct absolute file offsets into a contiguous view.
    pub(crate) fn validator_buf(&mut self) -> Result<Arc<[u8]>> {
        let generation = self.current_generation();
        if let Some(entry) = self
            .validator_buf
            .read()
            .map_err(|_| Error::InvalidFile("validator cache lock poisoned".into()))?
            .as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }

        let data = Arc::<[u8]>::from(self.build_validator_buf()?);
        let mut cache = self
            .validator_buf
            .write()
            .map_err(|_| Error::InvalidFile("validator cache lock poisoned".into()))?;
        if let Some(entry) = cache.as_ref()
            && let Some(bytes) = entry.valid_bytes(generation)
        {
            return Ok(bytes);
        }
        *cache = Some(CacheEntry::new(generation, Arc::clone(&data)));
        Ok(data)
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
    fn build_validator_buf(&mut self) -> Result<Vec<u8>> {
        // Parse header to find region offsets
        let header_buf = self.header_buf_arc()?;
        let Ok(header) = Header::new(&header_buf) else {
            return Ok(header_buf.to_vec());
        };
        let Ok(current) = header.header(0) else {
            return Ok(header_buf.to_vec());
        };
        let Ok(rt) = header.region_table(0) else {
            return Ok(header_buf.to_vec());
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
        let header_len = header_buf.len().min(MIB as usize);
        buf[..header_len].copy_from_slice(&header_buf[..header_len]);

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
        let regions: Vec<_> = rt
            .entries()
            .map(|entry| {
                (
                    entry.guid(),
                    usize::try_from(entry.file_offset()).unwrap(),
                    usize::try_from(entry.length()).unwrap(),
                )
            })
            .collect();

        for (guid, offset, length) in regions {
            let region_data: Vec<u8> = if guid == BAT_REGION_GUID {
                self.bat_buf()
                    .map(|bytes| bytes.to_vec())
                    .unwrap_or_default()
            } else if guid == METADATA_REGION_GUID {
                self.metadata_buf()
                    .map(|bytes| bytes.to_vec())
                    .unwrap_or_default()
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

        Ok(buf)
    }

    // -- Region readers (private helpers) ------------------------------------

    /// Read the BAT region from the file using the region table.
    fn read_bat_region(&self) -> Result<Vec<u8>> {
        let header_buf = self.header_buf_arc()?;
        let header = Header::new(&header_buf)?;
        let rt = header.region_table(0)?;
        for entry in rt.entries() {
            if entry.guid() == BAT_REGION_GUID {
                let offset = entry.file_offset();
                let length = entry.length() as usize;
                let mut buf = vec![0u8; length];
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| Error::InvalidFile("medium inner lock poisoned".into()))?;
                read_exact_at(&mut *inner, offset, &mut buf)?;
                return Ok(buf);
            }
        }
        Err(Error::InvalidFile(
            "BAT region not found in region table".into(),
        ))
    }

    /// Read the Metadata region from the file using the region table.
    fn read_metadata_region(&self) -> Result<Vec<u8>> {
        let header_buf = self.header_buf_arc()?;
        let header = Header::new(&header_buf)?;
        let rt = header.region_table(0)?;
        for entry in rt.entries() {
            if entry.guid() == METADATA_REGION_GUID {
                let offset = entry.file_offset();
                let length = entry.length() as usize;
                let mut buf = vec![0u8; length];
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| Error::InvalidFile("medium inner lock poisoned".into()))?;
                read_exact_at(&mut *inner, offset, &mut buf)?;
                return Ok(buf);
            }
        }
        Err(Error::InvalidFile(
            "Metadata region not found in region table".into(),
        ))
    }

    /// Read the Log region from the file using header-specified offset/length.
    fn read_log_region(&self) -> Result<Vec<u8>> {
        let header_buf = self.header_buf_arc()?;
        let header = Header::new(&header_buf)?;
        let h = header.header(0)?;
        let offset = h.log_offset();
        let length = h.log_length() as usize;
        let mut buf = vec![0u8; length];
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::InvalidFile("medium inner lock poisoned".into()))?;
        read_exact_at(&mut *inner, offset, &mut buf)?;
        Ok(buf)
    }

    /// If a replay overlay exists, apply it to the given region buffer.
    fn apply_replay_overlay(&self, region_data: &mut [u8], region_offset: u64) {
        if let Some(ref overlay) = self.replay_overlay {
            overlay.apply_to_region(region_data, region_offset);
        }
    }
}
