//! VHDX 区域聚合模块
//!
//! 本模块是 VHDX 文件各区域的统一入口（MS-VHDX §2.1），提供：
//! - 子模块的导出（header、bat、log、metadata）
//! - 包装结构体（[`Metadata`]、[`Log`]），提供与子模块同名类型的外部接口
//! - 延迟加载的 [`Sections`] 结构体，按需从文件读取各区域数据
//! - CRC32C 校验辅助函数 [`crc32c_with_zero_field`]
//!
//! # 区域概述
//!
//! VHDX 文件由以下主要区域组成：
//! - **头部区域**（1MB）— 文件类型标识符、头部结构、区域表
//! - **BAT 区域** — 块分配表，映射虚拟块到文件偏移
//! - **元数据区域** — 虚拟磁盘参数和配置
//! - **日志区域** — 事务日志，用于崩溃恢复

use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom};

use crate::common::constants::HEADER_SECTION_SIZE;
use crate::error::{Error, Result};

// 子模块声明：每个子模块对应 VHDX 文件的一个区域
mod bat;
mod header;
mod log;
mod metadata;

// 公开导出子模块中的类型，供外部使用
pub use bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};
pub use header::{
    FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry, RegionTableHeader,
};
pub use log::{DataDescriptor, DataSector, Descriptor, Entry, LogEntryHeader, ZeroDescriptor};
pub use metadata::{
    EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, MetadataItems, MetadataTable,
    ParentLocator, TableEntry, TableHeader,
};

/// 元数据区域的外部包装类型
///
/// 包装内部的元数据实现，提供统一的外部接口。
/// 注意：此类型与子模块中的 `Metadata` 同名但不同。
pub struct Metadata {
    /// 内部元数据实现
    inner: metadata::Metadata,
}

impl Metadata {
    /// 从原始字节数据创建元数据包装实例
    ///
    /// 将数据传递给内部 `metadata::Metadata::new()` 进行解析。
    pub fn new(data: Vec<u8>) -> Result<Self> {
        Ok(Self {
            inner: metadata::Metadata::new(data)?,
        })
    }

    /// 获取元数据区域的原始字节数据
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    /// 获取元数据表的解析视图
    ///
    /// 返回对元数据表头和表项的借用引用。
    #[must_use]
    pub fn table(&self) -> crate::sections::metadata::MetadataTable<'_> {
        self.inner.table()
    }

    /// 获取已解析的元数据项集合
    ///
    /// 包含虚拟磁盘大小、块大小、磁盘类型等关键参数。
    #[must_use]
    pub fn items(&self) -> MetadataItems<'_> {
        self.inner.items()
    }
}

/// 日志区域的外部包装类型
///
/// 包装内部的日志实现，提供统一的外部接口。
/// 注意：此类型与子模块中的 `Log` 同名但不同。
pub struct Log {
    /// 内部日志实现
    inner: log::Log,
}

impl Log {
    /// 从原始字节数据创建日志包装实例
    ///
    /// 直接将数据传递给内部 `log::Log::new()`，不进行解析
    /// （日志条目在访问时按需解析）。
    #[must_use]
    pub const fn new(data: Vec<u8>) -> Self {
        Self {
            inner: log::Log::new(data),
        }
    }

    /// 获取日志区域的原始字节数据
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        self.inner.raw()
    }

    /// 按索引获取指定日志条目
    ///
    /// 返回 `None` 表示索引超出范围或该条目为空条目。
    #[must_use]
    pub const fn entry(&self, index: usize) -> Option<Entry<'_>> {
        self.inner.entry(index)
    }

    /// 获取所有有效（非空）的日志条目
    #[must_use]
    pub fn entries(&self) -> Vec<Entry<'_>> {
        self.inner.entries()
    }

    /// 检查是否需要重放日志
    ///
    /// 根据 VHDX 规范，当日志中存在未提交的事务时需要重放。
    #[must_use]
    pub fn is_replay_required(&self) -> bool {
        self.inner.is_replay_required()
    }

    /// 将日志条目重放到目标文件
    ///
    /// 遍历日志条目，将数据扇区写回文件中对应的偏移位置，
    /// 用于崩溃恢复（MS-VHDX §2.3.4）。
    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        self.inner.replay(file)
    }
}

/// Sections 初始化配置
///
/// 包含各区域在文件中的偏移量、大小和文件句柄，
/// 用于创建 [`Sections`] 实例时提供必要的定位信息。
pub struct SectionsConfig {
    /// VHDX 文件句柄
    pub file: std::fs::File,
    /// BAT 区域在文件中的偏移量（字节）
    pub bat_offset: u64,
    /// BAT 区域的大小（字节）
    pub bat_size: u64,
    /// 元数据区域在文件中的偏移量（字节）
    pub metadata_offset: u64,
    /// 元数据区域的大小（字节）
    pub metadata_size: u64,
    /// 日志区域在文件中的偏移量（字节）
    pub log_offset: u64,
    /// 日志区域的大小（字节）
    pub log_size: u64,
    /// BAT 条目总数
    pub entry_count: u64,
}

/// VHDX 文件各区域的延迟加载容器
///
/// 使用 `RefCell<Option<T>>` 模式实现延迟加载：
/// 每个区域首次访问时从文件读取并缓存，后续访问直接返回缓存数据。
///
/// 注意：此类型不是线程安全的（使用 `RefCell` 而非 `Mutex`），
/// 因为 VHDX 文件操作通常是单线程的。
pub struct Sections {
    /// VHDX 文件句柄（用于按需读取区域数据）
    file: std::fs::File,

    /// 头部区域（延迟加载，首次访问时从文件读取）
    header: RefCell<Option<Header>>,
    /// BAT 区域（延迟加载）
    bat: RefCell<Option<Bat>>,
    /// 元数据区域（延迟加载）
    metadata: RefCell<Option<Metadata>>,
    /// 日志区域（延迟加载）
    log: RefCell<Option<Log>>,

    /// BAT 区域在文件中的偏移量
    bat_offset: u64,
    /// BAT 区域的大小
    bat_size: u64,
    /// 元数据区域在文件中的偏移量
    metadata_offset: u64,
    /// 元数据区域的大小
    metadata_size: u64,
    /// 日志区域在文件中的偏移量
    log_offset: u64,
    /// 日志区域的大小
    log_size: u64,

    /// BAT 条目总数
    entry_count: u64,
}

impl Sections {
    /// 从配置创建 Sections 实例，所有区域初始化为未加载状态
    ///
    /// 仅保存配置信息（偏移量、大小等），不执行任何文件 I/O。
    /// 各区域数据在首次调用对应的 getter 方法时才会从文件读取。
    #[must_use]
    pub fn new(config: SectionsConfig) -> Self {
        Self {
            file: config.file,
            // 所有区域初始化为 None，等待延迟加载
            header: RefCell::new(None),
            bat: RefCell::new(None),
            metadata: RefCell::new(None),
            log: RefCell::new(None),
            bat_offset: config.bat_offset,
            bat_size: config.bat_size,
            metadata_offset: config.metadata_offset,
            metadata_size: config.metadata_size,
            log_offset: config.log_offset,
            log_size: config.log_size,
            entry_count: config.entry_count,
        }
    }

    /// 获取头部区域（延迟加载）
    ///
    /// 首次调用时从文件读取 1MB 头部区域数据并缓存。
    /// 后续调用直接返回缓存的引用。
    pub fn header(&self) -> Result<std::cell::Ref<'_, Header>> {
        if self.header.borrow().is_none() {
            // 从文件起始位置读取完整的头部区域（1MB）
            let header_data = self.read_header_section()?;
            *self.header.borrow_mut() = Some(Header::new(header_data)?);
        }
        // 将 Option<Header> 映射为 &Header，解包安全（刚确认是 Some）
        Ok(std::cell::Ref::map(self.header.borrow(), |h| {
            h.as_ref().unwrap()
        }))
    }

    /// 获取 BAT 区域（延迟加载），首次调用时从文件读取并缓存
    pub fn bat(&self) -> Result<std::cell::Ref<'_, Bat>> {
        if self.bat.borrow().is_none() {
            // 将 u64 大小转换为 usize，防止溢出
            let bat_size: usize = self.bat_size.try_into().map_err(|_| {
                Error::InvalidFile(format!("BAT size {} exceeds usize::MAX", self.bat_size))
            })?;
            let bat_data = self.read_section(self.bat_offset, bat_size)?;
            *self.bat.borrow_mut() = Some(Bat::new(bat_data, self.entry_count)?);
        }
        Ok(std::cell::Ref::map(self.bat.borrow(), |b| {
            b.as_ref().unwrap()
        }))
    }

    /// 获取元数据区域（延迟加载），首次调用时从文件读取并缓存
    pub fn metadata(&self) -> Result<std::cell::Ref<'_, Metadata>> {
        if self.metadata.borrow().is_none() {
            // 将 u64 大小转换为 usize，防止溢出
            let metadata_size: usize = self.metadata_size.try_into().map_err(|_| {
                Error::InvalidFile(format!(
                    "Metadata size {} exceeds usize::MAX",
                    self.metadata_size
                ))
            })?;
            let metadata_data = self.read_section(self.metadata_offset, metadata_size)?;
            *self.metadata.borrow_mut() = Some(Metadata::new(metadata_data)?);
        }
        Ok(std::cell::Ref::map(self.metadata.borrow(), |m| {
            m.as_ref().unwrap()
        }))
    }

    /// 获取日志区域（延迟加载），首次调用时从文件读取并缓存
    pub fn log(&self) -> Result<std::cell::Ref<'_, Log>> {
        if self.log.borrow().is_none() {
            // 将 u64 大小转换为 usize，防止溢出
            let log_size: usize = self.log_size.try_into().map_err(|_| {
                Error::InvalidFile(format!("Log size {} exceeds usize::MAX", self.log_size))
            })?;
            let log_data = self.read_section(self.log_offset, log_size)?;
            // Log::new 是 const fn，不会失败
            *self.log.borrow_mut() = Some(Log::new(log_data));
        }
        Ok(std::cell::Ref::map(self.log.borrow(), |l| {
            l.as_ref().unwrap()
        }))
    }

    /// 从文件读取头部区域（前 1MB）
    fn read_header_section(&self) -> Result<Vec<u8>> {
        // 头部区域始终从文件偏移 0 开始，大小为 HEADER_SECTION_SIZE（1MB）
        self.read_section(0, HEADER_SECTION_SIZE)
    }

    /// 从文件指定偏移量读取指定大小的数据
    ///
    /// 使用 `try_clone()` 复制文件句柄，避免修改原始句柄的读写位置。
    fn read_section(&self, offset: u64, size: usize) -> Result<Vec<u8>> {
        // 克隆文件句柄，使 seek 操作不影响其他延迟加载调用
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; size];
        file.read_exact(&mut data)?;
        Ok(data)
    }
}

/// 计算 CRC32C 校验和，计算前将指定字段置零
///
/// VHDX 格式中，校验和字段的计算规则为：将校验和字段本身置零后
/// 对整个结构计算 CRC32C（MS-VHDX §2.2.2、§2.3.1.1）。
///
/// # 参数
/// - `data` — 完整的数据（含校验和字段）
/// - `zero_offset` — 需要置零的字段起始偏移
/// - `zero_len` — 需要置零的字段长度
pub fn crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32 {
    // 复制数据，避免修改原始输入
    let mut data_copy = data.to_vec();
    // 将校验和字段区域置零
    for i in zero_offset..(zero_offset + zero_len).min(data_copy.len()) {
        data_copy[i] = 0;
    }
    // 对修改后的数据计算 CRC32C
    crc32c::crc32c(&data_copy)
}
