//! Open-options builder implementation.

use bitvec::prelude::*;
use crc32c::crc32c;
use std::io::{ErrorKind, Read, Seek, Write};

use super::{
    CacheEntry, Len, LogReplayPolicy, Medium, OpenOptions, ParentResolver, ReadOnly, ReadWrite,
    SetLen, SyncData, is_known_metadata_guid, is_known_region_guid, read_exact_at, write_all_at,
};
use crate::constants::{
    HEADER_BUFFER_SIZE, HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, METADATA_REGION_GUID, MIB,
    VHDX_SIGNATURE_BYTES,
};
use crate::error::{Error, Result, SignaturePosition};
use crate::header::Header;
use crate::log::Log;
use crate::log_replay;
use crate::types::Guid;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, RwLock};

impl<T, Mode> OpenOptions<T, Mode> {
    fn validate_policy_compatibility(write: bool, policy: LogReplayPolicy) -> Result<()> {
        match policy {
            LogReplayPolicy::InMemoryOnReadOnly | LogReplayPolicy::ReadOnlyNoReplay if write => {
                Err(Error::InvalidParameter(
                    "log replay policy incompatible with write access".into(),
                ))
            }
            _ => Ok(()),
        }
    }

    fn read_header(inner: &mut T) -> Result<Vec<u8>>
    where
        T: Read + Seek,
    {
        let mut header_buf = vec![0u8; HEADER_BUFFER_SIZE];
        let mut signature = [0u8; 8];
        match read_exact_at(inner, 0, &mut signature) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                return Err(Error::InvalidFile(
                    "file too small to contain VHDX signature".into(),
                ));
            }
            Err(err) => return Err(err.into()),
        }
        match read_exact_at(inner, 0, &mut header_buf) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                return Err(Error::InvalidFile(format!(
                    "header section too small: need at least {HEADER_BUFFER_SIZE}"
                )));
            }
            Err(err) => return Err(err.into()),
        }
        Ok(header_buf)
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
        inner: &mut T, header: &Header, strict: bool,
    ) -> Result<()>
    where
        T: Read + Seek,
    {
        let rt = header.region_table(0)?;
        Self::validate_region_table_entries(&rt, strict)?;
        Self::validate_unknown_metadata(inner, &rt, strict)
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
        inner: &mut T, rt: &crate::header::RegionTable<'_>, strict: bool,
    ) -> Result<()>
    where
        T: Read + Seek,
    {
        for entry in rt.entries() {
            if entry.guid() != METADATA_REGION_GUID {
                continue;
            }
            let mut meta_data = vec![0u8; entry.length() as usize];
            read_exact_at(inner, entry.file_offset(), &mut meta_data)?;
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

    fn load_log_data(inner: &mut T, offset: u64, length: u32) -> Result<Vec<u8>>
    where
        T: Read + Seek,
    {
        let mut log_data = vec![0u8; length as usize];
        read_exact_at(inner, offset, &mut log_data)?;
        Ok(log_data)
    }

    fn apply_writable_header_update(
        write: bool, inner: &mut T, header_buf: &mut Vec<u8>,
    ) -> Result<()>
    where
        T: Write + Seek + SyncData,
    {
        if !write {
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
        let noncurrent_offset = if noncurrent_idx == 1 {
            u64::from(HEADER1_OFFSET)
        } else {
            u64::from(HEADER2_OFFSET)
        };
        let current_header = hdr.header(0)?;
        let updated_header = Self::build_updated_header(&current_header);
        write_all_at(inner, noncurrent_offset, &updated_header)?;
        inner.sync_data()?;
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

    #[must_use]
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    #[must_use]
    pub fn log_replay(mut self, policy: LogReplayPolicy) -> Self {
        self.log_replay_policy = policy;
        self
    }

    /// Configure the resolver used for differencing disk parent reads.
    #[must_use]
    pub fn with_parent_resolver<R>(mut self, resolver: R) -> Self
    where
        R: ParentResolver + Send + 'static,
    {
        self.parent_resolver = Some(Box::new(resolver));
        self
    }
}

impl<T> OpenOptions<T, ReadOnly> {
    #[must_use]
    pub fn write(self) -> OpenOptions<T, ReadWrite>
    where
        T: Read + Write + Seek + Len + SetLen + SyncData,
    {
        OpenOptions {
            inner: self.inner,
            strict: self.strict,
            log_replay_policy: self.log_replay_policy,
            parent_resolver: self.parent_resolver,
            _mode: std::marker::PhantomData,
        }
    }

    /// Open the medium in read-only mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the medium is not a valid VHDX file, validation fails,
    /// or the selected log replay policy cannot be satisfied.
    pub fn finish(mut self) -> Result<Medium<T>>
    where
        T: Read + Seek,
    {
        Self::validate_policy_compatibility(false, self.log_replay_policy)?;
        let strict = self.strict;
        let log_replay_policy = self.log_replay_policy;
        let mut header_buf = Self::read_header(&mut self.inner)?;
        Self::validate_file_signature(&header_buf)?;
        let header = Header::new(&header_buf)?;
        let current = header.header(0)?;
        Self::validate_current_header(&current)?;
        let log_offset = current.log_offset();
        let log_length = current.log_length();
        let log_guid = current.log_guid();
        Self::validate_region_table_and_metadata(&mut self.inner, &header, strict)?;
        let log_data = Self::load_log_data(&mut self.inner, log_offset, log_length)?;

        let replay_overlay = match log_replay_policy {
            LogReplayPolicy::Require => {
                let log = Log::new(&log_data)?;
                if log_replay::has_pending_log(&log, &log_guid) {
                    return Err(Error::LogReplayRequired);
                }
                None
            }
            LogReplayPolicy::Auto | LogReplayPolicy::InMemoryOnReadOnly => {
                let log = Log::new(&log_data)?;
                if log_replay::has_pending_log(&log, &log_guid) {
                    let active = log_replay::detect_active_sequence(&log, &log_guid)?;
                    Some(Arc::new(log_replay::build_replay_overlay(&active)?))
                } else {
                    None
                }
            }
            LogReplayPolicy::ReadOnlyNoReplay => None,
        };

        if let Some(ref overlay) = replay_overlay {
            if header_buf.len() < HEADER_BUFFER_SIZE {
                header_buf.resize(HEADER_BUFFER_SIZE, 0);
            }
            overlay.apply_to_region(&mut header_buf, 0);
        }

        Ok(Medium {
            inner: Mutex::new(self.inner),
            header_buf: RwLock::new(Some(CacheEntry::new(0, Arc::from(header_buf)))),
            bat_buf: RwLock::new(None),
            metadata_buf: RwLock::new(None),
            log_buf: RwLock::new(Some(CacheEntry::new(0, Arc::from(log_data)))),
            generation: AtomicU64::new(0),
            write: false,
            strict,
            log_replay_policy,
            replay_overlay,
            parent_resolver: Mutex::new(self.parent_resolver),
            validator_buf: RwLock::new(None),
        })
    }
}

impl<T> OpenOptions<T, ReadWrite> {
    /// Open the medium in read-write mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the medium is not a valid VHDX file, validation fails,
    /// or the selected log replay policy is incompatible with write access.
    pub fn finish(mut self) -> Result<Medium<T>>
    where
        T: Read + Write + Seek + Len + SetLen + SyncData,
    {
        Self::validate_policy_compatibility(true, self.log_replay_policy)?;
        let strict = self.strict;
        let log_replay_policy = self.log_replay_policy;
        let mut header_buf = Self::read_header(&mut self.inner)?;
        Self::validate_file_signature(&header_buf)?;
        let header = Header::new(&header_buf)?;
        let current = header.header(0)?;
        Self::validate_current_header(&current)?;
        let log_offset = current.log_offset();
        let log_length = current.log_length();
        let log_guid = current.log_guid();
        Self::validate_region_table_and_metadata(&mut self.inner, &header, strict)?;
        let log_data = Self::load_log_data(&mut self.inner, log_offset, log_length)?;

        let replay_overlay = match log_replay_policy {
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
                    let file_size = self.inner.len()?;
                    if file_size < active.flushed_file_offset() {
                        return Err(Error::CorruptedHeader(format!(
                            "file truncated: size {} < FlushedFileOffset {}",
                            file_size,
                            active.flushed_file_offset()
                        )));
                    }
                    log_replay::replay_to_file(&mut self.inner, &active)?;
                }
                None
            }
            LogReplayPolicy::InMemoryOnReadOnly | LogReplayPolicy::ReadOnlyNoReplay => {
                unreachable!()
            }
        };

        Self::apply_writable_header_update(true, &mut self.inner, &mut header_buf)?;

        Ok(Medium {
            inner: Mutex::new(self.inner),
            header_buf: RwLock::new(Some(CacheEntry::new(0, Arc::from(header_buf)))),
            bat_buf: RwLock::new(None),
            metadata_buf: RwLock::new(None),
            log_buf: RwLock::new(Some(CacheEntry::new(0, Arc::from(log_data)))),
            generation: AtomicU64::new(0),
            write: true,
            strict,
            log_replay_policy,
            replay_overlay,
            parent_resolver: Mutex::new(self.parent_resolver),
            validator_buf: RwLock::new(None),
        })
    }
}
