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

/// VHDX 日志区域（MS-VHDX §2.3）
///
/// 包装日志区域的原始数据，提供对日志条目的解析和回放功能。
/// 日志区域在文件中由头部结构的 `log_offset` 和 `log_length` 字段定位。
pub struct Log<'a> {
    /// 日志区域的原始字节数据
    raw_data: Vec<u8>,
    marker: PhantomData<&'a [u8]>,
}

impl<'a> Log<'a> {
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
    /// 如果日志区域中包含至少一个有效条目，则返回 `true`。
    #[must_use]
    pub fn is_replay_required(&self) -> bool {
        !self.entries().is_empty()
    }

    /// 回放日志条目以恢复文件一致性（MS-VHDX §2.3.3）
    ///
    /// 回放流程：
    /// 1. 解析所有日志条目
    /// 2. 对每个条目，验证签名
    /// 3. 遍历描述符：
    ///    - **数据描述符**：将数据扇区写入指定文件偏移，
    ///      并在前后填充零字节（leading_bytes / trailing_bytes）
    ///    - **零描述符**：在指定偏移处写入零填充
    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        // 解析所有日志条目
        let entries = self.entries();
        if entries.is_empty() {
            return Ok(());
        }

        for entry in entries {
            let header = entry.header();

            // 验证日志条目签名
            if header.signature() != LOG_ENTRY_SIGNATURE {
                return Err(Error::LogEntryCorrupted(
                    "Invalid log entry signature".to_string(),
                ));
            }

            // 提取描述符和数据扇区
            let descriptors = entry.descriptors();
            let data_sectors = entry.data();
            let mut data_sector_index = 0;

            for desc in descriptors {
                match desc {
                    Descriptor::Data(data_desc) => {
                        // 数据描述符：将对应的数据扇区写入文件指定偏移
                        if data_sector_index < data_sectors.len() {
                            let sector = &data_sectors[data_sector_index];
                            let file_offset = data_desc.file_offset();

                            // 定位到目标文件偏移
                            file.seek(SeekFrom::Start(file_offset))?;
                            let leading = data_desc.leading_bytes();
                            let trailing = data_desc.trailing_bytes();

                            // 先写入前导零字节（leading bytes）
                            if leading > 0 {
                                file.write_all(&vec![0u8; usize::try_from(leading).unwrap_or(0)])?;
                            }
                            // 写入数据扇区的实际内容
                            file.write_all(sector.data())?;
                            // 再写入尾部零字节（trailing bytes）
                            if trailing > 0 {
                                file.write_all(&vec![0u8; usize::try_from(trailing).unwrap_or(0)])?;
                            }

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

        Ok(())
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
/// 描述一个数据写入操作。包含目标文件偏移量和前后填充字节数。
///
/// `leading_bytes` 和 `trailing_bytes` 用于处理写入区域与扇区边界不对齐的情况：
/// - `trailing_bytes`：在数据扇区内容之前写入的零字节数（描述符偏移 4-8）
/// - `leading_bytes`：在数据扇区内容之后写入的零字节数（描述符偏移 8-16）
/// - `file_offset`：目标写入位置（描述符偏移 16-24）
/// - `sequence_number`：序列号，用于与数据扇区匹配（描述符偏移 24-32）
#[derive(Debug)]
pub struct DataDescriptor<'a> {
    /// 描述符签名（应为 "desc"）
    pub signature: [u8; 4],
    /// 尾部填充字节数
    pub trailing_bytes: u32,
    /// 前导填充字节数
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

    /// 尾部填充字节数（MS-VHDX §2.3.1.3）
    ///
    /// 在数据扇区内容之前写入的零字节数。
    #[must_use]
    pub const fn trailing_bytes(&self) -> u32 {
        self.trailing_bytes
    }

    /// 前导填充字节数（MS-VHDX §2.3.1.3）
    ///
    /// 在数据扇区内容之后写入的零字节数。
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
/// - 字节 4-8 为序列号高 32 位（sequence_high）
/// - 字节 8-4092 为数据内容
/// - 字节 4092-4096 为序列号低 32 位（sequence_low）
///
/// 如果 sequence_high ≠ sequence_low，说明写入不完整（撕裂写入）。
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
