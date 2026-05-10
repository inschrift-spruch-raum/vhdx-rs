//! Core file types: [`File`], [`OpenOptions`], [`CreateOptions`], and opening policies.

use bitvec::prelude::*;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::common::crc32c;
use crate::error::{Error, Result, SignaturePosition};
use crate::header::Header;
use crate::log::Log;
use crate::log_replay::{self, ReplayOverlay};
use crate::sections::Sections;
use crate::types::{self, Guid};

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;
const TB: u64 = GB * 1024; // 1024^4 = 1 TiB

const HEADER1_OFFSET: u64 = 64 * KB;
const HEADER2_OFFSET: u64 = 128 * KB;
const REGION_TABLE1_OFFSET: u64 = 192 * KB;
const REGION_TABLE2_OFFSET: u64 = 256 * KB;

/// Log starts at 1 MB (first MB-aligned slot after the header section).
const LOG_OFFSET: u64 = MB;
/// Minimum log length is 1 MB.
const LOG_LENGTH: u32 = 1024 * 1024;

/// BAT region starts at 2 MB (right after the log).
const BAT_REGION_OFFSET: u64 = 2 * MB;
/// Metadata region default size: 1 MB.
const METADATA_REGION_SIZE: u32 = 1024 * 1024;

const HEADER_SIZE: usize = 4096;
const REGION_TABLE_SIZE: usize = 64 * 1024;

/// Metadata table fixed size: 64 KB.
const METADATA_TABLE_SIZE: usize = 64 * 1024;
/// Table header size: 32 bytes.
const TABLE_HEADER_SIZE: usize = 32;
/// Table entry size: 32 bytes.
const TABLE_ENTRY_SIZE: usize = 32;
/// Locator header size: 20 bytes.
const LOCATOR_HEADER_SIZE: usize = 20;
/// Key-value entry size: 12 bytes.
const KV_ENTRY_SIZE: usize = 12;

struct MetadataEntryMeta {
    guid: Guid,
    rel_offset: u32,
    length: u32,
    flags: u32,
}

// Signatures are written as byte literals (b"head", b"regi", etc.) to avoid
// endianness issues — they are byte-strings, not numeric values.

/// VHDX file signature bytes: "vhdxfile" (8 bytes).
const VHDX_SIGNATURE_BYTES: [u8; 8] = [0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6C, 0x65];

/// Size of the header buffer (first 1 MB of the file).
const HEADER_BUFFER_SIZE: usize = 1024 * 1024;

// Known region GUIDs (mixed-endian on-disk byte order)
//
// BAT:   2DC27766-F623-4200-9D64-115E9BFD4A08
// Metadata: 8B7CA206-4790-4B9A-B8FE-575F050F886E

const BAT_REGION_GUID: Guid = Guid::from_bytes([
    0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
]);

const METADATA_REGION_GUID: Guid = Guid::from_bytes([
    0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
]);

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check whether a GUID corresponds to a known region type.
fn is_known_region_guid(guid: &Guid) -> bool {
    const KNOWN: &[Guid] = &[BAT_REGION_GUID, METADATA_REGION_GUID];
    KNOWN.contains(guid)
}

/// Check whether a GUID corresponds to a known metadata item type.
fn is_known_metadata_guid(guid: &Guid) -> bool {
    const KNOWN: &[Guid] = &[
        types::StandardItems::FILE_PARAMETERS,
        types::StandardItems::VIRTUAL_DISK_SIZE,
        types::StandardItems::VIRTUAL_DISK_ID,
        types::StandardItems::LOGICAL_SECTOR_SIZE,
        types::StandardItems::PHYSICAL_SECTOR_SIZE,
        types::StandardItems::PARENT_LOCATOR,
    ];
    KNOWN.contains(guid)
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

/// An opened VHDX file.
///
/// Obtain via [`File::open`] followed by [`OpenOptions::finish`], or via
/// [`File::create`] followed by [`CreateOptions::finish`].
pub struct File {
    inner: std::fs::File,
    path: PathBuf,
    /// First 1 MB of the file, buffered for header section parsing.
    header_buf: Vec<u8>,
    /// Cached BAT region data (lazy-loaded by [`File::bat_buf`]).
    bat_buf: OnceLock<Vec<u8>>,
    /// Cached metadata region data (lazy-loaded by [`File::metadata_buf`]).
    metadata_buf: OnceLock<Vec<u8>>,
    /// Cached log region data (lazy-loaded by [`File::log_buf`]).
    log_buf: OnceLock<Vec<u8>>,
    /// Whether the file was opened with write access.
    write: bool,
    /// Strict validation mode.
    strict: bool,
    /// Configured log replay policy.
    log_replay_policy: LogReplayPolicy,
    /// In-memory replay overlay (for `InMemoryOnReadOnly` / Auto read-only).
    replay_overlay: Option<Arc<ReplayOverlay>>,
    /// Cached validator buffer: assembled region data at correct file offsets.
    validator_buf: OnceLock<Vec<u8>>,
    /// Cached `Sections` container, enabling zero-copy `&Sections<'_>` return.
    sections_cache: OnceLock<Sections<'static>>,
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

    /// Return a cached `Sections` container for this file.
    ///
    /// The container is lazily initialized on first access and cached for the
    /// lifetime of the `File`, providing a zero-copy view of all sections.
    pub fn sections(&self) -> &Sections<'_> {
        self.sections_cache.get_or_init(|| {
            // SAFETY: `Sections` is stored inside `File` via `OnceLock`, so the
            // `'static` lifetime is a contained fiction. The returned `&Sections<'_>`
            // borrows from `&self`, ensuring it never outlives the `File`.
            unsafe { std::mem::transmute::<Sections<'_>, Sections<'static>>(Sections::new(self)) }
        })
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
    /// # Panics
    ///
    /// Panics if the `OnceLock::set` operation fails (should not occur in
    /// single-threaded usage).
    pub(crate) fn bat_buf(&self) -> Result<&[u8]> {
        if let Some(buf) = self.bat_buf.get() {
            return Ok(&buf[..]);
        }
        let mut data = self.read_bat_region()?;
        // Apply replay overlay if present
        if self.replay_overlay.is_some() {
            let header = Header::new(&self.header_buf)?;
            let rt = header.region_table(0)?;
            for entry in rt.entries() {
                if entry.guid() == BAT_REGION_GUID {
                    self.apply_replay_overlay(&mut data, entry.file_offset());
                    break;
                }
            }
        }
        self.bat_buf.set(data).unwrap_or_else(|_| {
            // Safe: single-threaded, we checked get() above
            unreachable!("bat_buf already initialized")
        });
        Ok(self.bat_buf.get().unwrap().as_slice())
    }

    /// Lazy-load the Metadata region data from disk.
    ///
    /// Reads the metadata region using the offset and length stored in the
    /// header's region table. Subsequent calls return the cached buffer.
    ///
    /// # Panics
    ///
    /// Panics if the `OnceLock::set` operation fails (should not occur in
    /// single-threaded usage).
    pub(crate) fn metadata_buf(&self) -> Result<&[u8]> {
        if let Some(buf) = self.metadata_buf.get() {
            return Ok(&buf[..]);
        }
        let mut data = self.read_metadata_region()?;
        // Apply replay overlay if present
        if self.replay_overlay.is_some() {
            let header = Header::new(&self.header_buf)?;
            let rt = header.region_table(0)?;
            for entry in rt.entries() {
                if entry.guid() == METADATA_REGION_GUID {
                    self.apply_replay_overlay(&mut data, entry.file_offset());
                    break;
                }
            }
        }
        self.metadata_buf
            .set(data)
            .unwrap_or_else(|_| unreachable!("metadata_buf already initialized"));
        Ok(self.metadata_buf.get().unwrap().as_slice())
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

        let one_mb: usize = 1024 * 1024;
        let log_offset = usize::try_from(current.log_offset()).unwrap();
        let log_length = usize::try_from(current.log_length()).unwrap();
        let header_log_guid = current.log_guid();

        // Determine maximum extent across all regions
        let mut max_end = one_mb.max(log_offset + log_length);
        for entry in rt.entries() {
            let end = usize::try_from(entry.file_offset()).unwrap()
                + usize::try_from(entry.length()).unwrap();
            max_end = max_end.max(end);
        }

        let mut buf = vec![0u8; max_end];

        // Copy header at offset 0
        let header_len = self.header_buf.len().min(one_mb);
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
                self.bat_buf().unwrap_or(&[])
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

// ---------------------------------------------------------------------------
// OpenOptions
// ---------------------------------------------------------------------------

/// Builder for configuring how an existing VHDX file is opened.
///
/// Obtain via [`File::open`]. The default configuration is:
/// - read-only (no write access)
/// - strict validation enabled
/// - log replay policy: [`LogReplayPolicy::Require`]
///
/// # Standard
///
/// docs/Standard/MS-VHDX-只读扩展标准.md §3/§4
pub struct OpenOptions {
    path: PathBuf,
    write: bool,
    strict: bool,
    log_replay_policy: LogReplayPolicy,
}

impl OpenOptions {
    fn validate_policy_compatibility(&self) -> Result<()> {
        match self.log_replay_policy {
            LogReplayPolicy::InMemoryOnReadOnly | LogReplayPolicy::ReadOnlyNoReplay
                if self.write =>
            {
                Err(Error::InvalidParameter(
                    "log replay policy incompatible with write access".into(),
                ))
            }
            _ => Ok(()),
        }
    }

    fn open_file_and_read_header(&self) -> Result<(std::fs::File, Vec<u8>)> {
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true);
        if self.write {
            opts.write(true);
        }
        let mut file = opts.open(&self.path)?;
        let mut header_buf = vec![0u8; HEADER_BUFFER_SIZE];
        let bytes_read = file.read(&mut header_buf)?;
        if bytes_read < VHDX_SIGNATURE_BYTES.len() {
            return Err(Error::InvalidFile(
                "file too small to contain VHDX signature".into(),
            ));
        }
        header_buf.truncate(bytes_read);
        Ok((file, header_buf))
    }

    fn validate_file_signature(header_buf: &[u8]) -> Result<()> {
        let sig = &header_buf[..VHDX_SIGNATURE_BYTES.len()];
        if sig == VHDX_SIGNATURE_BYTES {
            return Ok(());
        }
        let mut actual_bytes = [0u8; 8];
        actual_bytes.copy_from_slice(sig);
        Err(Error::InvalidSignature {
            position: SignaturePosition::FileTypeIdentifier,
            expected: VHDX_SIGNATURE_BYTES,
            found: actual_bytes,
        })
    }

    fn validate_current_header(current: &crate::header::HeaderStructure<'_>) -> Result<()> {
        if current.version() != 1 {
            return Err(Error::UnsupportedVersion {
                version: current.version(),
            });
        }
        if current.log_version() != 0 && current.log_guid() != Guid::zero() {
            return Err(Error::UnsupportedLogVersion {
                version: current.log_version(),
            });
        }
        Ok(())
    }

    fn validate_region_table_and_metadata(
        &self, file: &std::fs::File, header: &Header,
    ) -> Result<()> {
        let rt = header.region_table(0)?;
        Self::validate_region_table_entries(&rt, self.strict)?;
        Self::validate_unknown_metadata(file, &rt, self.strict)
    }

    fn validate_region_table_entries(
        rt: &crate::header::RegionTable<'_>, strict: bool,
    ) -> Result<()> {
        let mb: u64 = 1024 * 1024;
        let entries: Vec<_> = rt.entries().collect();
        for (i, entry) in entries.iter().enumerate() {
            let file_offset = entry.file_offset();
            let length = entry.length();
            if file_offset % mb != 0 {
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_ALIGNMENT: entry {i} file_offset {file_offset:#x} not 1MB-aligned"
                )));
            }
            if file_offset < mb {
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_OFFSET_MINIMUM: entry {i} file_offset {file_offset} < 1MB minimum"
                )));
            }
            if u64::from(length) % mb != 0 {
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_ALIGNMENT: entry {i} length {length} not 1MB-aligned"
                )));
            }
            let end = file_offset + u64::from(length);
            for (j, prev) in entries[..i].iter().enumerate() {
                let prev_end = prev.file_offset() + u64::from(prev.length());
                if file_offset < prev_end && prev.file_offset() < end {
                    return Err(Error::InvalidRegionTable(format!(
                        "REGION_ENTRY_OVERLAP: entries {j} and {i} overlap"
                    )));
                }
            }
            if !is_known_region_guid(&entry.guid()) {
                if entry.required() {
                    return Err(Error::RegionRequiredUnknown { guid: entry.guid() });
                }
                if strict {
                    return Err(Error::RegionOptionalUnknown { guid: entry.guid() });
                }
            }
        }
        Ok(())
    }

    fn validate_unknown_metadata(
        file: &std::fs::File, rt: &crate::header::RegionTable<'_>, strict: bool,
    ) -> Result<()> {
        for entry in rt.entries() {
            if entry.guid() != METADATA_REGION_GUID {
                continue;
            }
            let mut meta_data = vec![0u8; entry.length() as usize];
            (&*file).seek(SeekFrom::Start(entry.file_offset()))?;
            (&*file).read_exact(&mut meta_data)?;
            let meta = crate::metadata::Metadata::new(&meta_data)?;
            for table_entry in meta.table().entries() {
                if table_entry.flags().is_required()
                    && !is_known_metadata_guid(&table_entry.item_id())
                {
                    return Err(Error::MetadataRequiredUnknown {
                        guid: table_entry.item_id(),
                    });
                }
                if strict
                    && !table_entry.flags().is_required()
                    && !is_known_metadata_guid(&table_entry.item_id())
                {
                    return Err(Error::MetadataOptionalUnknown {
                        guid: table_entry.item_id(),
                    });
                }
            }
            break;
        }
        Ok(())
    }

    fn load_log_data(file: &std::fs::File, offset: u64, length: u32) -> Result<Vec<u8>> {
        let mut log_data = vec![0u8; length as usize];
        (&*file).seek(SeekFrom::Start(offset))?;
        (&*file).read_exact(&mut log_data)?;
        Ok(log_data)
    }

    /// # Panics
    ///
    /// Panics if header offset conversions overflow `usize`.
    /// This should not happen with well-formed VHDX files.
    fn apply_writable_header_update(
        &self, file: &mut std::fs::File, header_buf: &mut Vec<u8>,
    ) -> Result<()> {
        if !self.write {
            return Ok(());
        }
        if header_buf.len() < HEADER_BUFFER_SIZE {
            header_buf.resize(HEADER_BUFFER_SIZE, 0);
        }
        let hdr = Header::new(header_buf)?;
        let h1 = hdr.header(1)?;
        let h2 = hdr.header(2)?;
        let current_idx = if h1.sequence_number() > h2.sequence_number() {
            1
        } else {
            2
        };
        let noncurrent_idx = if current_idx == 1 { 2 } else { 1 };
        let noncurrent_offset: u64 = if noncurrent_idx == 1 {
            HEADER1_OFFSET
        } else {
            HEADER2_OFFSET
        };
        let current_header = hdr.header(0)?;
        let updated_header = Self::build_updated_header(&current_header);
        file.seek(SeekFrom::Start(noncurrent_offset))?;
        file.write_all(&updated_header)?;
        file.sync_all()?;
        let start = usize::try_from(noncurrent_offset).unwrap();
        header_buf[start..start + HEADER_SIZE].copy_from_slice(&updated_header);
        Ok(())
    }

    fn build_updated_header(
        current_header: &crate::header::HeaderStructure<'_>,
    ) -> [u8; HEADER_SIZE] {
        let mut updated_header = [0u8; HEADER_SIZE];
        updated_header[..4].copy_from_slice(b"head");
        updated_header[4..8].copy_from_slice(&0u32.to_le_bytes());
        updated_header[8..16]
            .copy_from_slice(&(current_header.sequence_number() + 1).to_le_bytes());
        updated_header[16..32].copy_from_slice(&Guid::new_v4().to_bytes());
        updated_header[32..48].copy_from_slice(&current_header.data_write_guid().to_bytes());
        updated_header[48..64].copy_from_slice(&current_header.log_guid().to_bytes());
        updated_header[64..66].copy_from_slice(&current_header.log_version().to_le_bytes());
        updated_header[66..68].copy_from_slice(&current_header.version().to_le_bytes());
        updated_header[68..72].copy_from_slice(&current_header.log_length().to_le_bytes());
        updated_header[72..80].copy_from_slice(&current_header.log_offset().to_le_bytes());
        let checksum = crc32c(&updated_header);
        updated_header[4..8].copy_from_slice(&checksum.to_le_bytes());
        updated_header
    }

    /// Enable write access (read-write mode).
    ///
    /// # Standard
    ///
    /// docs/Standard/MS-VHDX-只读扩展标准.md
    #[must_use]
    pub fn write(mut self) -> Self {
        self.write = true;
        self
    }

    /// Set strict validation mode.
    ///
    /// When `strict = true` (the default), all validation errors are treated
    /// as hard errors. When `strict = false`, *optional* unknown fields are
    /// tolerated, but *required* unknown fields still cause failure.
    ///
    /// # Standard
    ///
    /// docs/Standard/MS-VHDX-宽松扩展标准.md §3
    #[must_use]
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Set the log replay policy (default: [`LogReplayPolicy::Require`]).
    ///
    /// # Standard
    ///
    /// MS-VHDX §2.3 + MS-VHDX-只读扩展标准.md §3/§4
    #[must_use]
    pub fn log_replay(mut self, policy: LogReplayPolicy) -> Self {
        self.log_replay_policy = policy;
        self
    }

    /// Finish opening the VHDX file.
    ///
    /// Opens the file (read-only or read-write depending on configuration),
    /// reads the first 1 MB into an internal buffer, validates the
    /// "vhdxfile" signature, parses headers, loads the log region, and
    /// applies the configured [`LogReplayPolicy`].
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, the signature is
    /// invalid, headers are malformed, region tables are invalid, metadata
    /// is missing required items, or log replay is required but not allowed.
    ///
    /// # Standard
    ///
    /// - MS-VHDX-只读扩展标准 §4.1: if a replayable log exists and the
    ///   policy is `Require`, `finish()` returns [`Error::LogReplayRequired`].
    /// - MS-VHDX-只读扩展标准 §4.2: `Auto` replays automatically (in-memory
    ///   for read-only, to-file for read-write).
    /// - MS-VHDX-只读扩展标准 §4.3: `InMemoryOnReadOnly` builds an overlay
    ///   without writing to the file (read-only only).
    /// - MS-VHDX-只读扩展标准 §4.4: `ReadOnlyNoReplay` allows structure
    ///   reads only; payload data-plane consistency is not guaranteed.
    ///
    /// # Panics
    ///
    /// Panics if internal offset conversions from validated on-disk structures
    /// overflow target integer sizes.
    pub fn finish(self) -> Result<File> {
        self.validate_policy_compatibility()?;
        let (mut file, mut header_buf) = self.open_file_and_read_header()?;
        Self::validate_file_signature(&header_buf)?;
        let header = Header::new(&header_buf)?;
        let current = header.header(0)?;
        Self::validate_current_header(&current)?;
        let log_offset = current.log_offset();
        let log_length = current.log_length();
        let log_guid = current.log_guid();
        self.validate_region_table_and_metadata(&file, &header)?;
        let log_data = Self::load_log_data(&file, log_offset, log_length)?;

        // -- Apply log replay policy -----------------------------------------
        let replay_overlay = match self.log_replay_policy {
            LogReplayPolicy::Require => {
                let log = Log::new(&log_data)?;
                if log_replay::has_pending_log(&log, &log_guid) {
                    return Err(Error::LogReplayRequired);
                }
                None
            }
            LogReplayPolicy::Auto => {
                let log = Log::new(&log_data)?;
                if log_replay::has_pending_log(&log, &log_guid) {
                    let active = log_replay::detect_active_sequence(&log, &log_guid)?;
                    // Truncation check (MS-VHDX §2.3.3)
                    let file_size = file.metadata()?.len();
                    if file_size < active.flushed_file_offset() {
                        return Err(Error::CorruptedHeader(format!(
                            "file truncated: size {} < FlushedFileOffset {}",
                            file_size,
                            active.flushed_file_offset()
                        )));
                    }
                    if self.write {
                        log_replay::replay_to_file(&file, &active)?;
                        None
                    } else {
                        Some(Arc::new(log_replay::build_replay_overlay(&active)?))
                    }
                } else {
                    None
                }
            }
            LogReplayPolicy::InMemoryOnReadOnly => {
                // write check already done above
                let log = Log::new(&log_data)?;
                if log_replay::has_pending_log(&log, &log_guid) {
                    let active = log_replay::detect_active_sequence(&log, &log_guid)?;
                    // Truncation check (MS-VHDX §2.3.3)
                    let file_size = file.metadata()?.len();
                    if file_size < active.flushed_file_offset() {
                        return Err(Error::CorruptedHeader(format!(
                            "file truncated: size {} < FlushedFileOffset {}",
                            file_size,
                            active.flushed_file_offset()
                        )));
                    }
                    Some(Arc::new(log_replay::build_replay_overlay(&active)?))
                } else {
                    None
                }
            }
            LogReplayPolicy::ReadOnlyNoReplay => {
                // write check already done above; skip replay entirely
                None
            }
        };

        // -- Patch header_buf with replay overlay data -------------------------
        // header_buf was loaded earlier (before overlay was built).
        // Per MS-VHDX-只读扩展标准 §4.3.4, structure reads must be based
        // on the post-replay view. Apply overlay patches now.
        if let Some(ref overlay) = replay_overlay {
            // Ensure header_buf covers the full 1 MB header section
            if header_buf.len() < HEADER_BUFFER_SIZE {
                header_buf.resize(HEADER_BUFFER_SIZE, 0);
            }
            overlay.apply_to_region(&mut header_buf, 0);
        }

        // -- Cache log buffer ------------------------------------------------
        let log_buf = OnceLock::new();
        log_buf.set(log_data).unwrap_or_else(|_| unreachable!());

        self.apply_writable_header_update(&mut file, &mut header_buf)?;

        Ok(File {
            inner: file,
            path: self.path,
            header_buf,
            bat_buf: OnceLock::new(),
            metadata_buf: OnceLock::new(),
            log_buf,
            write: self.write,
            strict: self.strict,
            log_replay_policy: self.log_replay_policy,
            replay_overlay,
            validator_buf: OnceLock::new(),
            sections_cache: OnceLock::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

/// Log replay policy controlling how pending logs are handled on open.
///
/// # Standard
///
/// MS-VHDX §2.3 + MS-VHDX-只读扩展标准 §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogReplayPolicy {
    /// If a replayable log exists, `finish()` returns
    /// [`Error::LogReplayRequired`]. No implicit replay.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.1
    #[default]
    Require,

    /// Automatically replay the log during `finish()`.
    /// On read-only open, replay is done in memory only.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.2
    Auto,

    /// In-memory replay is allowed for read-only opens.
    /// Not valid for read-write opens.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.3
    InMemoryOnReadOnly,

    /// Open read-only without replaying the log.
    /// Only structure-level reads are guaranteed consistent;
    /// payload data-plane reads may be inconsistent.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.4
    ReadOnlyNoReplay,
}

/// BAT read semantics policy.
///
/// Controls whether effective data or raw data is preferred when resolving
/// block reads. For differencing disks, child data is always preferred
/// regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadSemanticsPolicy {
    /// Prefer effective (possibly parent-assembled) data.
    #[default]
    EffectiveDataPreferred,
    /// Prefer raw on-disk data.
    RawDataPreferred,
}

/// Result of parent chain validation for differencing disks.
#[cfg(test)]
pub(crate) struct ParentChainInfo {
    pub(crate) _child_path: PathBuf,
    pub(crate) _parent_path: PathBuf,
    pub(crate) _linkage_matched: bool,
}

// ---------------------------------------------------------------------------
// CreateOptions
// ---------------------------------------------------------------------------

/// Builder for creating a new VHDX file.
pub struct CreateOptions {
    path: PathBuf,
    virtual_size: u64,
    fixed: bool,
    block_size: u32,
    logical_sector_size: u32,
    physical_sector_size: u32,
    parent_path: Option<PathBuf>,
}

impl CreateOptions {
    // -- Builder methods ----------------------------------------------------

    /// Set the virtual disk size in bytes (required).
    ///
    /// Must be a multiple of `logical_sector_size` and at most 64 TB.
    #[must_use]
    pub fn size(mut self, virtual_size: u64) -> Self {
        self.virtual_size = virtual_size;
        self
    }

    /// Set whether this is a fixed-size disk (default: dynamic).
    #[must_use]
    pub fn fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    /// Set the payload block size in bytes (default: 32 MB).
    ///
    /// Must be in `[1 MB, 256 MB]` and a power of two.
    #[must_use]
    pub fn block_size(mut self, size: u32) -> Self {
        self.block_size = size;
        self
    }

    /// Set the logical sector size in bytes (default: 4096).
    ///
    /// Must be 512 or 4096.
    #[must_use]
    pub fn logical_sector_size(mut self, size: u32) -> Self {
        self.logical_sector_size = size;
        self
    }

    /// Set the physical sector size in bytes (default: 4096).
    ///
    /// Must be 512 or 4096.
    #[must_use]
    pub fn physical_sector_size(mut self, size: u32) -> Self {
        self.physical_sector_size = size;
        self
    }

    /// Set the parent disk path (creates a differencing disk).
    #[must_use]
    pub fn parent_path(mut self, path: impl AsRef<Path>) -> Self {
        self.parent_path = Some(path.as_ref().to_path_buf());
        self
    }

    // -- Validation ---------------------------------------------------------

    fn validate(&self) -> Result<()> {
        if self.virtual_size == 0 {
            return Err(Error::InvalidParameter(
                "virtual disk size must be set".into(),
            ));
        }

        if self.virtual_size > 64 * TB {
            return Err(Error::InvalidParameter(
                "virtual disk size must not exceed 64 TB".into(),
            ));
        }

        if !self
            .virtual_size
            .is_multiple_of(u64::from(self.logical_sector_size))
        {
            return Err(Error::InvalidParameter(
                "virtual disk size must be a multiple of logical sector size".into(),
            ));
        }

        let one_mb: u32 = 1024 * 1024;
        let two_fifty_six_mb: u32 = 256 * 1024 * 1024;
        if self.block_size < one_mb || self.block_size > two_fifty_six_mb {
            return Err(Error::InvalidParameter(
                "block size must be between 1 MB and 256 MB".into(),
            ));
        }
        if !self.block_size.is_power_of_two() {
            return Err(Error::InvalidParameter(
                "block size must be a power of 2".into(),
            ));
        }

        if !matches!(self.logical_sector_size, 512 | 4096) {
            return Err(Error::InvalidParameter(
                "logical sector size must be 512 or 4096".into(),
            ));
        }

        if !matches!(self.physical_sector_size, 512 | 4096) {
            return Err(Error::InvalidParameter(
                "physical sector size must be 512 or 4096".into(),
            ));
        }

        if self.fixed && self.parent_path.is_some() {
            return Err(Error::InvalidParameter(
                "fixed disk cannot have a parent".into(),
            ));
        }

        Ok(())
    }

    // -- Finalisation -------------------------------------------------------

    /// Create the VHDX file on disk.
    ///
    /// Writes the File Type Identifier, both Headers, both Region Tables,
    /// the BAT region (initialised per disk type), the full Metadata table
    /// and items, and — for fixed disks — pre-allocates and zero-fills all
    /// payload blocks.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails, the file cannot be created,
    /// or any write operation fails.
    pub fn finish(self) -> Result<File> {
        self.validate()?;

        // Must open with read+write access: on Windows, File::create is
        // write-only, which would deny the re-read below.
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.path)?;
        let mut w = BufWriter::new(file);

        let bat_size =
            Self::calculate_bat_size(self.virtual_size, self.block_size, self.logical_sector_size);
        let metadata_offset = BAT_REGION_OFFSET + u64::from(bat_size);

        // 1. File Type Identifier (offset 0)
        Self::write_file_type_identifier(&mut w)?;

        // 2. Headers (offsets 64 KB and 128 KB)
        let file_write_guid = Guid::new_v4();
        let data_write_guid = Guid::new_v4();
        let log_guid = Guid::zero(); // No active log for fresh file (MS-VHDX §2.2.1)

        // Header 1 with sequence number 0
        let header1 = Self::build_header(0, &file_write_guid, &data_write_guid, &log_guid);
        std::io::Seek::seek(&mut w, std::io::SeekFrom::Start(HEADER1_OFFSET))?;
        w.write_all(&header1)?;

        // Header 2 with sequence number 1 (different from header1 to satisfy §2.2.2)
        let header2 = Self::build_header(1, &file_write_guid, &data_write_guid, &log_guid);
        std::io::Seek::seek(&mut w, std::io::SeekFrom::Start(HEADER2_OFFSET))?;
        w.write_all(&header2)?;

        // 3. Region Tables (offsets 192 KB and 256 KB)
        let region = Self::build_region_table(bat_size, metadata_offset);

        std::io::Seek::seek(&mut w, std::io::SeekFrom::Start(REGION_TABLE1_OFFSET))?;
        w.write_all(&region)?;

        std::io::Seek::seek(&mut w, std::io::SeekFrom::Start(REGION_TABLE2_OFFSET))?;
        w.write_all(&region)?;

        // 4. Extend file to cover log + BAT + metadata (zero-filled)
        //    For fixed disks, also pre-allocate all payload blocks.
        //    The first payload offset must be block_size-aligned for the
        //    validator's payload-offset alignment check (MS-VHDX §2.5.1.1).
        let _first_payload_offset_mb = if self.fixed {
            let (num_payload, _num_sb, _total_entries, _chunk_ratio) =
                Self::compute_bat_entry_counts(
                    self.virtual_size,
                    self.block_size,
                    self.logical_sector_size,
                );
            let payload_align = u64::from(self.block_size) / MB;
            let raw_first_mb = (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(MB);
            let first_payload_offset_mb = raw_first_mb.div_ceil(payload_align) * payload_align;
            let total_payload = num_payload * u64::from(self.block_size);
            let end = first_payload_offset_mb * MB + total_payload;
            w.flush()?;
            w.get_ref().set_len(end)?;
            first_payload_offset_mb
        } else {
            let end = metadata_offset + u64::from(METADATA_REGION_SIZE);
            w.flush()?;
            w.get_ref().set_len(end)?;
            0
        };

        // 5. Write BAT entries
        Self::write_bat_entries(
            &mut w,
            bat_size,
            self.virtual_size,
            self.block_size,
            self.logical_sector_size,
            self.fixed,
            metadata_offset,
        )?;

        // Read parent's DataWriteGuid if this is a differencing disk
        let parent_data_write_guid = if let Some(parent_path) = &self.parent_path {
            Some(Self::read_parent_data_write_guid(parent_path)?)
        } else {
            None
        };

        // 6. Write metadata table + items
        self.write_metadata(&mut w, metadata_offset, parent_data_write_guid)?;

        w.flush()?;
        let mut inner = w
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        // Re-read header buffer from the start of the file.
        inner.seek(std::io::SeekFrom::Start(0))?;
        let mut header_buf = vec![0u8; HEADER_BUFFER_SIZE];
        let bytes_read = inner.read(&mut header_buf)?;
        header_buf.truncate(bytes_read);

        Ok(File {
            inner,
            path: self.path,
            header_buf,
            bat_buf: OnceLock::new(),
            metadata_buf: OnceLock::new(),
            log_buf: OnceLock::new(),
            write: true,
            strict: true,
            log_replay_policy: LogReplayPolicy::Require,
            replay_overlay: None,
            validator_buf: OnceLock::new(),
            sections_cache: OnceLock::new(),
        })
    }

    // -- Internal helpers ---------------------------------------------------

    pub(crate) fn calculate_bat_size(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> u32 {
        let (_num_payload, _num_sb, total_entries, _chunk_ratio) =
            Self::compute_bat_entry_counts(virtual_size, block_size, logical_sector_size);
        let bat_bytes = total_entries * 8;
        let bat_mb = std::cmp::max(bat_bytes.div_ceil(MB), 1);
        u32::try_from(bat_mb).unwrap() * (1024 * 1024)
    }

    fn write_file_type_identifier(w: &mut (impl Write + ?Sized)) -> Result<()> {
        w.write_all(&VHDX_SIGNATURE_BYTES)?;

        let mut creator = [0u8; 512];
        let ident = "vhdx-rs\0";
        for (i, ch) in ident.encode_utf16().enumerate() {
            let off = i * 2;
            if off + 1 < 512 {
                creator[off..off + 2].copy_from_slice(&ch.to_le_bytes());
            }
        }
        w.write_all(&creator)?;
        Ok(())
    }

    fn build_header(
        sequence_number: u64, file_write_guid: &Guid, data_write_guid: &Guid, log_guid: &Guid,
    ) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[..4].copy_from_slice(b"head");
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());
        buf[8..16].copy_from_slice(&sequence_number.to_le_bytes());
        buf[16..32].copy_from_slice(&file_write_guid.to_bytes());
        buf[32..48].copy_from_slice(&data_write_guid.to_bytes());
        buf[48..64].copy_from_slice(&log_guid.to_bytes());
        buf[64..66].copy_from_slice(&0u16.to_le_bytes());
        buf[66..68].copy_from_slice(&1u16.to_le_bytes());
        buf[68..72].copy_from_slice(&LOG_LENGTH.to_le_bytes());
        buf[72..80].copy_from_slice(&LOG_OFFSET.to_le_bytes());

        let checksum = crc32c(&buf);
        buf[4..8].copy_from_slice(&checksum.to_le_bytes());

        buf
    }

    fn build_region_table(bat_size: u32, metadata_offset: u64) -> Vec<u8> {
        let mut buf = vec![0u8; REGION_TABLE_SIZE];
        buf[..4].copy_from_slice(b"regi");
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());
        buf[8..12].copy_from_slice(&2u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0u32.to_le_bytes());

        buf[16..32].copy_from_slice(&BAT_REGION_GUID.to_bytes());
        buf[32..40].copy_from_slice(&BAT_REGION_OFFSET.to_le_bytes());
        buf[40..44].copy_from_slice(&bat_size.to_le_bytes());
        buf[44..48].view_bits_mut::<Lsb0>().set(0, true); // Required

        buf[48..64].copy_from_slice(&METADATA_REGION_GUID.to_bytes());
        buf[64..72].copy_from_slice(&metadata_offset.to_le_bytes());
        buf[72..76].copy_from_slice(&METADATA_REGION_SIZE.to_le_bytes());
        buf[76..80].view_bits_mut::<Lsb0>().set(0, true); // Required

        let checksum = crc32c(&buf);
        buf[4..8].copy_from_slice(&checksum.to_le_bytes());

        buf
    }

    /// Compute BAT entry counts and chunk ratio.
    ///
    /// Returns `(num_payload_blocks, num_sector_bitmap_blocks, total_entries, chunk_ratio)`.
    pub(crate) fn compute_bat_entry_counts(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> (u64, u64, u64, u64) {
        let num_payload = virtual_size.div_ceil(u64::from(block_size));
        let chunk_ratio = (1u64 << 23) * u64::from(logical_sector_size) / u64::from(block_size);
        let num_sb = num_payload.div_ceil(chunk_ratio);
        let total = num_payload + num_sb;
        (num_payload, num_sb, total, chunk_ratio)
    }

    /// Write BAT entries at [`BAT_REGION_OFFSET`].
    ///
    /// - Dynamic disk: all entries = 0 (`PAYLOAD_BLOCK_NOT_PRESENT`).
    /// - Fixed disk: payload entries = `FullyPresent` with sequential
    ///   `FileOffsetMB`; sector-bitmap entries = 0.
    fn write_bat_entries(
        w: &mut (impl Write + std::io::Seek), bat_size: u32, virtual_size: u64, block_size: u32,
        logical_sector_size: u32, fixed: bool, metadata_offset: u64,
    ) -> Result<()> {
        let (_num_payload, num_sb, total_entries, chunk_ratio) =
            Self::compute_bat_entry_counts(virtual_size, block_size, logical_sector_size);

        if !fixed {
            // Dynamic disk: BAT region is already zero-filled by set_len.
            // Still ensure the minimum BAT region exists by seeking to its end.
            std::io::Seek::seek(
                w,
                std::io::SeekFrom::Start(BAT_REGION_OFFSET + u64::from(bat_size)),
            )?;
            return Ok(());
        }

        // Fixed disk: write interleaved payload + sector-bitmap entries.
        // Align first payload to block_size boundary so that the validator's
        // payload-offset alignment check passes (MS-VHDX §2.5.1.1).
        let payload_align = u64::from(block_size) / MB;
        let raw_first_payload_mb = (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(MB);
        let first_payload_offset_mb = raw_first_payload_mb.div_ceil(payload_align) * payload_align;

        std::io::Seek::seek(w, std::io::SeekFrom::Start(BAT_REGION_OFFSET))?;

        let mut sb_written: u64 = 0;
        for i in 0..total_entries {
            // Determine if this entry is a sector bitmap based on how many
            // payload entries have been written since the last SB entry.
            // SB entries appear after every chunk_ratio payload entries.
            let payloads_written = i - sb_written;
            let is_sb = payloads_written > 0
                && payloads_written.is_multiple_of(chunk_ratio)
                && sb_written < num_sb;
            if is_sb {
                // Sector bitmap entry: NotPresent
                w.write_all(&0u64.to_le_bytes())?;
                sb_written += 1;
            } else {
                // Payload entry: FullyPresent at sequential offset
                let payload_idx = payloads_written;
                let offset_mb = first_payload_offset_mb + payload_idx * u64::from(block_size) / MB;
                let mut raw_bytes = [0u8; 8];
                let bits = raw_bytes.view_bits_mut::<Lsb0>();
                bits[0..3].store::<u8>(6u8); // FullyPresent
                bits[20..64].store::<u64>(offset_mb);
                w.write_all(&raw_bytes)?;
            }
        }

        Ok(())
    }

    /// Write the full metadata table (64 KB header + entries) followed by all
    /// required metadata items at `metadata_offset`.
    fn write_metadata(
        &self, w: &mut (impl Write + std::io::Seek), metadata_offset: u64,
        parent_data_write_guid: Option<Guid>,
    ) -> Result<()> {
        let has_parent = self.parent_path.is_some();
        let (items_buf, item_metas) =
            self.build_metadata_items(has_parent, parent_data_write_guid)?;
        let table = Self::build_metadata_table(if has_parent { 6 } else { 5 }, &item_metas);
        std::io::Seek::seek(w, std::io::SeekFrom::Start(metadata_offset))?;
        w.write_all(&table)?;
        w.write_all(&items_buf)?;
        Ok(())
    }

    fn rel_metadata_offset(items_buf: &[u8]) -> Result<u32> {
        let base = u32::try_from(METADATA_TABLE_SIZE).expect("METADATA_TABLE_SIZE must fit u32");
        let rel = u32::try_from(items_buf.len())
            .map_err(|_| Error::InvalidParameter("metadata items buffer too large".into()))?;
        base.checked_add(rel)
            .ok_or_else(|| Error::InvalidParameter("metadata relative offset overflow".into()))
    }

    fn metadata_flags(is_virtual_disk: bool, is_required: bool) -> u32 {
        let mut buf = [0u8; 4];
        let bits = buf.view_bits_mut::<Lsb0>();
        bits.set(1, is_virtual_disk);
        bits.set(2, is_required);
        u32::from_le_bytes(buf)
    }

    fn build_metadata_items(
        &self, has_parent: bool, parent_data_write_guid: Option<Guid>,
    ) -> Result<(Vec<u8>, Vec<MetadataEntryMeta>)> {
        let virtual_disk_id = Guid::new_v4();
        let mut items_buf = Vec::new();
        let mut item_metas = Vec::with_capacity(if has_parent { 6 } else { 5 });
        self.push_file_parameters_item(&mut items_buf, &mut item_metas, has_parent)?;
        Self::push_simple_item(
            &mut items_buf,
            &mut item_metas,
            types::StandardItems::VIRTUAL_DISK_SIZE,
            &self.virtual_size.to_le_bytes(),
            true,
        )?;
        Self::push_simple_item(
            &mut items_buf,
            &mut item_metas,
            types::StandardItems::VIRTUAL_DISK_ID,
            &virtual_disk_id.to_bytes(),
            true,
        )?;
        Self::push_simple_item(
            &mut items_buf,
            &mut item_metas,
            types::StandardItems::LOGICAL_SECTOR_SIZE,
            &self.logical_sector_size.to_le_bytes(),
            true,
        )?;
        Self::push_simple_item(
            &mut items_buf,
            &mut item_metas,
            types::StandardItems::PHYSICAL_SECTOR_SIZE,
            &self.physical_sector_size.to_le_bytes(),
            true,
        )?;
        if has_parent {
            self.push_parent_locator_item(
                &mut items_buf,
                &mut item_metas,
                parent_data_write_guid
                    .expect("parent_data_write_guid must be set when has_parent is true"),
            )?;
        }
        Ok((items_buf, item_metas))
    }

    fn push_file_parameters_item(
        &self, items_buf: &mut Vec<u8>, metas: &mut Vec<MetadataEntryMeta>, has_parent: bool,
    ) -> Result<()> {
        let rel = Self::rel_metadata_offset(items_buf)?;
        let mut fp_buf = [0u8; 8];
        let fp_bits = fp_buf.view_bits_mut::<Lsb0>();
        fp_bits[0..32].store_le::<u32>(self.block_size);
        fp_bits.set(32, self.fixed);
        fp_bits.set(33, has_parent);
        items_buf.extend_from_slice(&fp_buf);
        metas.push(MetadataEntryMeta {
            guid: types::StandardItems::FILE_PARAMETERS,
            rel_offset: rel,
            length: 8,
            flags: Self::metadata_flags(false, true),
        });
        Ok(())
    }

    fn push_simple_item(
        items_buf: &mut Vec<u8>, metas: &mut Vec<MetadataEntryMeta>, guid: Guid, bytes: &[u8],
        is_virtual_disk: bool,
    ) -> Result<()> {
        let rel = Self::rel_metadata_offset(items_buf)?;
        items_buf.extend_from_slice(bytes);
        metas.push(MetadataEntryMeta {
            guid,
            rel_offset: rel,
            length: u32::try_from(bytes.len()).expect("metadata item length fits u32"),
            flags: Self::metadata_flags(is_virtual_disk, true),
        });
        Ok(())
    }

    fn push_parent_locator_item(
        &self, items_buf: &mut Vec<u8>, metas: &mut Vec<MetadataEntryMeta>, parent_guid: Guid,
    ) -> Result<()> {
        let rel = Self::rel_metadata_offset(items_buf)?;
        let pl_data = self.build_parent_locator(parent_guid);
        let pl_length = u32::try_from(pl_data.len())
            .map_err(|_| Error::InvalidParameter("parent locator metadata too large".into()))?;
        items_buf.extend_from_slice(&pl_data);
        metas.push(MetadataEntryMeta {
            guid: types::StandardItems::PARENT_LOCATOR,
            rel_offset: rel,
            length: pl_length,
            flags: Self::metadata_flags(false, true),
        });
        Ok(())
    }

    fn build_metadata_table(entry_count: u16, item_metas: &[MetadataEntryMeta]) -> Vec<u8> {
        let mut table = vec![0u8; METADATA_TABLE_SIZE];
        table[0..8].copy_from_slice(b"metadata");
        table[10..12].copy_from_slice(&entry_count.to_le_bytes());
        let mut entry_off = TABLE_HEADER_SIZE;
        for meta in item_metas {
            table[entry_off..entry_off + 16].copy_from_slice(&meta.guid.to_bytes());
            table[entry_off + 16..entry_off + 20].copy_from_slice(&meta.rel_offset.to_le_bytes());
            table[entry_off + 20..entry_off + 24].copy_from_slice(&meta.length.to_le_bytes());
            table[entry_off + 24..entry_off + 28].copy_from_slice(&meta.flags.to_le_bytes());
            entry_off += TABLE_ENTRY_SIZE;
        }
        table
    }

    /// Build the parent locator metadata item for differencing disks.
    ///
    /// Layout: 20-byte header + 2×12-byte KV entries + UTF-16LE key/value data.
    ///
    /// `parent_data_write_guid` is the `DataWriteGuid` read from the parent file's
    /// header. Per MS-VHDX §2.6.2.6.3, the `parent_linkage` value MUST be the
    /// parent's `DataWriteGuid`, formatted as a lowercase GUID string with braces.
    ///
    /// # Panics
    ///
    /// Panics if any of the 8 `u32::try_from` / `u16::try_from` offset or length
    /// conversions overflow. This should not happen with valid UTF-16 key/value
    /// data within reasonable size limits.
    fn build_parent_locator(&self, parent_data_write_guid: Guid) -> Vec<u8> {
        // Format parent_linkage as the parent's DataWriteGuid with braces:
        // "{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}"
        let guid_str = parent_data_write_guid.to_uuid().hyphenated().to_string();
        let parent_linkage_str = format!("{{{guid_str}}}");
        let relative_path = self
            .parent_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let key1 = "parent_linkage";
        let key2 = "relative_path";

        let key1_utf16: Vec<u8> = key1.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let val1_utf16: Vec<u8> = parent_linkage_str
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let key2_utf16: Vec<u8> = key2.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let val2_utf16: Vec<u8> = relative_path
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();

        let kv_data_start = LOCATOR_HEADER_SIZE + 2 * KV_ENTRY_SIZE;

        let key1_off = kv_data_start;
        let val1_off = key1_off + key1_utf16.len();
        let key2_off = val1_off + val1_utf16.len();
        let val2_off = key2_off + key2_utf16.len();

        let total_len = val2_off + val2_utf16.len();
        let mut buf = vec![0u8; total_len];

        // Locator header (20 bytes)
        buf[0..16].copy_from_slice(&types::StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
        // reserved (2 bytes) = 0
        buf[18..20].copy_from_slice(&2u16.to_le_bytes()); // 2 KV entries

        // KV entry 0: parent_linkage
        let kv0_off = LOCATOR_HEADER_SIZE;
        buf[kv0_off..kv0_off + 4].copy_from_slice(&u32::try_from(key1_off).unwrap().to_le_bytes());
        buf[kv0_off + 4..kv0_off + 8]
            .copy_from_slice(&u32::try_from(val1_off).unwrap().to_le_bytes());
        buf[kv0_off + 8..kv0_off + 10]
            .copy_from_slice(&u16::try_from(key1_utf16.len()).unwrap().to_le_bytes());
        buf[kv0_off + 10..kv0_off + 12]
            .copy_from_slice(&u16::try_from(val1_utf16.len()).unwrap().to_le_bytes());

        // KV entry 1: relative_path
        let kv1_off = LOCATOR_HEADER_SIZE + KV_ENTRY_SIZE;
        buf[kv1_off..kv1_off + 4].copy_from_slice(&u32::try_from(key2_off).unwrap().to_le_bytes());
        buf[kv1_off + 4..kv1_off + 8]
            .copy_from_slice(&u32::try_from(val2_off).unwrap().to_le_bytes());
        buf[kv1_off + 8..kv1_off + 10]
            .copy_from_slice(&u16::try_from(key2_utf16.len()).unwrap().to_le_bytes());
        buf[kv1_off + 10..kv1_off + 12]
            .copy_from_slice(&u16::try_from(val2_utf16.len()).unwrap().to_le_bytes());

        // Key/value data
        buf[key1_off..key1_off + key1_utf16.len()].copy_from_slice(&key1_utf16);
        buf[val1_off..val1_off + val1_utf16.len()].copy_from_slice(&val1_utf16);
        buf[key2_off..key2_off + key2_utf16.len()].copy_from_slice(&key2_utf16);
        buf[val2_off..val2_off + val2_utf16.len()].copy_from_slice(&val2_utf16);

        buf
    }

    /// Open the parent file, read its first 1 MB, parse the header, and return
    /// the `DataWriteGuid`. Used during differencing disk creation to populate
    /// the `parent_linkage` field.
    fn read_parent_data_write_guid(parent_path: &std::path::Path) -> Result<Guid> {
        use std::io::Read;

        let mut pf = std::fs::File::open(parent_path).map_err(|_| Error::ParentNotFound)?;

        let mut buf = vec![0u8; HEADER_BUFFER_SIZE];
        let bytes_read = pf.read(&mut buf).map_err(|_| Error::ParentNotFound)?;

        if bytes_read < 8 {
            return Err(Error::ParentNotFound);
        }
        buf.truncate(bytes_read);

        // Validate parent's VHDX signature
        if buf[..8] != VHDX_SIGNATURE_BYTES {
            return Err(Error::ParentNotFound);
        }

        let header = Header::new(&buf).map_err(|_| Error::ParentNotFound)?;

        let current = header.header(0).map_err(|_| Error::ParentNotFound)?;

        Ok(current.data_write_guid())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::EntryFlags;

    /// Create a tempdir under `target/test/`, copy a reference file from misc/,
    /// return (`TempDir`, `PathBuf`). The `TempDir` keeps the copy alive.
    fn ref_to_tmp(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let root = std::path::Path::new("target").join("test");
        let _ = std::fs::create_dir_all(&root);
        let dir = tempfile::Builder::new()
            .prefix("test-")
            .tempdir_in(&root)
            .expect("tempdir");
        let src = format!("misc/{name}");
        let dst = dir.path().join(name);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
        (dir, dst)
    }

    // -- Open tests ---------------------------------------------------------

    #[test]
    fn open_void_vhdx() {
        let (_dir, path) = ref_to_tmp("test-void.vhdx");
        let f = File::open(&path).finish();
        assert!(f.is_ok(), "failed to open test-void.vhdx: {:?}", f.err());
        let f = f.unwrap();
        assert!(!f.is_write());
        assert!(f.is_strict());
        assert_eq!(f.log_replay_policy(), LogReplayPolicy::Require);
    }

    #[test]
    fn open_options_builder_write() {
        let (_dir, path) = ref_to_tmp("test-void.vhdx");
        let f = File::open(&path).write().finish();
        assert!(f.is_ok());
        let f = f.unwrap();
        assert!(f.is_write());
    }

    #[test]
    fn open_options_builder_log_replay() {
        let (_dir, path) = ref_to_tmp("test-void.vhdx");
        let f = File::open(&path).log_replay(LogReplayPolicy::Auto).finish();
        assert!(f.is_ok());
        assert_eq!(f.unwrap().log_replay_policy(), LogReplayPolicy::Auto);
    }

    #[test]
    fn open_options_builder_non_strict() {
        let (_dir, path) = ref_to_tmp("test-void.vhdx");
        let f = File::open(&path).strict(false).finish();
        assert!(f.is_ok());
        assert!(!f.unwrap().is_strict());
    }

    #[test]
    fn open_nonexistent_file() {
        let f = File::open("misc/does-not-exist.vhdx").finish();
        assert!(f.is_err());
    }

    #[test]
    fn open_invalid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.vhdx");
        {
            let mut tmp = std::fs::File::create(&path).unwrap();
            tmp.write_all(b"not a vhdx file at all").unwrap();
        }

        let f = File::open(&path).finish();
        assert!(f.is_err());
        assert!(matches!(f.unwrap_err(), Error::InvalidSignature { .. }));
    }

    #[test]
    fn log_replay_default_is_require() {
        assert_eq!(LogReplayPolicy::default(), LogReplayPolicy::Require);
    }

    #[test]
    fn read_semantics_default() {
        assert_eq!(
            ReadSemanticsPolicy::default(),
            ReadSemanticsPolicy::EffectiveDataPreferred
        );
    }

    // -- Create tests -------------------------------------------------------

    /// Helper: create a VHDX in `target/test-output/` and return the bytes.
    fn create_test_vhdx(size: u64) -> Vec<u8> {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_output = project_root.join("target").join("test-output");
        std::fs::create_dir_all(&test_output).expect("create test-output dir");
        let test_id: u64 = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )
        .unwrap();
        let test_dir = test_output.join(format!("vhdx-test-{test_id}"));
        std::fs::create_dir_all(&test_dir).expect("create test dir");
        let path = test_dir.join("test.vhdx");
        File::create(&path)
            .size(size)
            .finish()
            .expect("create vhdx");
        let mut buf = Vec::new();
        let mut f = std::fs::File::open(&path).expect("reopen");
        f.read_to_end(&mut buf).expect("read");
        // Clean up the test artifacts.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&test_dir);
        buf
    }

    #[test]
    fn create_1gb_default() {
        let data = create_test_vhdx(1024 * 1024 * 1024);
        assert!(data.len() > 1024 * 1024, "file must extend past 1 MB");

        // File type identifier signature == "vhdxfile"
        assert_eq!(&data[0..8], b"vhdxfile");

        // Header 1 signature == "head"
        assert_eq!(
            &data[usize::try_from(HEADER1_OFFSET).unwrap()..][..4],
            b"head"
        );
        // Header 2 signature == "head"
        assert_eq!(
            &data[usize::try_from(HEADER2_OFFSET).unwrap()..][..4],
            b"head"
        );

        // Region table 1 signature == "regi"
        assert_eq!(
            &data[usize::try_from(REGION_TABLE1_OFFSET).unwrap()..][..4],
            b"regi"
        );
        // Region table 2 signature == "regi"
        assert_eq!(
            &data[usize::try_from(REGION_TABLE2_OFFSET).unwrap()..][..4],
            b"regi"
        );

        validate_header_crc(&data, usize::try_from(HEADER1_OFFSET).unwrap());
        validate_header_crc(&data, usize::try_from(HEADER2_OFFSET).unwrap());

        validate_region_crc(&data, usize::try_from(REGION_TABLE1_OFFSET).unwrap());
        validate_region_crc(&data, usize::try_from(REGION_TABLE2_OFFSET).unwrap());
    }

    #[test]
    fn sequence_number_is_zero() {
        let data = create_test_vhdx(1024 * 1024 * 1024);
        let seq = u64::from_le_bytes(
            data[usize::try_from(HEADER1_OFFSET).unwrap() + 8..][..8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(seq, 0);
    }

    #[test]
    fn version_is_one() {
        let data = create_test_vhdx(1024 * 1024 * 1024);
        let version_offset =
            usize::try_from(HEADER1_OFFSET).unwrap() + 4 + 4 + 8 + 16 + 16 + 16 + 2;
        let version = u16::from_le_bytes(data[version_offset..][..2].try_into().unwrap());
        assert_eq!(version, 1);
    }

    #[test]
    fn region_table_has_two_entries() {
        let data = create_test_vhdx(1024 * 1024 * 1024);
        let count = u32::from_le_bytes(
            data[usize::try_from(REGION_TABLE1_OFFSET).unwrap() + 8..][..4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(count, 2);
    }

    #[test]
    fn validation_rejects_zero_size() {
        let tf = tempfile::NamedTempFile::new().unwrap();
        let result = File::create(tf.path()).finish();
        assert!(result.is_err());
    }

    #[test]
    fn validation_rejects_invalid_block_size() {
        let tf = tempfile::NamedTempFile::new().unwrap();
        let result = File::create(tf.path())
            .size(1024 * 1024 * 1024)
            .block_size(3 * 1024 * 1024) // not power of 2
            .finish();
        assert!(result.is_err());
    }

    #[test]
    fn validation_rejects_invalid_sector_size() {
        let tf = tempfile::NamedTempFile::new().unwrap();
        let result = File::create(tf.path())
            .size(1024 * 1024 * 1024)
            .logical_sector_size(1024)
            .finish();
        assert!(result.is_err());
    }

    #[test]
    fn validation_allows_physical_lt_logical() {
        let tf = tempfile::NamedTempFile::new().unwrap();
        let result = File::create(tf.path())
            .size(1024 * 1024 * 1024)
            .logical_sector_size(4096)
            .physical_sector_size(512)
            .finish();
        assert!(
            result.is_ok(),
            "physical(512) < logical(4096) should be valid per VHDX spec"
        );
    }

    // -- Metadata verification tests ----------------------------------------

    /// Helper: create a VHDX in `target/test-output/` and return bytes + path.
    fn create_test_vhdx_detailed(
        size: u64, block_size: u32, fixed: bool, parent_path: Option<&std::path::Path>,
    ) -> (Vec<u8>, std::path::PathBuf) {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let test_output = project_root.join("target").join("test-output");
        std::fs::create_dir_all(&test_output).expect("create test-output dir");
        let test_id: u64 = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )
        .unwrap();
        let test_dir = test_output.join(format!("vhdx-test-{test_id}"));
        std::fs::create_dir_all(&test_dir).expect("create test dir");

        // If a parent path is specified, create a minimal parent VHDX first
        let actual_parent_path = if parent_path.is_some() {
            let parent_path_buf = test_dir.join("parent.vhdx");
            File::create(&parent_path_buf)
                .size(10 * 1024 * 1024 * 1024u64) // 10 GB
                .block_size(block_size)
                .fixed(false)
                .finish()
                .expect("create parent vhdx");
            Some(parent_path_buf)
        } else {
            None
        };

        let path = test_dir.join("test.vhdx");
        let mut opts = File::create(&path)
            .size(size)
            .block_size(block_size)
            .fixed(fixed);
        if let Some(ref p) = actual_parent_path {
            opts = opts.parent_path(p);
        }
        opts.finish().expect("create vhdx");
        let mut buf = Vec::new();
        let mut f = std::fs::File::open(&path).expect("reopen");
        f.read_to_end(&mut buf).expect("read");
        // Clean up the test artifacts.
        let _ = std::fs::remove_file(&path);
        if let Some(ref p) = actual_parent_path {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_dir(&test_dir);
        (buf, path)
    }

    /// Read metadata table `entry_count` from raw bytes at `metadata_offset`.
    fn read_entry_count(data: &[u8], metadata_offset: u64) -> u16 {
        let off = usize::try_from(metadata_offset).unwrap();
        u16::from_le_bytes(data[off + 10..off + 12].try_into().unwrap())
    }

    /// Read a metadata table entry's GUID and offset from the raw buffer.
    fn read_entry(data: &[u8], metadata_offset: u64, entry_idx: usize) -> (Guid, u32, u32, u32) {
        let base = usize::try_from(metadata_offset).unwrap()
            + TABLE_HEADER_SIZE
            + entry_idx * TABLE_ENTRY_SIZE;
        let guid_bytes: [u8; 16] = data[base..base + 16].try_into().unwrap();
        let guid = Guid::from_bytes(guid_bytes);
        let offset = u32::from_le_bytes(data[base + 16..base + 20].try_into().unwrap());
        let length = u32::from_le_bytes(data[base + 20..base + 24].try_into().unwrap());
        let flags = u32::from_le_bytes(data[base + 24..base + 28].try_into().unwrap());
        (guid, offset, length, flags)
    }

    #[test]
    fn create_dynamic_metadata_table() {
        let (data, _path) =
            create_test_vhdx_detailed(1024 * 1024 * 1024, 32 * 1024 * 1024, false, None);

        let bat_size =
            CreateOptions::calculate_bat_size(1024 * 1024 * 1024, 32 * 1024 * 1024, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

        // Signature "metadata"
        let meta_off = usize::try_from(metadata_offset).unwrap();
        assert_eq!(&data[meta_off..][..8], b"metadata");

        // Entry count = 5 (dynamic, no parent)
        let count = read_entry_count(&data, metadata_offset);
        assert_eq!(count, 5, "dynamic disk should have 5 metadata entries");

        // Collect entries
        let mut found = std::collections::HashSet::new();
        for i in 0..5 {
            let (guid, offset, length, flags) =
                read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
            found.insert(format!("{guid}"));
            // Item offsets must be >= 64KB (METADATA_TABLE_SIZE)
            assert!(
                offset >= u32::try_from(METADATA_TABLE_SIZE).unwrap(),
                "entry {i} offset {offset} < 64KB"
            );
            // Flags: IsRequired bit (2 per MS-VHDX §2.6.1.2) must be set
            let ef = EntryFlags(flags);
            assert!(
                ef.is_required(),
                "entry {i} flags {flags:#x} missing IsRequired"
            );
            // Validate known GUIDs
            match i {
                0 => {
                    assert_eq!(guid, types::StandardItems::FILE_PARAMETERS);
                    assert_eq!(length, 8);
                    assert!(
                        !ef.is_virtual_disk(),
                        "FileParameters should not be IsVirtualDisk"
                    );
                }
                1 => {
                    assert_eq!(guid, types::StandardItems::VIRTUAL_DISK_SIZE);
                    assert_eq!(length, 8);
                    assert!(
                        ef.is_virtual_disk(),
                        "VirtualDiskSize should be IsVirtualDisk"
                    );
                }
                2 => {
                    assert_eq!(guid, types::StandardItems::VIRTUAL_DISK_ID);
                    assert_eq!(length, 16);
                }
                3 => {
                    assert_eq!(guid, types::StandardItems::LOGICAL_SECTOR_SIZE);
                    assert_eq!(length, 4);
                }
                4 => {
                    assert_eq!(guid, types::StandardItems::PHYSICAL_SECTOR_SIZE);
                    assert_eq!(length, 4);
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(found.len(), 5);
    }

    #[test]
    fn create_dynamic_metadata_items_values() {
        let (data, _path) =
            create_test_vhdx_detailed(10 * 1024 * 1024 * 1024u64, 32 * 1024 * 1024, false, None);

        let bat_size =
            CreateOptions::calculate_bat_size(10 * 1024 * 1024 * 1024, 32 * 1024 * 1024, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

        // Find each item by GUID and verify its value
        for i in 0..5 {
            let (guid, offset, length, _flags) =
                read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
            let meta_off = usize::try_from(metadata_offset).unwrap();
            let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                [..usize::try_from(length).unwrap()];

            if guid == types::StandardItems::FILE_PARAMETERS {
                // MS-VHDX §2.6.2.1: block_size (u32) + bit_fields (u32)
                let fp_block = u32::from_le_bytes(item_data[..4].try_into().unwrap());
                assert_eq!(fp_block, 32 * 1024 * 1024, "block size mismatch");
                let fp = item_data[4..8].view_bits::<Lsb0>();
                assert!(!fp[0], "LeaveBlockAllocated should be 0 for dynamic");
                assert!(!fp[1], "HasParent should be 0 for non-differencing");
            } else if guid == types::StandardItems::VIRTUAL_DISK_SIZE {
                let vs = u64::from_le_bytes(item_data[..8].try_into().unwrap());
                assert_eq!(vs, 10 * 1024 * 1024 * 1024, "virtual disk size mismatch");
            } else if guid == types::StandardItems::VIRTUAL_DISK_ID {
                assert_eq!(length, 16);
                // Should be non-zero (random GUID)
                let all_zero = item_data.iter().all(|&b| b == 0);
                assert!(!all_zero, "VirtualDiskId should not be zero");
            } else if guid == types::StandardItems::LOGICAL_SECTOR_SIZE
                || guid == types::StandardItems::PHYSICAL_SECTOR_SIZE
            {
                let sector_size = u32::from_le_bytes(item_data[..4].try_into().unwrap());
                assert_eq!(sector_size, 4096);
            }
        }
    }

    #[test]
    fn create_dynamic_bat_entries_not_present() {
        let size = 1024 * 1024 * 1024u64;
        let block_size = 32 * 1024 * 1024u32;
        let (data, _path) = create_test_vhdx_detailed(size, block_size, false, None);

        let bat_offset = 2 * 1024 * 1024; // BAT_REGION_OFFSET
        let (_num_payload, _num_sb, total_entries, _cr) =
            CreateOptions::compute_bat_entry_counts(size, block_size, 4096);

        for i in 0..usize::try_from(total_entries).unwrap() {
            let entry_bytes: [u8; 8] = data[bat_offset + i * 8..][..8].try_into().unwrap();
            let raw = u64::from_le_bytes(entry_bytes);
            assert_eq!(
                raw, 0,
                "BAT entry {i} should be 0 (PAYLOAD_BLOCK_NOT_PRESENT) for dynamic disk"
            );
        }
    }

    #[test]
    fn create_fixed_bat_entries_fully_present() {
        let size = 128 * 1024 * 1024u64; // 128 MB
        let block_size = 32 * 1024 * 1024u32;
        let (data, _path) = create_test_vhdx_detailed(size, block_size, true, None);

        let bat_offset = 2 * 1024 * 1024;
        let (num_payload, num_sb, total_entries, chunk_ratio) =
            CreateOptions::compute_bat_entry_counts(size, block_size, 4096);

        let bat_size = CreateOptions::calculate_bat_size(size, block_size, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);
        let raw_first_mb =
            (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(1024 * 1024);
        let payload_align = u64::from(block_size) / (1024 * 1024);
        let first_payload_offset_mb = raw_first_mb.div_ceil(payload_align) * payload_align;

        let mut sb_seen: u64 = 0;
        let mut payload_idx: u64 = 0;

        for i in 0..total_entries {
            let entry_bytes: [u8; 8] = data[bat_offset + usize::try_from(i).unwrap() * 8..][..8]
                .try_into()
                .unwrap();
            let raw = u64::from_le_bytes(entry_bytes);

            let payloads_before = i - sb_seen;
            let is_sb = payloads_before > 0
                && payloads_before.is_multiple_of(chunk_ratio)
                && sb_seen < num_sb;
            if is_sb {
                // Sector bitmap entry: NotPresent
                assert_eq!(
                    raw, 0,
                    "BAT sector bitmap entry {i} should be 0 (NotPresent) for fixed disk"
                );
                sb_seen += 1;
            } else {
                // Payload entry: FullyPresent with sequential offset
                let raw_bytes = raw.to_le_bytes();
                let raw_bits = raw_bytes.view_bits::<Lsb0>();
                let state: u8 = raw_bits[0..3].load::<u8>();
                assert_eq!(
                    state, 6,
                    "BAT payload entry {i} should be FullyPresent (6) for fixed disk"
                );

                let file_offset_mb: u64 = raw_bits[20..64].load::<u64>();
                let expected_mb =
                    first_payload_offset_mb + payload_idx * u64::from(block_size) / (1024 * 1024);
                assert_eq!(
                    file_offset_mb, expected_mb,
                    "BAT entry {i} (payload_idx={payload_idx}) offset mismatch"
                );
                payload_idx += 1;
            }
        }

        // Verify total payload count
        // When num_payload < chunk_ratio, the interleaving formula classifies
        // all entries as payload (the SB only appears at index == chunk_ratio).
        // This keeps the writer consistent with the bat.rs reader.
        let expected_payload_count = if num_payload < chunk_ratio {
            total_entries
        } else {
            num_payload
        };
        assert_eq!(
            payload_idx, expected_payload_count,
            "should have {expected_payload_count} payload entries (num_payload={num_payload}, chunk_ratio={chunk_ratio})"
        );
    }

    #[test]
    fn create_fixed_file_size_includes_payloads() {
        let size = 128 * 1024 * 1024u64; // 128 MB
        let block_size = 32 * 1024 * 1024u32;
        let (data, _path) = create_test_vhdx_detailed(size, block_size, true, None);

        let bat_size = CreateOptions::calculate_bat_size(size, block_size, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);
        let raw_first_mb =
            (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(1024 * 1024);
        let payload_align = u64::from(block_size) / (1024 * 1024);
        let first_payload_mb = raw_first_mb.div_ceil(payload_align) * payload_align;
        let first_payload = first_payload_mb * (1024 * 1024);

        let (num_payload, _num_sb, _total, _cr) =
            CreateOptions::compute_bat_entry_counts(size, block_size, 4096);
        let expected_end = first_payload + num_payload * u64::from(block_size);

        assert_eq!(
            data.len() as u64,
            expected_end,
            "fixed disk file should extend to cover all payload blocks"
        );

        // Verify payload blocks are zero-filled (spot-check first and last)
        let first_block_start = usize::try_from(first_payload).unwrap();
        let last_block_start = usize::try_from(first_payload).unwrap()
            + (usize::try_from(num_payload).unwrap() - 1) * usize::try_from(block_size).unwrap();
        assert!(
            data[first_block_start..first_block_start + 1024]
                .iter()
                .all(|&b| b == 0),
            "first payload block should be zero-filled"
        );
        assert!(
            data[last_block_start..last_block_start + 1024]
                .iter()
                .all(|&b| b == 0),
            "last payload block should be zero-filled"
        );
    }

    #[test]
    fn create_differencing_has_parent_locator() {
        let (data, _path) = create_test_vhdx_detailed(
            10 * 1024 * 1024 * 1024,
            32 * 1024 * 1024,
            false,
            Some(std::path::Path::new("parent.vhdx")),
        );

        let bat_size =
            CreateOptions::calculate_bat_size(10 * 1024 * 1024 * 1024, 32 * 1024 * 1024, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

        // Should have 6 entries (including ParentLocator)
        let count = read_entry_count(&data, metadata_offset);
        assert_eq!(count, 6, "differencing disk should have 6 metadata entries");

        // Find the ParentLocator entry
        let mut found_pl = false;
        for i in 0..6 {
            let (guid, offset, length, _flags) =
                read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
            if guid == types::StandardItems::PARENT_LOCATOR {
                found_pl = true;
                assert!(length > 0, "ParentLocator should have non-zero length");
                assert!(
                    offset >= u32::try_from(METADATA_TABLE_SIZE).unwrap(),
                    "parent locator offset < 64KB"
                );

                // Verify the parent locator data
                let meta_off = usize::try_from(metadata_offset).unwrap();
                let pl_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                    [..usize::try_from(length).unwrap()];

                // Locator header: locator_type GUID (16 bytes) + reserved (2) + kv_count (2)
                let locator_type_bytes: [u8; 16] = pl_data[..16].try_into().unwrap();
                assert_eq!(
                    Guid::from_bytes(locator_type_bytes),
                    types::StandardItems::LOCATOR_TYPE_VHDX,
                    "locator type should be VHDX"
                );
                let kv_count = u16::from_le_bytes(pl_data[18..20].try_into().unwrap());
                assert_eq!(
                    kv_count, 2,
                    "should have 2 KV entries (parent_linkage, relative_path)"
                );
            }
        }
        assert!(found_pl, "ParentLocator entry not found");
    }

    #[test]
    fn create_differencing_file_parameters_has_parent_flag() {
        let (data, _path) = create_test_vhdx_detailed(
            1024 * 1024 * 1024,
            32 * 1024 * 1024,
            false,
            Some(std::path::Path::new("parent.vhdx")),
        );

        let bat_size =
            CreateOptions::calculate_bat_size(1024 * 1024 * 1024, 32 * 1024 * 1024, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

        for i in 0..6 {
            let (guid, offset, length, _flags) =
                read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
            if guid == types::StandardItems::FILE_PARAMETERS {
                let meta_off = usize::try_from(metadata_offset).unwrap();
                let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                    [..usize::try_from(length).unwrap()];
                let fp = item_data[4..8].view_bits::<Lsb0>();
                assert!(fp[1], "HasParent flag should be set for differencing disk");
                assert!(
                    !fp[0],
                    "LeaveBlockAllocated should be 0 for dynamic differencing"
                );
                return;
            }
        }
        panic!("FileParameters not found in differencing disk metadata");
    }

    #[test]
    fn create_fixed_file_parameters_leave_block_allocated() {
        let (data, _path) =
            create_test_vhdx_detailed(128 * 1024 * 1024, 32 * 1024 * 1024, true, None);

        let bat_size = CreateOptions::calculate_bat_size(128 * 1024 * 1024, 32 * 1024 * 1024, 4096);
        let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

        for i in 0..5 {
            let (guid, offset, length, _flags) =
                read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
            if guid == types::StandardItems::FILE_PARAMETERS {
                let meta_off = usize::try_from(metadata_offset).unwrap();
                let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                    [..usize::try_from(length).unwrap()];
                let fp = item_data[4..8].view_bits::<Lsb0>();
                assert!(
                    fp[0],
                    "LeaveBlockAllocated flag should be set for fixed disk"
                );
                return;
            }
        }
        panic!("FileParameters not found in fixed disk metadata");
    }

    // -- helpers --

    fn validate_header_crc(data: &[u8], offset: usize) {
        let mut slice = data[offset..][..HEADER_SIZE].to_vec();
        let stored = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        let computed = crc32c(&slice);
        assert_eq!(stored, computed, "header CRC mismatch at offset {offset}");
    }

    fn validate_region_crc(data: &[u8], offset: usize) {
        let mut slice = data[offset..][..REGION_TABLE_SIZE].to_vec();
        let stored = u32::from_le_bytes(slice[4..8].try_into().unwrap());
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        let computed = crc32c(&slice);
        assert_eq!(
            stored, computed,
            "region table CRC mismatch at offset {offset}"
        );
    }
}
