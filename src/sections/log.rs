//! VHDX 日志解析与回放模块
//!
//! 本模块实现了 VHDX 日志（Log）的解析与回放，对应 MS-VHDX §2.3。
//!
//! 日志用于保护 VHDX 元数据更新的事务完整性。当 VHDX 文件更新元数据时，
//! 变更先写入日志区域，然后再应用到目标位置。如果写入过程中发生中断，
//! 可以通过回放日志恢复一致性。
//!
//! 日志以环形缓冲区（circular buffer）方式组织在文件的日志区域中，
//! 由一系列日志条目（[`LogEntry`]）组成，每个条目包含：
//! - **条目头部**（[`LogEntryHeader`]）— 签名、校验和、序列号等（§2.3.1.1）
//! - **描述符**（[`Descriptor`]）— 数据描述符或零描述符（§2.3.1.2/§2.3.1.3）
//! - **数据扇区**（[`DataSector`]）— 实际的数据负载（§2.3.1.4）

use crate::common::constants::{
    DATA_DESCRIPTOR_SIGNATURE, DATA_SECTOR_SIZE, DESCRIPTOR_SIZE, LOG_ENTRY_HEADER_SIZE,
    LOG_ENTRY_SIGNATURE, ZERO_DESCRIPTOR_SIGNATURE,
};
use crate::error::{Error, Result};
use crate::types::Guid;
use crc32c::crc32c;
use std::marker::PhantomData;

/// 从切片安全读取固定长度数组；长度不足时以 0 填充。
fn read_array<const N: usize>(data: &[u8], start: usize) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(slice) = data.get(start..start + N) {
        out.copy_from_slice(slice);
    }
    out
}

/// 从切片安全读取 `u32`（LE）；长度不足返回 0。
fn read_u32(data: &[u8], start: usize) -> u32 {
    u32::from_le_bytes(read_array::<4>(data, start))
}

/// 从切片安全读取 `u64`（LE）；长度不足返回 0。
fn read_u64(data: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(read_array::<8>(data, start))
}

/// 从切片安全读取 GUID；长度不足的字节按 0 填充。
fn read_guid(data: &[u8], start: usize) -> Guid {
    Guid::from_bytes(read_array::<16>(data, start))
}

/// 计算日志条目 CRC-32C（MS-VHDX §2.3.1.1）。
///
/// 计算前会将 checksum 字段（偏移 4..8）置零。
fn calculate_log_entry_crc32c(entry_bytes: &[u8]) -> u32 {
    let mut crc_input = entry_bytes.to_vec();
    if crc_input.len() >= 8 {
        crc_input[4..8].fill(0);
    }
    crc32c(&crc_input)
}

/// VHDX 日志区域（MS-VHDX §2.3）
///
/// 包装日志区域的原始数据，提供对日志条目的解析和回放功能。
/// 日志区域在文件中由头部结构的 `log_offset` 和 `log_length` 字段定位。
pub struct Log<'a> {
    /// 日志区域的原始字节数据
    raw_data: Vec<u8>,
    marker: PhantomData<&'a [u8]>,
}

impl Log<'_> {
    /// 从原始数据创建日志实例
    #[must_use]
    pub const fn new(data: Vec<u8>) -> Self {
        Self {
            raw_data: data,
            marker: PhantomData,
        }
    }

    /// 返回日志区域的原始字节数据
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// 获取指定索引的日志条目
    ///
    /// 索引语义与 [`Self::entries`] 保持一致：
    /// - 采用相同的线性扫描顺序
    /// - 仅统计成功解析且长度有效（>= 头部大小）的条目
    /// - 解析失败时按扇区步进继续
    #[must_use]
    pub const fn entry(&self, index: usize) -> Option<LogEntry<'_>> {
        let raw = self.raw_data.as_slice();
        let mut current_index = 0;
        let mut offset = 0;

        while offset + LOG_ENTRY_HEADER_SIZE <= raw.len() {
            // 与 entries() 对齐：仅要求剩余数据至少包含头部，读取 entry_length 决定步进
            let entry_length = u32::from_le_bytes([
                raw[offset + 8],
                raw[offset + 9],
                raw[offset + 10],
                raw[offset + 11],
            ]);
            let entry_len = entry_length as usize;

            // 条目长度小于头部大小，视为无效，按扇区步进
            if entry_len < LOG_ENTRY_HEADER_SIZE {
                // 解析失败，按扇区步进
                offset += DATA_SECTOR_SIZE;
                continue;
            }

            if current_index == index {
                let data_len = raw.len() - offset;
                // 在 const 上下文中避免使用切片 range 索引（当前仍受限），
                // 通过原始指针重建从 offset 开始的尾部切片。
                let data =
                    unsafe { std::slice::from_raw_parts(raw.as_ptr().add(offset), data_len) };
                return Some(LogEntry { data });
            }

            current_index += 1;
            offset += entry_len;
        }

        None
    }

    /// 扫描日志区域，解析所有有效的日志条目
    ///
    /// 从日志区域起始位置开始，逐个尝试解析日志条目。
    /// 如果解析失败或条目长度异常，则按扇区大小（4KB）步进继续扫描。
    #[must_use]
    pub fn entries(&self) -> Vec<LogEntry<'_>> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + LOG_ENTRY_HEADER_SIZE <= self.raw_data.len() {
            if let Ok(entry) = self.try_parse_entry_at(offset) {
                let entry_len = usize::try_from(entry.header().entry_length()).unwrap_or(0);
                // 条目长度小于头部大小，视为无效，按扇区步进
                if entry_len < LOG_ENTRY_HEADER_SIZE {
                    offset += DATA_SECTOR_SIZE;
                    continue;
                }
                entries.push(entry);
                offset += entry_len;
            } else {
                // 解析失败，按扇区步进
                offset += DATA_SECTOR_SIZE;
            }
        }

        entries
    }

    /// 尝试在指定偏移量处解析日志条目
    ///
    /// 检查剩余数据是否足够容纳日志条目头部，然后委托给 [`LogEntry::new`]。
    fn try_parse_entry_at(&self, offset: usize) -> Result<LogEntry<'_>> {
        if offset + LOG_ENTRY_HEADER_SIZE > self.raw_data.len() {
            return Err(Error::LogEntryCorrupted("Not enough data".to_string()));
        }
        LogEntry::new(&self.raw_data[offset..])
    }

    /// 检查是否存在需要回放的日志条目
    ///
    /// 如果日志区域中存在有效且连续的活动序列条目，则返回 `true`。
    #[must_use]
    pub fn is_replay_required(&self) -> bool {
        let active = self.active_entries();
        if !active.is_empty() {
            return true;
        }

        // 若存在 "loge" 候选但不构成有效活动序列，也应触发回放路径，
        // 由严格校验返回损坏错误，避免静默忽略非法条目。
        self.entries()
            .iter()
            .any(|entry| entry.header().signature() == LOG_ENTRY_SIGNATURE)
    }

    /// 回放日志条目以恢复文件一致性（MS-VHDX §2.3.3）
    ///
    /// 回放流程：
    /// 1. 解析所有日志条目
    /// 2. 对每个条目，验证签名
    /// 3. 遍历描述符：
    ///    - **数据描述符**：按扇区合并语义，将数据扇区有效部分写入指定偏移，
    ///      `leading_bytes` 和 `trailing_bytes` 定义目标范围中需保留的字节数
    ///    - **零描述符**：在指定偏移处写入零填充
    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        self.replay_entries(file, self.active_entries_strict()?)
    }

    /// 使用指定的活动日志 GUID 回放日志条目（Task 5）。
    ///
    /// 规则：
    /// - `expected_log_guid == Guid::nil()` 视为无可回放日志，直接返回 `Ok(())`
    /// - 仅允许回放 `entry.header().log_guid == expected_log_guid` 的条目
    /// - 一旦发现 GUID 不匹配条目，立即返回 `Error::LogEntryCorrupted`
    pub fn replay_with_log_guid(
        &self, file: &mut std::fs::File, expected_log_guid: Guid,
    ) -> Result<()> {
        if expected_log_guid == Guid::nil() {
            return Ok(());
        }

        let entries = self.entries_for_log_guid(expected_log_guid)?;
        self.replay_entries(file, entries)
    }

    /// 过滤并校验可回放条目：仅允许与 `expected_log_guid` 一致的条目。
    pub fn entries_for_log_guid(&self, expected_log_guid: Guid) -> Result<Vec<LogEntry<'_>>> {
        if expected_log_guid == Guid::nil() {
            return Ok(Vec::new());
        }

        let mut matched = Vec::new();
        for entry in self.entries() {
            // 仅将真正的 log entry 候选纳入 GUID 一致性检查：
            // - 非 "loge" 签名（例如日志区尾部零槽位/噪声）直接忽略
            // - "loge" 条目必须通过 Task 4 precheck，失败即报错
            let header = entry.header();
            if header.signature() != LOG_ENTRY_SIGNATURE {
                continue;
            }
            // 对 "loge" 候选必须严格校验，禁止静默跳过损坏条目。
            Self::validate_replay_candidate(&entry)?;

            let entry_log_guid = entry.header().log_guid();
            if entry_log_guid != expected_log_guid {
                return Err(Error::LogEntryCorrupted(format!(
                    "Log GUID mismatch: expected {expected_log_guid:?}, found {entry_log_guid:?}",
                )));
            }
            matched.push(entry);
        }
        Ok(Self::take_contiguous_active_sequence(matched))
    }

    /// 提取可回放的活动序列（仅包含通过有效性校验的连续条目）。
    fn active_entries(&self) -> Vec<LogEntry<'_>> {
        let mut candidates = Vec::new();
        for entry in self.entries() {
            let header = entry.header();
            if header.signature() != LOG_ENTRY_SIGNATURE {
                continue;
            }
            if Self::validate_replay_candidate(&entry).is_ok() {
                candidates.push(entry);
            }
        }
        Self::take_contiguous_active_sequence(candidates)
    }

    /// 提取可回放活动序列（严格模式）。
    ///
    /// 与 `active_entries` 的差异：遇到任何 "loge" 损坏候选立即报错，
    /// 防止回放路径静默忽略非法条目。
    fn active_entries_strict(&self) -> Result<Vec<LogEntry<'_>>> {
        let mut candidates = Vec::new();
        for entry in self.entries() {
            let header = entry.header();
            if header.signature() != LOG_ENTRY_SIGNATURE {
                continue;
            }
            Self::validate_replay_candidate(&entry)?;
            candidates.push(entry);
        }
        Ok(Self::take_contiguous_active_sequence(candidates))
    }

    /// 按序列号连续性从候选条目中提取活动序列前缀。
    ///
    /// 规则：
    /// - 以首个候选条目为起点
    /// - 仅接受 `sequence_number` 严格递增且步长为 1 的后续条目
    /// - 一旦出现断链（缺号/回退/跳号），立即停止
    fn take_contiguous_active_sequence(entries: Vec<LogEntry<'_>>) -> Vec<LogEntry<'_>> {
        let mut active = Vec::new();
        let mut expected_next: Option<u64> = None;

        for entry in entries {
            let seq = entry.header().sequence_number();
            match expected_next {
                None => {
                    expected_next = Some(seq.saturating_add(1));
                    active.push(entry);
                }
                Some(next) if seq == next => {
                    expected_next = Some(seq.saturating_add(1));
                    active.push(entry);
                }
                Some(_) => break,
            }
        }

        active
    }

    /// 校验条目是否可作为活动序列候选。
    ///
    /// 校验包含：
    /// - Task 4 预检（签名/长度/描述符区边界/CRC）
    /// - 描述符数量与解析结果一致
    /// - 数据描述符数量与数据扇区数量一致
    /// - 同一条目内描述符序列号一致
    /// - 数据扇区签名与撕裂检测通过，且扇区序列号匹配描述符
    /// - `leading + trailing` 不超过数据扇区有效载荷长度
    fn validate_replay_candidate(entry: &LogEntry<'_>) -> Result<()> {
        let _entry_len = Self::precheck_replay_entry(entry)?;

        let header = entry.header();
        let descriptor_count = usize::try_from(header.descriptor_count()).map_err(|_| {
            Error::LogEntryCorrupted("descriptor_count exceeds usize::MAX".to_string())
        })?;

        let descriptors = entry.descriptors();
        if descriptors.len() != descriptor_count {
            return Err(Error::LogEntryCorrupted(format!(
                "descriptor parse mismatch: header={descriptor_count}, parsed={}",
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
                "data sector mismatch: expected={data_descriptors}, actual={}",
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
                            return Err(Error::LogEntryCorrupted(
                                "inconsistent descriptor sequence numbers".to_string(),
                            ));
                        }
                    } else {
                        descriptor_sequence = Some(current_sequence);
                    }

                    let sector = data_sectors.get(data_sector_index).ok_or_else(|| {
                        Error::LogEntryCorrupted("missing data sector for descriptor".to_string())
                    })?;

                    if sector.signature != *b"data" {
                        return Err(Error::LogEntryCorrupted(
                            "invalid data sector signature".to_string(),
                        ));
                    }
                    if sector.sequence_high() != sector.sequence_low() {
                        return Err(Error::LogEntryCorrupted(
                            "torn data sector detected".to_string(),
                        ));
                    }

                    let leading = usize::try_from(data_desc.leading_bytes()).map_err(|_| {
                        Error::LogEntryCorrupted("leading_bytes exceeds usize::MAX".to_string())
                    })?;
                    let trailing = usize::try_from(data_desc.trailing_bytes()).map_err(|_| {
                        Error::LogEntryCorrupted("trailing_bytes exceeds usize::MAX".to_string())
                    })?;
                    if leading
                        .checked_add(trailing)
                        .is_none_or(|sum| sum > sector.data().len())
                    {
                        return Err(Error::LogEntryCorrupted(
                            "leading_bytes + trailing_bytes exceeds sector data size".to_string(),
                        ));
                    }

                    data_sector_index += 1;
                }
                Descriptor::Zero(zero_desc) => {
                    let current_sequence = zero_desc.sequence_number();
                    if let Some(seq) = descriptor_sequence {
                        if seq != current_sequence {
                            return Err(Error::LogEntryCorrupted(
                                "inconsistent descriptor sequence numbers".to_string(),
                            ));
                        }
                    } else {
                        descriptor_sequence = Some(current_sequence);
                    }
                }
            }
        }

        Ok(())
    }

    /// 基于给定条目集合执行回放。
    fn replay_entries(&self, file: &mut std::fs::File, entries: Vec<LogEntry<'_>>) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        // 解析所有日志条目
        if entries.is_empty() {
            return Ok(());
        }

        // 获取当前文件长度，用于 flushed_file_offset / last_file_offset 约束检查
        let file_len = file.metadata().map_or(0, |m| m.len());

        // 预扫描：验证 flushed_file_offset 约束并收集 last_file_offset
        let mut max_last_file_offset: u64 = 0;
        for entry in &entries {
            let header = entry.header();
            let flushed = header.flushed_file_offset();
            // 若文件长度 < flushed_file_offset，说明回放前提不满足
            if flushed > 0 && file_len < flushed {
                return Err(Error::LogEntryCorrupted(format!(
                    "File size ({file_len}) is less than flushed_file_offset ({flushed})"
                )));
            }
            let last = header.last_file_offset();
            if last > max_last_file_offset {
                max_last_file_offset = last;
            }
        }

        for entry in entries {
            // 先执行条目级前置校验；失败必须立即返回，禁止继续回放。
            let entry_len = Self::precheck_replay_entry(&entry)?;

            // 提取描述符和数据扇区
            let descriptors = entry.descriptors();
            let data_sectors = entry.data();
            let mut data_sector_index = 0;

            // 二次防御：确保描述符区间不会越过 entry_length 指定边界。
            // 该校验与 precheck 口径一致，保证 descriptor 处理前强制拦截。
            let descriptor_count =
                usize::try_from(entry.header().descriptor_count()).map_err(|_| {
                    Error::LogEntryCorrupted("descriptor_count exceeds usize::MAX".to_string())
                })?;
            let descriptor_area_end = LOG_ENTRY_HEADER_SIZE
                .checked_add(descriptor_count.saturating_mul(DESCRIPTOR_SIZE))
                .ok_or_else(|| {
                    Error::LogEntryCorrupted("descriptor area size overflow".to_string())
                })?;
            if descriptor_area_end > entry_len {
                return Err(Error::LogEntryCorrupted(
                    "descriptor area exceeds entry length".to_string(),
                ));
            }

            for desc in descriptors {
                match desc {
                    Descriptor::Data(data_desc) => {
                        // 数据描述符：按 MS-VHDX §2.3.3 扇区合并语义写入
                        //
                        // leading_bytes 和 trailing_bytes 定义目标范围内需要保留
                        // （不覆盖）的前后字节数。有效数据从数据扇区开头取
                        // effective_len = 4084 - leading - trailing 字节，
                        // 写入目标 file_offset + leading 处。
                        if data_sector_index < data_sectors.len() {
                            let sector = &data_sectors[data_sector_index];
                            let file_offset = data_desc.file_offset();
                            let sector_data = sector.data();

                            let leading =
                                usize::try_from(data_desc.leading_bytes()).map_err(|_| {
                                    Error::LogEntryCorrupted(
                                        "leading_bytes exceeds usize::MAX".to_string(),
                                    )
                                })?;
                            let trailing =
                                usize::try_from(data_desc.trailing_bytes()).map_err(|_| {
                                    Error::LogEntryCorrupted(
                                        "trailing_bytes exceeds usize::MAX".to_string(),
                                    )
                                })?;

                            // 边界安全：leading + trailing 不得超过扇区数据长度
                            if leading
                                .checked_add(trailing)
                                .is_none_or(|sum| sum > sector_data.len())
                            {
                                return Err(Error::LogEntryCorrupted(format!(
                                    "leading_bytes ({leading}) + trailing_bytes ({trailing}) \
                                     exceeds sector data size ({})",
                                    sector_data.len()
                                )));
                            }

                            let effective_len = sector_data.len() - leading - trailing;

                            // 定位到跳过 leading 字节后的目标偏移
                            file.seek(SeekFrom::Start(
                                file_offset
                                    .checked_add(u64::try_from(leading).map_err(|_| {
                                        Error::LogEntryCorrupted(
                                            "file_offset + leading overflow".to_string(),
                                        )
                                    })?)
                                    .ok_or_else(|| {
                                        Error::LogEntryCorrupted(
                                            "file_offset + leading overflow".to_string(),
                                        )
                                    })?,
                            ))?;

                            // 写入有效数据段（数据扇区 payload 的前 effective_len 字节）
                            file.write_all(&sector_data[..effective_len])?;

                            data_sector_index += 1;
                        }
                    }
                    Descriptor::Zero(zero_desc) => {
                        // 零描述符：在指定偏移处写入指定长度的零字节
                        let file_offset = zero_desc.file_offset();
                        let length = zero_desc.zero_length();

                        file.seek(SeekFrom::Start(file_offset))?;
                        file.write_all(&vec![0u8; usize::try_from(length).unwrap_or(0)])?;
                    }
                }
            }
        }

        // 回放完成后：若 last_file_offset 超出当前文件长度，扩展文件
        if max_last_file_offset > 0 {
            let current_len = file.metadata().map_or(0, |m| m.len());
            if max_last_file_offset > current_len {
                file.seek(SeekFrom::Start(max_last_file_offset - 1))?;
                file.write_all(&[0u8])?;
            }
        }

        Ok(())
    }

    /// 回放前的日志条目基础有效性校验（Task 4）。
    ///
    /// 校验口径对齐 `SpecValidator::validate_log` 的基础约束：
    /// - signature 必须为 `loge`
    /// - `entry_length` 必须在合法边界内
    /// - descriptor area 必须落在 `entry_length` 内
    /// - CRC-32C 必须匹配（计算时 checksum 字段置零）
    fn precheck_replay_entry(entry: &LogEntry<'_>) -> Result<usize> {
        let header = entry.header();

        if header.signature() != LOG_ENTRY_SIGNATURE {
            return Err(Error::LogEntryCorrupted(
                "Invalid log entry signature".to_string(),
            ));
        }

        let entry_len = usize::try_from(header.entry_length())
            .map_err(|_| Error::LogEntryCorrupted("entry_length exceeds usize::MAX".to_string()))?;
        if entry_len < LOG_ENTRY_HEADER_SIZE {
            return Err(Error::LogEntryCorrupted(
                "entry_length smaller than log entry header".to_string(),
            ));
        }
        if entry_len > entry.raw().len() {
            return Err(Error::LogEntryCorrupted(
                "entry_length exceeds available entry bytes".to_string(),
            ));
        }

        let descriptor_count = usize::try_from(header.descriptor_count()).map_err(|_| {
            Error::LogEntryCorrupted("descriptor_count exceeds usize::MAX".to_string())
        })?;
        let descriptor_area_end = LOG_ENTRY_HEADER_SIZE
            .checked_add(descriptor_count.saturating_mul(DESCRIPTOR_SIZE))
            .ok_or_else(|| Error::LogEntryCorrupted("descriptor area size overflow".to_string()))?;
        if descriptor_area_end > entry_len {
            return Err(Error::LogEntryCorrupted(
                "descriptor area exceeds entry length".to_string(),
            ));
        }

        let crc_expected = header.checksum();
        let crc_actual = calculate_log_entry_crc32c(&entry.raw()[..entry_len]);
        if crc_actual != crc_expected {
            return Err(Error::LogEntryCorrupted(format!(
                "Invalid log entry checksum: expected {crc_expected:#010x}, calculated {crc_actual:#010x}",
            )));
        }

        Ok(entry_len)
    }
}

/// 日志条目（MS-VHDX §2.3.1）
///
/// 每个日志条目由三部分组成：
/// 1. 条目头部（64 字节）— 包含签名、校验和和描述符计数
/// 2. 描述符数组（每个 32 字节）— 描述要写入的位置和方式
/// 3. 数据扇区数组（每个 4KB）— 包含实际要写入的数据
pub struct LogEntry<'a> {
    /// 条目的原始字节数据（包含头部、描述符和数据扇区）
    data: &'a [u8],
}

impl<'a> LogEntry<'a> {
    /// 从原始字节切片解析日志条目
    ///
    /// 要求数据长度至少能容纳一个日志条目头部（64 字节）。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < LOG_ENTRY_HEADER_SIZE {
            return Err(Error::LogEntryCorrupted("LogEntry too small".to_string()));
        }
        Ok(Self { data })
    }

    /// 返回条目的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 返回日志条目的头部（前 64 字节）
    #[must_use]
    pub fn header(&self) -> LogEntryHeader<'_> {
        LogEntryHeader::new(&self.data[0..LOG_ENTRY_HEADER_SIZE])
    }

    /// 获取指定索引处的描述符
    ///
    /// 描述符紧跟在头部之后，每个占 32 字节。
    #[must_use]
    pub fn descriptor(&self, index: usize) -> Option<Descriptor<'_>> {
        let header = self.header();
        if index >= usize::try_from(header.descriptor_count()).unwrap_or(0) {
            return None;
        }

        let desc_offset = LOG_ENTRY_HEADER_SIZE + index * DESCRIPTOR_SIZE;
        if desc_offset + DESCRIPTOR_SIZE > self.data.len() {
            return None;
        }

        Descriptor::parse(&self.data[desc_offset..desc_offset + DESCRIPTOR_SIZE]).ok()
    }

    /// 返回条目中的所有描述符
    #[must_use]
    pub fn descriptors(&self) -> Vec<Descriptor<'_>> {
        let count = usize::try_from(self.header().descriptor_count()).unwrap_or(0);
        (0..count).filter_map(|i| self.descriptor(i)).collect()
    }

    /// 返回条目中的所有数据扇区
    ///
    /// 数据扇区位于头部和描述符之后。仅数据描述符对应的数据扇区被计入，
    /// 零描述符不占用数据扇区。
    #[must_use]
    pub fn data(&self) -> Vec<DataSector<'_>> {
        let header = self.header();
        let desc_count = usize::try_from(header.descriptor_count()).unwrap_or(0);
        // 数据扇区起始位置 = 头部 + 描述符数组
        let data_start = LOG_ENTRY_HEADER_SIZE + desc_count * DESCRIPTOR_SIZE;

        // 计算需要的数据扇区数量（仅数据描述符需要）
        let data_sectors_needed: usize = self
            .descriptors()
            .iter()
            .filter_map(|d| match d {
                Descriptor::Data(_) => Some(1),
                Descriptor::Zero(_) => None,
            })
            .sum();

        let mut sectors = Vec::with_capacity(data_sectors_needed);
        for i in 0..data_sectors_needed {
            let offset = data_start + i * DATA_SECTOR_SIZE;
            if offset + DATA_SECTOR_SIZE > self.data.len() {
                break;
            }
            if let Ok(sector) = DataSector::new(&self.data[offset..offset + DATA_SECTOR_SIZE]) {
                sectors.push(sector);
            }
        }

        sectors
    }
}

/// 日志条目头部（MS-VHDX §2.3.1.1）
///
/// 固定 64 字节，包含日志条目的元数据。
pub struct LogEntryHeader<'a> {
    /// 日志条目签名（应为 "loge"）
    pub signature: [u8; 4],
    /// CRC32C 校验和
    pub checksum: u32,
    /// 条目总长度
    pub entry_length: u32,
    /// 环形缓冲 tail
    pub tail: u32,
    /// 序列号
    pub sequence_number: u64,
    /// 描述符数量
    pub descriptor_count: u32,
    /// 保留字段
    pub reserved: u32,
    /// 日志 GUID
    pub log_guid: Guid,
    /// 已刷写文件偏移
    pub flushed_file_offset: u64,
    /// 最后文件偏移
    pub last_file_offset: u64,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> LogEntryHeader<'a> {
    /// 从原始字节切片创建日志条目头部
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            signature: read_array::<4>(data, 0),
            checksum: read_u32(data, 4),
            entry_length: read_u32(data, 8),
            tail: read_u32(data, 12),
            sequence_number: read_u64(data, 16),
            descriptor_count: read_u32(data, 24),
            reserved: read_u32(data, 28),
            log_guid: read_guid(data, 32),
            flushed_file_offset: read_u64(data, 48),
            last_file_offset: read_u64(data, 56),
            raw: data,
        }
    }

    /// 返回头部的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 日志条目签名 "loge"（MS-VHDX §2.3.1.1）
    #[must_use]
    pub const fn signature(&self) -> &[u8; 4] {
        &self.signature
    }

    /// CRC32C 校验和（MS-VHDX §2.3.1.1）
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }

    /// 条目总长度（MS-VHDX §2.3.1.1），包含头部、描述符和数据扇区
    #[must_use]
    pub const fn entry_length(&self) -> u32 {
        self.entry_length
    }

    /// 尾部偏移量（MS-VHDX §2.3.1.1），用于环形缓冲区管理
    #[must_use]
    pub const fn tail(&self) -> u32 {
        self.tail
    }

    /// 序列号（MS-VHDX §2.3.1.1），单调递增，用于确定条目顺序
    #[must_use]
    pub const fn sequence_number(&self) -> u64 {
        self.sequence_number
    }

    /// 描述符数量（MS-VHDX §2.3.1.1）
    #[must_use]
    pub const fn descriptor_count(&self) -> u32 {
        self.descriptor_count
    }

    /// 日志 GUID（MS-VHDX §2.3.1.1），标识该条目所属的日志
    #[must_use]
    pub const fn log_guid(&self) -> Guid {
        self.log_guid
    }

    /// 已刷写的文件偏移量（MS-VHDX §2.3.1.1）
    #[must_use]
    pub const fn flushed_file_offset(&self) -> u64 {
        self.flushed_file_offset
    }

    /// 最后写入的文件偏移量（MS-VHDX §2.3.1.1）
    #[must_use]
    pub const fn last_file_offset(&self) -> u64 {
        self.last_file_offset
    }
}

/// 日志描述符（MS-VHDX §2.3.1.2/§2.3.1.3）
///
/// 描述日志条目要执行的操作类型。
/// 根据签名区分：
/// - "desc" → 数据描述符（写入数据）
/// - "zero" → 零描述符（写入零填充）
#[derive(Debug)]
pub enum Descriptor<'a> {
    /// 数据描述符 — 将数据扇区写入指定文件偏移
    Data(DataDescriptor<'a>),
    /// 零描述符 — 在指定偏移处写入零填充
    Zero(ZeroDescriptor<'a>),
}

impl<'a> Descriptor<'a> {
    /// 根据签名解析描述符类型
    ///
    /// 根据前 4 字节签名判断是数据描述符（"desc"）还是零描述符（"zero"）。
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted("Descriptor too small".to_string()));
        }

        let signature = &data[0..4];
        if signature == DATA_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Data(DataDescriptor::new(data)?))
        } else if signature == ZERO_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Zero(ZeroDescriptor::new(data)?))
        } else {
            Err(Error::InvalidSignature {
                expected: "desc or zero".to_string(),
                found: String::from_utf8_lossy(signature).to_string(),
            })
        }
    }

    /// 返回描述符的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        match self {
            Descriptor::Data(d) => d.raw(),
            Descriptor::Zero(z) => z.raw(),
        }
    }
}

/// 数据描述符（MS-VHDX §2.3.1.3）
///
/// 描述一个扇区级别的数据合并写入操作。
///
/// 回放语义（MS-VHDX §2.3.3）：
/// - `trailing_bytes`：目标范围末尾需保留（不覆盖）的字节数（描述符偏移 4-8）
/// - `leading_bytes`：目标范围开头需保留（不覆盖）的字节数（描述符偏移 8-16）
/// - `file_offset`：目标写入起始位置（描述符偏移 16-24）
/// - `sequence_number`：序列号，用于与数据扇区匹配（描述符偏移 24-32）
///
/// 有效数据长度 = `4084 - leading_bytes - trailing_bytes`，取自数据扇区 payload
/// 开头，写入到 `file_offset + leading_bytes` 处。
#[derive(Debug)]
pub struct DataDescriptor<'a> {
    /// 描述符签名（应为 "desc"）
    pub signature: [u8; 4],
    /// 目标范围末尾需保留的字节数
    pub trailing_bytes: u32,
    /// 目标范围开头需保留的字节数
    pub leading_bytes: u64,
    /// 目标文件偏移
    pub file_offset: u64,
    /// 序列号
    pub sequence_number: u64,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> DataDescriptor<'a> {
    /// 从原始字节切片解析数据描述符
    ///
    /// 要求数据长度至少 32 字节。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Data Descriptor too small".to_string(),
            ));
        }
        Ok(Self {
            signature: read_array::<4>(data, 0),
            trailing_bytes: read_u32(data, 4),
            leading_bytes: read_u64(data, 8),
            file_offset: read_u64(data, 16),
            sequence_number: read_u64(data, 24),
            raw: data,
        })
    }

    /// 返回描述符的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 目标范围末尾需保留的字节数（MS-VHDX §2.3.1.3）
    ///
    /// 回放时，目标范围内最后 `trailing_bytes` 字节不被覆盖。
    #[must_use]
    pub const fn trailing_bytes(&self) -> u32 {
        self.trailing_bytes
    }

    /// 目标范围开头需保留的字节数（MS-VHDX §2.3.1.3）
    ///
    /// 回放时，目标范围内前 `leading_bytes` 字节不被覆盖。
    /// 有效数据从数据扇区 payload 开头取，写入到 `file_offset + leading_bytes`。
    #[must_use]
    pub const fn leading_bytes(&self) -> u64 {
        self.leading_bytes
    }

    /// 目标文件写入偏移量（MS-VHDX §2.3.1.3）
    #[must_use]
    pub const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    /// 序列号（MS-VHDX §2.3.1.3），用于与数据扇区匹配
    #[must_use]
    pub const fn sequence_number(&self) -> u64 {
        self.sequence_number
    }
}

/// 零描述符（MS-VHDX §2.3.1.2）
///
/// 描述一个零填充操作。在指定偏移处写入指定长度的零字节。
#[derive(Debug)]
pub struct ZeroDescriptor<'a> {
    /// 描述符签名（应为 "zero"）
    pub signature: [u8; 4],
    /// 保留字段
    pub reserved: u32,
    /// 零填充长度
    pub zero_length: u64,
    /// 目标文件偏移
    pub file_offset: u64,
    /// 序列号
    pub sequence_number: u64,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> ZeroDescriptor<'a> {
    /// 从原始字节切片解析零描述符
    ///
    /// 要求数据长度至少 32 字节。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Zero Descriptor too small".to_string(),
            ));
        }
        Ok(Self {
            signature: read_array::<4>(data, 0),
            reserved: read_u32(data, 4),
            zero_length: read_u64(data, 8),
            file_offset: read_u64(data, 16),
            sequence_number: read_u64(data, 24),
            raw: data,
        })
    }

    /// 返回描述符的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 零填充长度（MS-VHDX §2.3.1.2）
    #[must_use]
    pub const fn zero_length(&self) -> u64 {
        self.zero_length
    }

    /// 目标文件写入偏移量（MS-VHDX §2.3.1.2）
    #[must_use]
    pub const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    /// 序列号（MS-VHDX §2.3.1.2）
    #[must_use]
    pub const fn sequence_number(&self) -> u64 {
        self.sequence_number
    }
}

/// 数据扇区（MS-VHDX §2.3.1.4）
///
/// 固定 4KB 大小，包含日志条目的实际数据负载。
///
/// 序列号被拆分为两部分存储以检测撕裂写入（torn writes）：
/// - 前 4 字节为签名（DataSignature）
/// - 字节 4-8 为序列号高 32 `位（sequence_high`）
/// - 字节 8-4092 为数据内容
/// - 字节 4092-4096 为序列号低 32 `位（sequence_low`）
///
/// 如果 `sequence_high` ≠ `sequence_low，说明写入不完整（撕裂写入`）。
pub struct DataSector<'a> {
    /// 数据扇区签名
    pub signature: [u8; 4],
    /// 序列号高 32 位
    pub sequence_high: u32,
    /// 数据内容（字节 8..4092）
    pub data: &'a [u8],
    /// 序列号低 32 位
    pub sequence_low: u32,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> DataSector<'a> {
    /// 从原始字节切片解析数据扇区
    ///
    /// 要求数据长度恰好为 4096 字节（`DATA_SECTOR_SIZE`）。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != DATA_SECTOR_SIZE {
            return Err(Error::InvalidFile(format!(
                "Data Sector must be {} bytes, got {}",
                DATA_SECTOR_SIZE,
                data.len()
            )));
        }
        Ok(Self {
            signature: read_array::<4>(data, 0),
            sequence_high: read_u32(data, 4),
            data: &data[8..4092],
            sequence_low: read_u32(data, 4092),
            raw: data,
        })
    }

    /// 返回数据扇区的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 序列号高 32 位（MS-VHDX §2.3.1.4）
    ///
    /// 位于数据扇区字节偏移 4-8，用于撕裂写入检测。
    #[must_use]
    pub const fn sequence_high(&self) -> u32 {
        self.sequence_high
    }

    /// 返回数据扇区的实际数据内容（字节 8-4092）
    #[must_use]
    pub const fn data(&self) -> &[u8] {
        self.data
    }

    /// 序列号低 32 位（MS-VHDX §2.3.1.4）
    ///
    /// 位于数据扇区字节偏移 4092-4096，用于撕裂写入检测。
    #[must_use]
    pub const fn sequence_low(&self) -> u32 {
        self.sequence_low
    }

    /// 组合高低 32 位序列号，用于验证数据完整性（MS-VHDX §2.3.1.4）
    ///
    /// 如果高 32 位与低 32 位不一致，说明发生了撕裂写入（torn write）。
    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        (u64::from(self.sequence_high()) << 32) | u64::from(self.sequence_low())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_log_entry(entry_length: u32, sequence_number: u64) -> Vec<u8> {
        let mut entry = vec![0u8; usize::try_from(entry_length).unwrap_or(0)];
        entry[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        entry[8..12].copy_from_slice(&entry_length.to_le_bytes());
        entry[16..24].copy_from_slice(&sequence_number.to_le_bytes());
        entry
    }

    #[test]
    fn test_log_entry_header() {
        let mut data = [0u8; 64];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        data[4..8].copy_from_slice(&0x1234_5678_u32.to_le_bytes());
        data[8..12].copy_from_slice(&0x1000_u32.to_le_bytes());
        data[16..24].copy_from_slice(&0x1_u64.to_le_bytes());
        data[24..28].copy_from_slice(&2_u32.to_le_bytes());

        let header = LogEntryHeader::new(&data);
        assert_eq!(header.signature(), LOG_ENTRY_SIGNATURE);
        assert_eq!(header.checksum(), 0x1234_5678);
        assert_eq!(header.entry_length(), 0x1000);
        assert_eq!(header.sequence_number(), 1);
        assert_eq!(header.descriptor_count(), 2);
    }

    #[test]
    fn test_data_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        data[4..8].copy_from_slice(&0x100_u32.to_le_bytes());
        data[8..16].copy_from_slice(&0x200_u64.to_le_bytes());
        data[16..24].copy_from_slice(&0x0010_0000_u64.to_le_bytes());
        data[24..32].copy_from_slice(&0x1_u64.to_le_bytes());

        let desc = DataDescriptor::new(&data).unwrap();
        assert_eq!(desc.trailing_bytes(), 0x100);
        assert_eq!(desc.leading_bytes(), 0x200);
        assert_eq!(desc.file_offset(), 0x0010_0000);
        assert_eq!(desc.sequence_number(), 1);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        data[8..16].copy_from_slice(&0x1000_u64.to_le_bytes());
        data[16..24].copy_from_slice(&0x0020_0000_u64.to_le_bytes());

        let desc = ZeroDescriptor::new(&data).unwrap();
        assert_eq!(desc.zero_length(), 0x1000);
        assert_eq!(desc.file_offset(), 0x0020_0000);
    }

    #[test]
    fn test_log_entry_index_matches_entries_order() {
        let mut raw = Vec::new();
        raw.extend(build_log_entry(64, 11));
        raw.extend(build_log_entry(64, 22));

        let log = Log::new(raw);
        let entries = log.entries();

        let indexed = log.entry(1).expect("entry(1) should exist");
        let by_entries = entries.get(1).expect("entries().get(1) should exist");

        assert_eq!(
            indexed.header().sequence_number(),
            by_entries.header().sequence_number()
        );
        assert_eq!(
            indexed.header().entry_length(),
            by_entries.header().entry_length()
        );
    }

    #[test]
    fn test_log_entry_out_of_range_returns_none() {
        let mut raw = Vec::new();
        raw.extend(build_log_entry(64, 1));

        let log = Log::new(raw);

        assert!(log.entry(1).is_none());
    }
}
