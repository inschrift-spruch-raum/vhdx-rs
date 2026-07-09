//! Create-options builder implementation.

use bitvec::prelude::*;
use crc32c::crc32c;
use std::io::{BufWriter, Read, Seek, Write};

use super::{
    CreateOptions, Len, LogReplayPolicy, Medium, ParentCreateInfo, ParentMedium, SetLen, SyncData,
    read_exact_at, write_all_at,
};
use crate::constants::{
    BAT_REGION_GUID, HEADER_BUFFER_SIZE, HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, LOG_OFFSET,
    METADATA_REGION_GUID, MIB, VHDX_SIGNATURE_BYTES,
};
use crate::constants::{
    BAT_REGION_OFFSET, KV_ENTRY_SIZE, LOCATOR_HEADER_SIZE, LOG_LENGTH, METADATA_REGION_SIZE,
    METADATA_TABLE_SIZE, REGION_TABLE_SIZE, REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET,
    TABLE_ENTRY_SIZE, TABLE_HEADER_SIZE, TIB,
};
use crate::error::{Error, Result};
use crate::types::{self, Guid};
use std::sync::atomic::AtomicU64;

struct MetadataEntryMeta {
    guid: Guid,
    rel_offset: u32,
    length: u32,
    flags: u32,
}

impl<T> CreateOptions<T> {
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

    /// Set caller-provided parent metadata for a differencing disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent medium's Data Write GUID cannot be read.
    pub fn parent<P>(
        mut self, parent: &mut Medium<P>, relative_path: impl AsRef<std::path::Path>,
    ) -> Result<Self>
    where
        P: Read + Seek + Send,
    {
        self.parent = Some(ParentCreateInfo {
            relative_path: relative_path.as_ref().to_path_buf(),
            data_write_guid: parent.data_write_guid()?,
        });
        Ok(self)
    }

    // -- Validation ---------------------------------------------------------

    fn validate(&self) -> Result<()> {
        if self.virtual_size == 0 {
            return Err(Error::InvalidParameter(
                "virtual disk size must be set".into(),
            ));
        }

        if self.virtual_size > 64 * TIB {
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

        if self.block_size < MIB || self.block_size > 256 * MIB {
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

        if self.fixed && self.parent.is_some() {
            return Err(Error::InvalidParameter(
                "fixed disk cannot have a parent".into(),
            ));
        }

        Ok(())
    }

    // -- Finalisation -------------------------------------------------------

    /// Create the VHDX on the caller-provided medium.
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
    ///
    /// # Panics
    ///
    /// Panics if this builder has already had its inner medium taken.
    pub fn finish(mut self) -> Result<Medium<T>>
    where
        T: Read + Write + Seek + Len + SetLen + SyncData,
    {
        self.validate()?;

        let inner = self
            .inner
            .take()
            .expect("CreateOptions always owns a medium before finish");
        let mut w = BufWriter::new(inner);

        let bat_size =
            Self::calculate_bat_size(self.virtual_size, self.block_size, self.logical_sector_size);
        let metadata_offset = u64::from(BAT_REGION_OFFSET) + u64::from(bat_size);

        // 1. File Type Identifier (offset 0)
        Self::write_file_type_identifier(&mut w)?;

        // 2. Headers (offsets 64 KB and 128 KB)
        let file_write_guid = Guid::new_v4();
        let data_write_guid = Guid::new_v4();
        let log_guid = Guid::zero(); // No active log for fresh file (MS-VHDX §2.2.1)

        // Header 1 with sequence number 0
        let header1 = Self::build_header(0, &file_write_guid, &data_write_guid, &log_guid);
        write_all_at(&mut w, u64::from(HEADER1_OFFSET), &header1)?;

        // Header 2 with sequence number 1 (different from header1 to satisfy §2.2.2)
        let header2 = Self::build_header(1, &file_write_guid, &data_write_guid, &log_guid);
        write_all_at(&mut w, u64::from(HEADER2_OFFSET), &header2)?;

        // 3. Region Tables (offsets 192 KB and 256 KB)
        let region = Self::build_region_table(bat_size, metadata_offset);

        write_all_at(&mut w, u64::from(REGION_TABLE1_OFFSET), &region)?;

        write_all_at(&mut w, u64::from(REGION_TABLE2_OFFSET), &region)?;

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
            let payload_align = u64::from(self.block_size / MIB);
            let raw_first_mb =
                (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(u64::from(MIB));
            let first_payload_offset_mb = raw_first_mb.div_ceil(payload_align) * payload_align;
            let total_payload = num_payload * u64::from(self.block_size);
            let end = first_payload_offset_mb * u64::from(MIB) + total_payload;
            w.flush()?;
            w.get_mut().set_len(end)?;
            first_payload_offset_mb
        } else {
            let end = metadata_offset + u64::from(METADATA_REGION_SIZE);
            w.flush()?;
            w.get_mut().set_len(end)?;
            0
        };

        // 5. Write BAT entries
        Self::write_bat_entries(
            &mut w,
            self.virtual_size,
            self.block_size,
            self.logical_sector_size,
            self.fixed,
            metadata_offset,
        )?;

        let parent_data_write_guid = self.parent.as_ref().map(|parent| parent.data_write_guid);

        // 6. Write metadata table + items
        self.write_metadata(&mut w, metadata_offset, parent_data_write_guid)?;

        w.flush()?;
        w.get_mut().sync_data()?;
        let mut inner = w
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        // Re-read header buffer from the start of the medium.
        let mut header_buf = vec![0u8; HEADER_BUFFER_SIZE];
        read_exact_at(&mut inner, 0, &mut header_buf)?;

        Ok(Medium {
            inner: std::sync::Mutex::new(inner),
            header_buf: std::sync::RwLock::new(Some(super::CacheEntry::new(
                0,
                std::sync::Arc::from(header_buf),
            ))),
            bat_buf: std::sync::RwLock::new(None),
            metadata_buf: std::sync::RwLock::new(None),
            log_buf: std::sync::RwLock::new(None),
            generation: AtomicU64::new(0),
            write: true,
            strict: true,
            log_replay_policy: LogReplayPolicy::Require,
            replay_overlay: None,
            parent_resolver: std::sync::Mutex::new(None),
            resolved_parent: std::sync::Mutex::new(None),
            parent_read_cache: std::sync::Mutex::new(super::ParentReadCache::default()),
            parent_effective_read_cache: std::sync::Mutex::new(super::ParentReadCache::default()),
            validator_buf: std::sync::RwLock::new(None),
        })
    }

    // -- Internal helpers ---------------------------------------------------

    pub(crate) fn calculate_bat_size(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> u32 {
        let (_num_payload, _num_sb, total_entries, _chunk_ratio) =
            Self::compute_bat_entry_counts(virtual_size, block_size, logical_sector_size);
        let bat_bytes = total_entries * 8;
        let bat_mb = std::cmp::max(bat_bytes.div_ceil(u64::from(MIB)), 1);
        u32::try_from(bat_mb).unwrap() * (1024 * 1024)
    }

    fn write_file_type_identifier(w: &mut (impl Write + Seek)) -> Result<()> {
        let mut creator = [0u8; 512];
        let ident = "vhdx-rs\0";
        for (i, ch) in ident.encode_utf16().enumerate() {
            let off = i * 2;
            if off + 1 < 512 {
                creator[off..off + 2].copy_from_slice(&ch.to_le_bytes());
            }
        }
        write_all_at(w, 0, &VHDX_SIGNATURE_BYTES.into_inner().to_le_bytes())?;
        write_all_at(w, 8, &creator)?;
        Ok(())
    }

    fn build_header(
        sequence_number: u64, file_write_guid: &Guid, data_write_guid: &Guid, log_guid: &Guid,
    ) -> [u8; HEADER_SIZE as usize] {
        let mut buf = [0u8; HEADER_SIZE as usize];
        buf[..4].copy_from_slice(b"head");
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());
        buf[8..16].copy_from_slice(&sequence_number.to_le_bytes());
        buf[16..32].copy_from_slice(&file_write_guid.to_bytes());
        buf[32..48].copy_from_slice(&data_write_guid.to_bytes());
        buf[48..64].copy_from_slice(&log_guid.to_bytes());
        buf[64..66].copy_from_slice(&0u16.to_le_bytes());
        buf[66..68].copy_from_slice(&1u16.to_le_bytes());
        buf[68..72].copy_from_slice(&LOG_LENGTH.to_le_bytes());
        buf[72..80].copy_from_slice(&u64::from(LOG_OFFSET).to_le_bytes());

        let checksum = crc32c(&buf);
        buf[4..8].copy_from_slice(&checksum.to_le_bytes());

        buf
    }

    fn build_region_table(bat_size: u32, metadata_offset: u64) -> Vec<u8> {
        let mut buf = vec![0u8; REGION_TABLE_SIZE as usize];
        buf[..4].copy_from_slice(b"regi");
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());
        buf[8..12].copy_from_slice(&2u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0u32.to_le_bytes());

        buf[16..32].copy_from_slice(&BAT_REGION_GUID.to_bytes());
        buf[32..40].copy_from_slice(&u64::from(BAT_REGION_OFFSET).to_le_bytes());
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
        w: &mut (impl Write + Seek), virtual_size: u64, block_size: u32, logical_sector_size: u32,
        fixed: bool, metadata_offset: u64,
    ) -> Result<()> {
        let (_num_payload, num_sb, total_entries, chunk_ratio) =
            Self::compute_bat_entry_counts(virtual_size, block_size, logical_sector_size);

        if !fixed {
            // Dynamic disk: BAT region is already zero-filled by set_len.
            return Ok(());
        }

        // Fixed disk: write interleaved payload + sector-bitmap entries.
        // Align first payload to block_size boundary so that the validator's
        // payload-offset alignment check passes (MS-VHDX §2.5.1.1).
        let payload_align = u64::from(block_size / MIB);
        let raw_first_payload_mb =
            (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(u64::from(MIB));
        let first_payload_offset_mb = raw_first_payload_mb.div_ceil(payload_align) * payload_align;

        let mut sb_written: u64 = 0;
        for i in 0..total_entries {
            let entry_offset = u64::from(BAT_REGION_OFFSET)
                .checked_add(i.checked_mul(8).expect("BAT entry offset fits u64"))
                .expect("BAT entry offset fits u64");
            // Determine if this entry is a sector bitmap based on how many
            // payload entries have been written since the last SB entry.
            // SB entries appear after every chunk_ratio payload entries.
            let payloads_written = i - sb_written;
            let is_sb = payloads_written > 0
                && payloads_written.is_multiple_of(chunk_ratio)
                && sb_written < num_sb;
            if is_sb {
                // Sector bitmap entry: NotPresent
                write_all_at(w, entry_offset, &0u64.to_le_bytes())?;
                sb_written += 1;
            } else {
                // Payload entry: FullyPresent at sequential offset
                let payload_idx = payloads_written;
                let offset_mb = first_payload_offset_mb + payload_idx * u64::from(block_size / MIB);
                let mut raw_bytes = [0u8; 8];
                let bits = raw_bytes.view_bits_mut::<Lsb0>();
                bits[0..3].store::<u8>(6u8); // FullyPresent
                bits[20..64].store::<u64>(offset_mb);
                write_all_at(w, entry_offset, &raw_bytes)?;
            }
        }

        Ok(())
    }

    /// Write the full metadata table (64 KB header + entries) followed by all
    /// required metadata items at `metadata_offset`.
    fn write_metadata(
        &self, w: &mut (impl Write + Seek), metadata_offset: u64,
        parent_data_write_guid: Option<Guid>,
    ) -> Result<()> {
        let has_parent = self.parent.is_some();
        let (items_buf, item_metas) =
            self.build_metadata_items(has_parent, parent_data_write_guid)?;
        let table = Self::build_metadata_table(if has_parent { 6 } else { 5 }, &item_metas);
        write_all_at(w, metadata_offset, &table)?;
        write_all_at(
            w,
            metadata_offset + u64::from(METADATA_TABLE_SIZE),
            &items_buf,
        )?;
        Ok(())
    }

    fn rel_metadata_offset(items_buf: &[u8]) -> Result<u32> {
        let base = METADATA_TABLE_SIZE;
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
        let mut table = vec![0u8; METADATA_TABLE_SIZE as usize];
        table[0..8].copy_from_slice(b"metadata");
        table[10..12].copy_from_slice(&entry_count.to_le_bytes());
        let mut entry_off: usize = TABLE_HEADER_SIZE as usize;
        for meta in item_metas {
            table[entry_off..entry_off + 16].copy_from_slice(&meta.guid.to_bytes());
            table[entry_off + 16..entry_off + 20].copy_from_slice(&meta.rel_offset.to_le_bytes());
            table[entry_off + 20..entry_off + 24].copy_from_slice(&meta.length.to_le_bytes());
            table[entry_off + 24..entry_off + 28].copy_from_slice(&meta.flags.to_le_bytes());
            entry_off += TABLE_ENTRY_SIZE as usize;
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
            .parent
            .as_ref()
            .map(|p| p.relative_path.to_string_lossy().to_string())
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

        let kv_data_start = LOCATOR_HEADER_SIZE as usize + 2 * KV_ENTRY_SIZE as usize;

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
        let kv0_off = LOCATOR_HEADER_SIZE as usize;
        buf[kv0_off..kv0_off + 4].copy_from_slice(&u32::try_from(key1_off).unwrap().to_le_bytes());
        buf[kv0_off + 4..kv0_off + 8]
            .copy_from_slice(&u32::try_from(val1_off).unwrap().to_le_bytes());
        buf[kv0_off + 8..kv0_off + 10]
            .copy_from_slice(&u16::try_from(key1_utf16.len()).unwrap().to_le_bytes());
        buf[kv0_off + 10..kv0_off + 12]
            .copy_from_slice(&u16::try_from(val1_utf16.len()).unwrap().to_le_bytes());

        // KV entry 1: relative_path
        let kv1_off = LOCATOR_HEADER_SIZE as usize + KV_ENTRY_SIZE as usize;
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
}
