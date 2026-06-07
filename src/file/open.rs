//! Open-options builder implementation.

use bitvec::prelude::*;
use crc32c::crc32c;
use std::io::{Read, Seek, SeekFrom, Write};

use super::{File, LogReplayPolicy, OpenOptions, is_known_metadata_guid, is_known_region_guid};
use crate::constants::{
    HEADER_BUFFER_SIZE, HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, METADATA_REGION_GUID, MIB,
    VHDX_SIGNATURE_BYTES,
};
use crate::error::{Error, Result, SignaturePosition};
use crate::header::Header;
use crate::log::Log;
use crate::log_replay;
use crate::types::Guid;
use std::sync::{Arc, OnceLock};

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
        if bytes_read < VHDX_SIGNATURE_BYTES.len() / 8 {
            return Err(Error::InvalidFile(
                "file too small to contain VHDX signature".into(),
            ));
        }
        header_buf.truncate(bytes_read);
        Ok((file, header_buf))
    }

    fn validate_file_signature(header_buf: &[u8]) -> Result<()> {
        let sig = &header_buf[..VHDX_SIGNATURE_BYTES.len() / 8];
        if sig.view_bits::<Lsb0>() == *VHDX_SIGNATURE_BYTES {
            return Ok(());
        }
        let mut actual_bytes = [0u8; 8];
        actual_bytes.copy_from_slice(sig);
        Err(Error::InvalidSignature {
            position: SignaturePosition::FileTypeIdentifier,
            expected: VHDX_SIGNATURE_BYTES.into_inner().to_le_bytes(),
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
        let entries: Vec<_> = rt.entries().collect();
        for (i, entry) in entries.iter().enumerate() {
            let file_offset = entry.file_offset();
            let length = entry.length();
            if file_offset % u64::from(MIB) != 0 {
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_ALIGNMENT: entry {i} file_offset {file_offset:#x} not 1MB-aligned"
                )));
            }
            if file_offset < u64::from(MIB) {
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_OFFSET_MINIMUM: entry {i} file_offset {file_offset} < 1MB minimum"
                )));
            }
            if u64::from(length) % u64::from(MIB) != 0 {
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
            u64::from(HEADER1_OFFSET)
        } else {
            u64::from(HEADER2_OFFSET)
        };
        let current_header = hdr.header(0)?;
        let updated_header = Self::build_updated_header(&current_header);
        file.seek(SeekFrom::Start(noncurrent_offset))?;
        file.write_all(&updated_header)?;
        file.sync_all()?;
        let start = usize::try_from(noncurrent_offset).unwrap();
        header_buf[start..start + HEADER_SIZE as usize].copy_from_slice(&updated_header);
        Ok(())
    }

    fn build_updated_header(
        current_header: &crate::header::HeaderStructure<'_>,
    ) -> [u8; HEADER_SIZE as usize] {
        let mut updated_header = [0u8; HEADER_SIZE as usize];
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
            bat_buf: std::sync::RwLock::new(None),
            metadata_buf: OnceLock::new(),
            log_buf,
            write: self.write,
            strict: self.strict,
            log_replay_policy: self.log_replay_policy,
            replay_overlay,
            validator_buf: OnceLock::new(),
        })
    }
}
