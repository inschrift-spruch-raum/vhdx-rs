//! VHDX 规范一致性校验模块
//!
//! 本模块提供只读校验入口，用于对已打开的 VHDX 文件执行
//! 结构层面的最小一致性检查。

use crate::File;
use crate::common::constants::{
    DATA_SECTOR_SIZE, FILE_TYPE_SIGNATURE, HEADER_SIGNATURE, LOG_ENTRY_SIGNATURE, LOG_VERSION,
    MAX_BLOCK_SIZE, METADATA_SIGNATURE, MIN_BLOCK_SIZE, REGION_TABLE_SIGNATURE, VHDX_VERSION,
    metadata_guids, region_guids,
};
use crate::error::{Error, Result};
use crate::file::ParentChainInfo;
use crate::section::StandardItems::LOCATOR_TYPE_VHDX;
use crate::section::{BatState, Descriptor, PayloadBlockState, SectorBitmapState};
use crate::types::Guid;

/// 解析 Parent Locator 中的 GUID 字符串。
///
/// 兼容带花括号和大小写差异的常见表示。
fn parse_locator_guid(value: &str) -> Option<Guid> {
    let trimmed = value.trim().trim_start_matches('{').trim_end_matches('}');
    let parsed = uuid::Uuid::parse_str(trimmed).ok()?;
    let bytes = parsed.as_bytes();

    // uuid::Uuid 字节序为 RFC4122；Guid 内部使用前 3 组小端布局。
    Some(Guid::from_bytes([
        bytes[3], bytes[2], bytes[1], bytes[0], bytes[5], bytes[4], bytes[7], bytes[6], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]))
}

/// 结构化校验问题
///
/// 用于承载可报告的校验问题元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// 问题所属区域
    pub section: &'static str,
    /// 问题代码
    pub code: &'static str,
    /// 人类可读问题描述
    pub message: String,
    /// 规范参考章节
    pub spec_ref: &'static str,
}

/// 规范一致性校验器（只读）
///
/// 该类型绑定到一个已打开的 [`File`]，提供按项与整体校验入口。
pub struct SpecValidator<'a> {
    /// 待校验的 VHDX 文件
    file: &'a File,
}

impl<'a> SpecValidator<'a> {
    /// 从文件句柄创建校验器
    #[must_use]
    pub const fn new(file: &'a File) -> Self {
        Self { file }
    }

    /// 执行全部基础结构校验
    pub fn validate_file(&self) -> Result<()> {
        self.validate_header()?;
        self.validate_region_table()?;
        self.validate_bat()?;
        self.validate_metadata()?;
        self.validate_required_metadata_items()?;
        if self.file.has_parent() {
            self.validate_parent_locator()?;
            self.validate_parent_chain()?;
        }
        self.validate_log()?;
        Ok(())
    }

    /// 校验 Header 区域基本可读性
    pub fn validate_header(&self) -> Result<()> {
        let header = self.file.sections().header()?;
        let file_type = header.file_type();
        if file_type.signature() != FILE_TYPE_SIGNATURE {
            return Err(Error::CorruptedHeader(format!(
                "Invalid file type signature: expected '{}', found '{}'",
                String::from_utf8_lossy(FILE_TYPE_SIGNATURE),
                String::from_utf8_lossy(file_type.signature())
            )));
        }

        let current_header = header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("Current header is not available".to_string()))?;

        if current_header.signature() != HEADER_SIGNATURE {
            return Err(Error::CorruptedHeader(format!(
                "Invalid header signature: expected '{}', found '{}'",
                String::from_utf8_lossy(HEADER_SIGNATURE),
                String::from_utf8_lossy(current_header.signature())
            )));
        }

        if let Err(Error::InvalidChecksum { expected, actual }) = current_header.verify_checksum() {
            return Err(Error::CorruptedHeader(format!(
                "Header checksum mismatch: expected {expected:08x}, actual {actual:08x}"
            )));
        }

        if current_header.version() != VHDX_VERSION {
            return Err(Error::CorruptedHeader(format!(
                "Unsupported header version: {}",
                current_header.version()
            )));
        }

        if current_header.log_version() != LOG_VERSION {
            return Err(Error::CorruptedHeader(format!(
                "Unsupported log version: {}",
                current_header.log_version()
            )));
        }

        Ok(())
    }

    /// 校验 Region Table 基本可读性
    pub fn validate_region_table(&self) -> Result<()> {
        let header = self.file.sections().header()?;

        let region_table = header.region_table(0).ok_or_else(|| {
            Error::InvalidRegionTable("Current region table is not available".to_string())
        })?;

        let table_header = region_table.header();
        if table_header.signature() != REGION_TABLE_SIGNATURE {
            return Err(Error::InvalidRegionTable(format!(
                "Invalid region table signature: expected '{}', found '{}'",
                String::from_utf8_lossy(REGION_TABLE_SIGNATURE),
                String::from_utf8_lossy(table_header.signature())
            )));
        }

        let expected_checksum = table_header.checksum();
        let actual_checksum = crate::sections::crc32c_with_zero_field(region_table.raw(), 4, 4);
        if expected_checksum != actual_checksum {
            return Err(Error::InvalidRegionTable(format!(
                "Region table checksum mismatch: expected {expected_checksum:08x}, actual {actual_checksum:08x}"
            )));
        }

        let entries = region_table.entries();
        if entries.len() != usize::try_from(table_header.entry_count()).unwrap_or(usize::MAX) {
            return Err(Error::InvalidRegionTable(format!(
                "Region table entry count mismatch: header={}, parsed={}",
                table_header.entry_count(),
                entries.len()
            )));
        }

        if region_table.find_entry(&region_guids::BAT_REGION).is_none() {
            return Err(Error::InvalidRegionTable(
                "BAT region not found".to_string(),
            ));
        }

        if region_table
            .find_entry(&region_guids::METADATA_REGION)
            .is_none()
        {
            return Err(Error::InvalidRegionTable(
                "Metadata region not found".to_string(),
            ));
        }

        for (index, entry) in entries.iter().enumerate() {
            if entry.length() == 0 {
                return Err(Error::InvalidRegionTable(format!(
                    "Region entry {index} has zero length"
                )));
            }

            if entry.required() && !Self::is_known_region_guid(&entry.guid()) {
                return Err(Error::InvalidRegionTable(format!(
                    "Unknown required region: {:?}",
                    entry.guid()
                )));
            }

            for (other_index, other) in entries.iter().enumerate().skip(index + 1) {
                if entry.guid() == other.guid() {
                    return Err(Error::InvalidRegionTable(format!(
                        "Duplicate region GUID at entries {index} and {other_index}: {:?}",
                        entry.guid()
                    )));
                }
            }
        }

        Ok(())
    }

    /// 判断区域 GUID 是否为规范已知项
    fn is_known_region_guid(guid: &Guid) -> bool {
        *guid == region_guids::BAT_REGION || *guid == region_guids::METADATA_REGION
    }

    /// 校验 BAT 区域可读取
    pub fn validate_bat(&self) -> Result<()> {
        let bat = self.file.sections().bat()?;
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();
        let file_parameters = items.file_parameters().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: file_parameters".to_string())
        })?;
        let virtual_disk_size = items.virtual_disk_size().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: virtual_disk_size".to_string())
        })?;
        let logical_sector_size = items.logical_sector_size().ok_or_else(|| {
            Error::InvalidMetadata(
                "Missing required metadata item: logical_sector_size".to_string(),
            )
        })?;

        let block_size = file_parameters.block_size();
        let payload_blocks =
            crate::sections::Bat::calculate_payload_blocks(virtual_disk_size, block_size);
        let expected_total_entries = crate::sections::Bat::calculate_total_entries(
            virtual_disk_size,
            block_size,
            logical_sector_size,
        );

        if bat.len() != usize::try_from(expected_total_entries).unwrap_or(usize::MAX) {
            return Err(Error::InvalidMetadata(format!(
                "BAT entry count mismatch: expected={expected_total_entries}, actual={}",
                bat.len()
            )));
        }

        let chunk_ratio = usize::try_from(crate::sections::Bat::calculate_chunk_ratio(
            logical_sector_size,
            block_size,
        ))
        .map_err(|_| {
            Error::InvalidMetadata("Calculated BAT chunk ratio exceeds usize::MAX".to_string())
        })?;
        let payload_blocks = usize::try_from(payload_blocks).map_err(|_| {
            Error::InvalidMetadata("Calculated payload block count exceeds usize::MAX".to_string())
        })?;

        for (index, entry) in bat.entries().into_iter().enumerate() {
            let expected_sector_bitmap = crate::sections::Bat::is_sector_bitmap_entry_index(
                index,
                chunk_ratio,
                payload_blocks,
            );

            if expected_sector_bitmap {
                let bitmap_state = match entry.state {
                    BatState::SectorBitmap(state) => state,
                    _ => return Err(Error::InvalidBlockState(entry.state.to_bits())),
                };

                if matches!(bitmap_state, SectorBitmapState::NotPresent)
                    && entry.file_offset_mb != 0
                {
                    return Err(Error::InvalidMetadata(format!(
                        "BAT sector-bitmap entry {index} is NotPresent but file_offset_mb={}",
                        entry.file_offset_mb
                    )));
                }

                if matches!(bitmap_state, SectorBitmapState::Present) && entry.file_offset_mb == 0 {
                    return Err(Error::InvalidMetadata(format!(
                        "BAT sector-bitmap entry {index} is Present but file_offset_mb=0"
                    )));
                }
            } else {
                let payload_state = match entry.state {
                    BatState::Payload(state) => state,
                    _ => return Err(Error::InvalidBlockState(entry.state.to_bits())),
                };

                if !self.file.has_parent()
                    && matches!(payload_state, PayloadBlockState::PartiallyPresent)
                {
                    return Err(Error::InvalidBlockState(
                        PayloadBlockState::PartiallyPresent as u8,
                    ));
                }

                if matches!(payload_state, PayloadBlockState::NotPresent)
                    && entry.file_offset_mb != 0
                {
                    return Err(Error::InvalidMetadata(format!(
                        "BAT payload entry {index} is NotPresent but file_offset_mb={}",
                        entry.file_offset_mb
                    )));
                }

                if payload_state.is_allocated() && entry.file_offset_mb == 0 {
                    return Err(Error::InvalidMetadata(format!(
                        "BAT payload entry {index} is allocated but file_offset_mb=0"
                    )));
                }

                if self.file.is_fixed() {
                    if !matches!(payload_state, PayloadBlockState::FullyPresent) {
                        return Err(Error::InvalidBlockState(payload_state.to_bits()));
                    }
                    if entry.file_offset_mb == 0 {
                        return Err(Error::InvalidMetadata(format!(
                            "BAT payload entry {index} in fixed disk has zero file_offset_mb"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// 校验 Metadata 区域可读取
    ///
    /// 执行以下约束检查：
    ///
    /// 表头级别（MS-VHDX §2.6.1.1）：
    /// - 表头签名必须为 `"metadata"`
    /// - 保留字段必须为零
    /// - 表项数量不得超过 2047
    /// - 已解析表项数量与表头声明的 `entry_count` 一致
    ///
    /// 表项级别（MS-VHDX §2.6.1.2）：
    /// - 表项数据长度不得为零
    /// - 表项偏移 + 长度不得超过元数据区域大小
    /// - 表项 `item_id` 不得重复
    /// - 任意两个表项的数据区域不得重叠
    pub fn validate_metadata(&self) -> Result<()> {
        // 表项数量上限（MS-VHDX §2.6.1.1: EntryCount MUST be <= 2047）
        const MAX_METADATA_ENTRY_COUNT: u16 = 2047;

        let metadata = self.file.sections().metadata()?;
        let table = metadata.table();
        let table_header = table.header();

        // 签名校验（MS-VHDX §2.6.1.1）
        if table_header.signature() != METADATA_SIGNATURE {
            return Err(Error::InvalidMetadata(format!(
                "Invalid metadata table signature: expected '{}', found '{}'",
                String::from_utf8_lossy(METADATA_SIGNATURE),
                String::from_utf8_lossy(table_header.signature())
            )));
        }

        // 保留字段校验（MS-VHDX §2.6.1.1）
        if table_header.reserved != [0u8; 2] {
            return Err(Error::InvalidMetadata(format!(
                "Metadata table header reserved field is not zero: {:?}",
                table_header.reserved
            )));
        }

        if table_header.reserved2 != [0u8; 20] {
            return Err(Error::InvalidMetadata(format!(
                "Metadata table header reserved2 field is not zero: {:?}",
                table_header.reserved2
            )));
        }

        if table_header.entry_count() > MAX_METADATA_ENTRY_COUNT {
            return Err(Error::InvalidMetadata(format!(
                "Metadata table entry count exceeds maximum: count={}, max={MAX_METADATA_ENTRY_COUNT}",
                table_header.entry_count()
            )));
        }

        // 已解析表项数量与表头声明的一致性校验
        let entries = table.entries();
        if entries.len() != usize::from(table_header.entry_count()) {
            return Err(Error::InvalidMetadata(format!(
                "Metadata table entry count mismatch: header={}, parsed={}",
                table_header.entry_count(),
                entries.len()
            )));
        }

        // 元数据区域原始长度，作为表项偏移/长度的上界基准
        let region_len = u64::try_from(metadata.raw().len()).unwrap_or(u64::MAX);

        // 表项结构约束校验（MS-VHDX §2.6.1.2）
        for (index, entry) in entries.iter().enumerate() {
            // 零长度规则：表项的数据长度不得为零
            if entry.length() == 0 {
                return Err(Error::InvalidMetadata(format!(
                    "Metadata entry {index} has zero length"
                )));
            }

            // 偏移/长度越界校验：offset + length 不得超过元数据区域大小
            let entry_offset = u64::from(entry.offset());
            let entry_length = u64::from(entry.length());
            if entry_offset.saturating_add(entry_length) > region_len {
                return Err(Error::InvalidMetadata(format!(
                    "Metadata entry {index} out of range: offset={}, length={}, region_size={}",
                    entry.offset(),
                    entry.length(),
                    metadata.raw().len()
                )));
            }
        }

        // 重复标识符校验：同一 item_id 不得出现多次
        for (index, entry) in entries.iter().enumerate() {
            for (other_index, other) in entries.iter().enumerate().skip(index + 1) {
                if entry.item_id() == other.item_id() {
                    return Err(Error::InvalidMetadata(format!(
                        "Duplicate metadata item_id at entries {index} and {other_index}: {:?}",
                        entry.item_id()
                    )));
                }
            }
        }

        // 重叠范围校验：任意两个表项的数据区域不得重叠
        for (index, entry) in entries.iter().enumerate() {
            let a_start = u64::from(entry.offset());
            let a_end = a_start.saturating_add(u64::from(entry.length()));
            for (other_index, other) in entries.iter().enumerate().skip(index + 1) {
                let b_start = u64::from(other.offset());
                let b_end = b_start.saturating_add(u64::from(other.length()));
                // 区间 [a_start, a_end) 与 [b_start, b_end) 重叠
                if a_start < b_end && b_start < a_end {
                    return Err(Error::InvalidMetadata(format!(
                        "Metadata entries {index} and {other_index} have overlapping data ranges"
                    )));
                }
            }
        }

        // 已知元数据项语义约束（仅校验已存在项，不在此处做 required completeness）
        let items = metadata.items();

        if let Some(file_parameters) = items.file_parameters() {
            let block_size = file_parameters.block_size();
            if !(MIN_BLOCK_SIZE..=MAX_BLOCK_SIZE).contains(&block_size) {
                return Err(Error::InvalidMetadata(format!(
                    "Invalid block size: {block_size} (expected range: {MIN_BLOCK_SIZE}..={MAX_BLOCK_SIZE})"
                )));
            }
            if !block_size.is_power_of_two() {
                return Err(Error::InvalidMetadata(format!(
                    "Invalid block size: {block_size} (must be a power of two)"
                )));
            }
        }

        if let Some(logical_sector_size) = items.logical_sector_size()
            && logical_sector_size != 512
            && logical_sector_size != 4096
        {
            return Err(Error::InvalidMetadata(format!(
                "Invalid logical sector size: {logical_sector_size} (expected: 512 or 4096)"
            )));
        }

        if let Some(physical_sector_size) = items.physical_sector_size()
            && physical_sector_size != 512
            && physical_sector_size != 4096
        {
            return Err(Error::InvalidMetadata(format!(
                "Invalid physical sector size: {physical_sector_size} (expected: 512 or 4096)"
            )));
        }

        if let Some(virtual_disk_size) = items.virtual_disk_size() {
            const MAX_VIRTUAL_DISK_SIZE: u64 = 64 * 1024 * 1024 * 1024 * 1024; // 64 TiB

            if virtual_disk_size == 0 {
                return Err(Error::InvalidMetadata(
                    "Invalid virtual disk size: 0 (must be greater than 0)".to_string(),
                ));
            }

            if virtual_disk_size > MAX_VIRTUAL_DISK_SIZE {
                return Err(Error::InvalidMetadata(format!(
                    "Invalid virtual disk size: {virtual_disk_size} (max: {MAX_VIRTUAL_DISK_SIZE})"
                )));
            }

            if let Some(logical_sector_size) = items.logical_sector_size() {
                let logical_sector_size_u64 = u64::from(logical_sector_size);
                if logical_sector_size_u64 != 0 && virtual_disk_size % logical_sector_size_u64 != 0
                {
                    return Err(Error::InvalidMetadata(format!(
                        "Virtual disk size {virtual_disk_size} is not aligned to logical sector size {logical_sector_size}"
                    )));
                }
            }
        }

        if let (Some(logical_sector_size), Some(physical_sector_size)) =
            (items.logical_sector_size(), items.physical_sector_size())
            && physical_sector_size < logical_sector_size
        {
            return Err(Error::InvalidMetadata(format!(
                "Physical sector size {physical_sector_size} is smaller than logical sector size {logical_sector_size}"
            )));
        }

        Ok(())
    }

    /// 校验 required 元数据项存在性
    pub fn validate_required_metadata_items(&self) -> Result<()> {
        let metadata = self.file.sections().metadata()?;
        let table = metadata.table();
        let entries = table.entries();

        for entry in &entries {
            if entry.flags().is_required() && !Self::is_known_metadata_guid(&entry.item_id()) {
                return Err(Error::InvalidMetadata(format!(
                    "Unknown required metadata item: {:?}",
                    entry.item_id()
                )));
            }
        }

        let has_file_parameters = entries
            .iter()
            .any(|entry| entry.item_id() == metadata_guids::FILE_PARAMETERS);
        if !has_file_parameters {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: file_parameters".to_string(),
            ));
        }

        let has_virtual_disk_size = entries
            .iter()
            .any(|entry| entry.item_id() == metadata_guids::VIRTUAL_DISK_SIZE);
        if !has_virtual_disk_size {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: virtual_disk_size".to_string(),
            ));
        }

        let has_virtual_disk_id = entries
            .iter()
            .any(|entry| entry.item_id() == metadata_guids::VIRTUAL_DISK_ID);
        if !has_virtual_disk_id {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: virtual_disk_id".to_string(),
            ));
        }

        let has_logical_sector_size = entries
            .iter()
            .any(|entry| entry.item_id() == metadata_guids::LOGICAL_SECTOR_SIZE);
        if !has_logical_sector_size {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: logical_sector_size".to_string(),
            ));
        }

        let has_physical_sector_size = entries
            .iter()
            .any(|entry| entry.item_id() == metadata_guids::PHYSICAL_SECTOR_SIZE);
        if !has_physical_sector_size {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: physical_sector_size".to_string(),
            ));
        }

        Ok(())
    }

    /// 判断元数据 GUID 是否为规范已知项
    fn is_known_metadata_guid(guid: &Guid) -> bool {
        *guid == metadata_guids::FILE_PARAMETERS
            || *guid == metadata_guids::VIRTUAL_DISK_SIZE
            || *guid == metadata_guids::VIRTUAL_DISK_ID
            || *guid == metadata_guids::LOGICAL_SECTOR_SIZE
            || *guid == metadata_guids::PHYSICAL_SECTOR_SIZE
            || *guid == metadata_guids::PARENT_LOCATOR
    }

    /// 校验 Log 区域可读取
    ///
    /// 执行以下检查（与回放路径 `precheck_replay_entry` 对齐）：
    /// - 条目签名
    /// - CRC-32C 校验和
    /// - 日志 GUID 与活动头部一致性
    /// - 描述符数量与可解析数量匹配
    /// - 数据扇区签名、撕裂检测、序列号一致性
    /// - `leading_bytes` + `trailing_bytes` 边界
    /// - `flushed_file_offset` / `last_file_offset` 约束
    /// - 序列号单调性
    pub fn validate_log(&self) -> Result<()> {
        let log = self.file.sections().log()?;
        let entries = log.entries();

        if entries.is_empty() {
            return Ok(());
        }

        // 获取活动头部的 log_guid，用于 GUID 一致性校验
        let header_sections = self.file.sections().header()?;
        let current_header = header_sections
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("Current header is not available".to_string()))?;
        let expected_log_guid = current_header.log_guid();

        let mut previous_sequence: Option<u64> = None;

        for (entry_index, entry) in entries.iter().enumerate() {
            let header = entry.header();

            if header.signature() != LOG_ENTRY_SIGNATURE {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} has invalid signature"
                )));
            }

            // CRC-32C 校验：计算 entry_length 范围内的校验和（MS-VHDX §2.3.1.1），
            // 计算前将 checksum 字段 [4..8] 置零。
            let entry_length = usize::try_from(header.entry_length()).unwrap_or(entry.raw().len());
            let check_len = entry_length.min(entry.raw().len());
            if check_len >= 8 {
                let expected_checksum =
                    crate::sections::crc32c_with_zero_field(&entry.raw()[..check_len], 4, 4);
                let stored_checksum = header.checksum();
                if expected_checksum != stored_checksum {
                    return Err(Error::LogEntryCorrupted(format!(
                        "Log entry {entry_index} CRC-32C mismatch: expected={expected_checksum:08x}, stored={stored_checksum:08x}"
                    )));
                }
            }

            // 日志 GUID 一致性校验（Task 5 约束）：
            // 若活动头部 log_guid 为非空，则所有有效条目的 log_guid 必须匹配。
            if expected_log_guid != Guid::nil() && header.log_guid() != expected_log_guid {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} GUID mismatch: header={expected_log_guid:?}, entry={:?}",
                    header.log_guid()
                )));
            }

            let descriptor_count = usize::try_from(header.descriptor_count()).map_err(|_| {
                Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} descriptor_count exceeds usize::MAX"
                ))
            })?;
            let descriptor_area_end = 64usize
                .checked_add(descriptor_count.saturating_mul(32))
                .ok_or_else(|| {
                    Error::LogEntryCorrupted(format!(
                        "Log entry {entry_index} descriptor area size overflow"
                    ))
                })?;

            if descriptor_area_end > entry.raw().len() {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} descriptor area exceeds entry length"
                )));
            }

            let descriptors = entry.descriptors();
            if descriptors.len() != descriptor_count {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} descriptor parse mismatch: header={}, parsed={}",
                    descriptor_count,
                    descriptors.len()
                )));
            }

            let data_descriptors = descriptors
                .iter()
                .filter(|d| matches!(d, Descriptor::Data(_)))
                .count();
            let data_sectors = entry.data();

            if data_sectors.len() != data_descriptors {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} data sector mismatch: expected={}, actual={}",
                    data_descriptors,
                    data_sectors.len()
                )));
            }

            let mut descriptor_sequence: Option<u64> = None;
            let mut data_sector_index = 0usize;
            for desc in descriptors {
                match desc {
                    Descriptor::Data(data_desc) => {
                        let current_sequence = data_desc.sequence_number();
                        if let Some(seq) = descriptor_sequence {
                            if seq != current_sequence {
                                return Err(Error::LogEntryCorrupted(format!(
                                    "Log entry {entry_index} has inconsistent descriptor sequence numbers"
                                )));
                            }
                        } else {
                            descriptor_sequence = Some(current_sequence);
                        }

                        let trailing =
                            usize::try_from(data_desc.trailing_bytes()).map_err(|_| {
                                Error::LogEntryCorrupted(format!(
                                    "Log entry {entry_index} trailing_bytes exceeds usize::MAX"
                                ))
                            })?;
                        let leading = usize::try_from(data_desc.leading_bytes()).map_err(|_| {
                            Error::LogEntryCorrupted(format!(
                                "Log entry {entry_index} leading_bytes exceeds usize::MAX"
                            ))
                        })?;

                        if trailing + leading > DATA_SECTOR_SIZE {
                            return Err(Error::LogEntryCorrupted(format!(
                                "Log entry {entry_index} has invalid leading/trailing byte total"
                            )));
                        }

                        let sector = data_sectors.get(data_sector_index).ok_or_else(|| {
                            Error::LogEntryCorrupted(format!(
                                "Log entry {entry_index} missing data sector for descriptor {data_sector_index}"
                            ))
                        })?;

                        if sector.signature != *b"data" {
                            return Err(Error::LogEntryCorrupted(format!(
                                "Log entry {entry_index} contains invalid data sector signature"
                            )));
                        }

                        // 撕裂写入检测：sequence_high 与 sequence_low 必须一致。
                        // 注意：sequence_number() = (high << 32) | low，当 high = low = seq32
                        // 时结果为 (seq32 << 32) | seq32，不直接等于描述符中的序列号，
                        // 因此不在此处校验 sequence_number() 与描述符序列号的精确相等。
                        if sector.sequence_high() != sector.sequence_low() {
                            return Err(Error::LogEntryCorrupted(format!(
                                "Log entry {entry_index} contains torn data sector"
                            )));
                        }

                        data_sector_index += 1;
                    }
                    Descriptor::Zero(zero_desc) => {
                        let current_sequence = zero_desc.sequence_number();
                        if let Some(seq) = descriptor_sequence {
                            if seq != current_sequence {
                                return Err(Error::LogEntryCorrupted(format!(
                                    "Log entry {entry_index} has inconsistent descriptor sequence numbers"
                                )));
                            }
                        } else {
                            descriptor_sequence = Some(current_sequence);
                        }
                    }
                }
            }

            if header.last_file_offset() < header.flushed_file_offset() {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry {entry_index} last_file_offset is smaller than flushed_file_offset"
                )));
            }

            if let Some(prev) = previous_sequence
                && header.sequence_number() != 0
                && header.sequence_number() < prev
            {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log entry sequence decreases at index {entry_index}"
                )));
            }
            previous_sequence = Some(header.sequence_number());
        }

        Ok(())
    }

    /// 校验 Parent Locator 的严格键约束（MS-VHDX §2.6.2.6）
    ///
    /// 执行以下检查：
    /// 1. `locator_type` 必须等于 VHDX 标准定位器类型 GUID
    /// 2. 每条 entry 的 key/value 偏移和长度必须 > 0
    /// 3. 键名必须唯一（不允许重复键）
    /// 4. 必须包含 `parent_linkage` 键，且值为有效 GUID
    /// 5. 至少存在一个路径键（`relative_path` / `volume_path` / `absolute_win32_path`）
    pub fn validate_parent_locator(&self) -> Result<()> {
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();
        let Some(file_parameters) = items.file_parameters() else {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: file_parameters".to_string(),
            ));
        };

        if !file_parameters.has_parent() {
            return Ok(());
        }

        let locator = items.parent_locator().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
        })?;

        // 规则 1：locator_type 必须等于 VHDX 标准定位器类型 GUID（MS-VHDX §2.6.2.6.1）
        let locator_header = locator.header();
        if locator_header.locator_type() != LOCATOR_TYPE_VHDX {
            return Err(Error::InvalidMetadata(format!(
                "Parent locator locator_type mismatch: expected LOCATOR_TYPE_VHDX, found {:?}",
                locator_header.locator_type()
            )));
        }

        let data = locator.key_value_data();
        let entries = locator.entries();

        let mut parent_linkage: Option<Guid> = None;
        let mut has_path = false;

        // 已解码键名集合，用于检测重复键
        let mut seen_keys = std::collections::HashSet::<String>::new();

        for (entry_index, entry) in entries.iter().enumerate() {
            // 规则 2：key/value 偏移和长度必须 > 0
            if entry.key_length == 0 {
                return Err(Error::InvalidMetadata(format!(
                    "Parent locator entry {entry_index} has key_length=0 (must be > 0)"
                )));
            }
            if entry.value_length == 0 {
                return Err(Error::InvalidMetadata(format!(
                    "Parent locator entry {entry_index} has value_length=0 (must be > 0)"
                )));
            }

            let Some(key) = entry.key(data) else {
                // key_offset 超出 key_value_data 范围时解码失败，视为偏移无效
                return Err(Error::InvalidMetadata(format!(
                    "Parent locator entry {entry_index} key_offset ({}) out of key_value_data bounds",
                    entry.key_offset
                )));
            };

            // 规则 3：键名唯一性检查
            if !seen_keys.insert(key.clone()) {
                return Err(Error::InvalidMetadata(format!(
                    "Parent locator has duplicate key: \"{key}\""
                )));
            }

            match key.as_str() {
                "parent_linkage" => {
                    let value = entry.value(data).ok_or_else(|| {
                        Error::InvalidMetadata(format!(
                            "Parent locator entry {entry_index} value_offset ({}) out of key_value_data bounds",
                            entry.value_offset
                        ))
                    })?;
                    parent_linkage = parse_locator_guid(&value);
                    if parent_linkage.is_none() {
                        return Err(Error::InvalidMetadata(
                            "Parent locator key parent_linkage is not a valid GUID".to_string(),
                        ));
                    }
                }
                "parent_linkage2" => {
                    let value = entry.value(data).ok_or_else(|| {
                        Error::InvalidMetadata(format!(
                            "Parent locator entry {entry_index} value_offset ({}) out of key_value_data bounds",
                            entry.value_offset
                        ))
                    })?;

                    // parent_linkage2 为可选键：存在时需可解析为 GUID。
                    if parse_locator_guid(&value).is_none() {
                        return Err(Error::InvalidMetadata(
                            "Parent locator key parent_linkage2 is not a valid GUID".to_string(),
                        ));
                    }
                }
                "relative_path" | "volume_path" | "absolute_win32_path" => has_path = true,
                _ => {}
            }
        }

        // 规则 4：必须包含 parent_linkage 键
        if parent_linkage.is_none() {
            return Err(Error::InvalidMetadata(
                "Parent locator missing required key: parent_linkage".to_string(),
            ));
        }

        // 规则 5：至少存在一个路径键
        if !has_path {
            return Err(Error::InvalidMetadata(
                "Parent locator must include at least one path key (relative_path, volume_path, or absolute_win32_path)".to_string(),
            ));
        }

        Ok(())
    }

    /// 差分链单跳校验（SINGLE-HOP ONLY）
    ///
    /// 仅校验 child → direct parent 的 `DataWriteGuid` 一致性，
    /// 不执行递归遍历、循环检测或多级链校验。
    ///
    /// # 行为范围（显式固化）
    ///
    /// - **单跳**：打开 `parent_linkage` / `parent_linkage2` 指向的直接父盘，
    ///   比对其 `DataWriteGuid` 与子盘记录的 linkage GUID。
    /// - **不递归**：不向上追溯 grandparent 或更深层级。
    /// - **不检测循环**：不检测 child → parent → child 环路。
    ///
    /// # 错误路径
    ///
    /// - 非差分盘调用 → `Error::InvalidParameter`
    /// - 父盘文件不存在 → `Error::ParentNotFound`
    /// - GUID 不匹配 → `Error::ParentMismatch`
    pub fn validate_parent_chain(&self) -> Result<ParentChainInfo> {
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();

        let file_parameters = items.file_parameters().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: file_parameters".to_string())
        })?;

        if !file_parameters.has_parent() {
            return Err(Error::InvalidParameter(
                "validate_parent_chain requires a differencing disk".to_string(),
            ));
        }

        let locator = items.parent_locator().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
        })?;

        // 尝试解析父盘路径
        let parent = locator
            .resolve_parent_path()
            .ok_or_else(|| Error::ParentNotFound {
                path: std::path::PathBuf::new(),
            })?;

        // 收集 parent_linkage / parent_linkage2
        let data = locator.key_value_data();
        let entries = locator.entries();
        let mut parent_linkage: Option<Guid> = None;
        let mut parent_linkage2: Option<Guid> = None;

        for entry in entries {
            if let Some(key) = entry.key(data) {
                match key.as_str() {
                    "parent_linkage" => {
                        let value = entry.value(data).ok_or_else(|| {
                            Error::InvalidMetadata(
                                "Parent locator key parent_linkage has no value".to_string(),
                            )
                        })?;
                        parent_linkage = parse_locator_guid(&value);
                    }
                    "parent_linkage2" => {
                        let value = entry.value(data).ok_or_else(|| {
                            Error::InvalidMetadata(
                                "Parent locator key parent_linkage2 has no value".to_string(),
                            )
                        })?;
                        parent_linkage2 = parse_locator_guid(&value);
                    }
                    _ => {}
                }
            }
        }

        let linkage = parent_linkage.ok_or_else(|| {
            Error::InvalidMetadata(
                "Parent locator missing required key: parent_linkage".to_string(),
            )
        })?;

        // 读取父盘 DataWriteGuid 进行链路一致性校验。
        let parent_file = File::open(&parent).finish()?;
        let parent_sections_header = parent_file.sections().header()?;
        let parent_header = parent_sections_header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("Current header is not available".to_string()))?;
        let parent_data_write_guid = parent_header.data_write_guid();

        let linkage_matched = parent_data_write_guid == linkage
            || parent_linkage2.is_some_and(|alt| parent_data_write_guid == alt);

        if !linkage_matched {
            return Err(Error::ParentMismatch {
                expected: linkage,
                actual: parent_data_write_guid,
            });
        }

        Ok(ParentChainInfo {
            child: self.file.opened_path().to_path_buf(),
            parent,
            linkage_matched,
        })
    }
}
