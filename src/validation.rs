//! VHDX 规范一致性校验模块
//!
//! 本模块提供只读校验入口，用于对已打开的 VHDX 文件执行
//! 结构层面的最小一致性检查。

use crate::File;
use crate::common::constants::{
    DATA_SECTOR_SIZE, FILE_TYPE_SIGNATURE, HEADER_SIGNATURE, LOG_ENTRY_SIGNATURE, LOG_VERSION,
    REGION_TABLE_SIGNATURE, VHDX_VERSION, metadata_guids, region_guids,
};
use crate::error::{Error, Result};
use crate::file::ParentChainInfo;
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
    pub fn validate_metadata(&self) -> Result<()> {
        let _metadata = self.file.sections().metadata()?;
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
    /// - leading_bytes + trailing_bytes 边界
    /// - flushed_file_offset / last_file_offset 约束
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
                                "Log entry {entry_index} missing data sector for descriptor {}",
                                data_sector_index
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

            if let Some(prev) = previous_sequence {
                if header.sequence_number() != 0 && header.sequence_number() < prev {
                    return Err(Error::LogEntryCorrupted(format!(
                        "Log entry sequence decreases at index {entry_index}"
                    )));
                }
            }
            previous_sequence = Some(header.sequence_number());
        }

        Ok(())
    }

    /// 校验 Parent Locator 的最小键约束
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

        let data = locator.key_value_data();
        let entries = locator.entries();

        let mut parent_linkage: Option<Guid> = None;
        let mut has_path = false;

        for entry in entries {
            let Some(key) = entry.key(data) else {
                continue;
            };
            match key.as_str() {
                "parent_linkage" => {
                    let value = entry.value(data).ok_or_else(|| {
                        Error::InvalidMetadata(
                            "Parent locator key parent_linkage has no value".to_string(),
                        )
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
                        Error::InvalidMetadata(
                            "Parent locator key parent_linkage2 has no value".to_string(),
                        )
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

        if parent_linkage.is_none() {
            return Err(Error::InvalidMetadata(
                "Parent locator missing required key: parent_linkage".to_string(),
            ));
        }

        if !has_path {
            return Err(Error::InvalidMetadata(
                "Parent locator must include one path key".to_string(),
            ));
        }

        Ok(())
    }

    /// 差分链校验
    ///
    /// 校验 parent_linkage / parent_linkage2 与父盘 DataWriteGuid 的一致性。
    /// 当前为最小实现：非差分盘返回错误，差分盘返回基本链信息。
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
