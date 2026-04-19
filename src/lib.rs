//! # vhdx-rs
//!
//! VHDX (Virtual Hard Disk v2) 文件格式的 Rust 库。
//!
//! 本库提供了完整的 VHDX 虚拟硬盘文件格式支持，包括：
//!
//! - **打开现有 VHDX 文件** — 验证签名、解析头部和区域表、读取元数据
//! - **创建新 VHDX 文件** — 支持 Fixed、Dynamic、Differencing 三种类型
//! - **读写虚拟磁盘数据** — 扇区级和块级 IO 操作
//! - **日志回放** — 自动检测并回放未完成的事务日志
//!
//! # 支持的 VHDX 类型
//!
//! | 类型 | 说明 |
//! |------|------|
//! | **Fixed** | 固定大小虚拟磁盘，数据连续存储 |
//! | **Dynamic** | 动态分配虚拟磁盘，按需分配数据块 |
//! | **Differencing** | 差分虚拟磁盘，引用父磁盘实现快照 |
//!
//! # 主要类型
//!
//! - [`File`] — VHDX 文件句柄，提供打开、创建、读写操作
//! - [`IO`] — 扇区/块级 IO 操作接口
//! - [`Sections`] — 各区域（头部、BAT、元数据、日志）的延迟加载容器
//! - [`Error`] — 统一错误类型
//!
//! # 参考规范
//!
//! 实现基于 Microsoft VHDX 规范（MS-VHDX）。

// 错误类型
pub use error::{Error, Result};
// GUID 类型
pub use types::Guid;
// 规范校验类型
pub use validation::{SpecValidator, ValidationIssue};

/// 规范一致性校验模块
pub mod validation;

/// Section 模块命名空间
///
/// 所有 VHDX 区域类型均在此模块下可访问：
/// - [`Bat`] / [`BatEntry`] — 块分配表
/// - [`Header`] / [`HeaderStructure`] — 头部结构
/// - [`Metadata`] / [`MetadataTable`] / [`FileParameters`] — 元数据
/// - [`Log`] / [`LogEntry`] / [`LogEntryHeader`] — 日志
/// - 以及所有关联的描述符、标志位和辅助类型
pub mod section {
    pub use crate::sections::{
        Bat, BatEntry, BatState, DataDescriptor, DataSector, Descriptor, EntryFlags,
        FileParameters, FileTypeIdentifier, Header, HeaderStructure, KeyValueEntry, LocatorHeader,
        Log, LogEntry, LogEntryHeader, Metadata, MetadataItems, MetadataTable, ParentLocator,
        PayloadBlockState, RegionTable, RegionTableEntry, RegionTableHeader, Sections,
        SectorBitmapState, TableEntry, TableHeader, ZeroDescriptor,
    };
}

// IO 抽象类型
pub use io_module::{IO, PayloadBlock, Sector};

// 文件操作类型
pub use file::{CreateOptions, File, LogReplayPolicy, OpenOptions, ParentChainInfo};

// 区域初始化配置
pub use sections::SectionsConfig;

// CRC32C 校验辅助函数
pub use sections::crc32c_with_zero_field;

/// VHDX 格式常量与 GUID 命名空间
///
/// 提供对 VHDX 布局常量、签名、块大小限制、对齐函数及已知区域/元数据 GUID 的访问。
pub mod constants {
    pub use crate::common::constants::*;
}

// 内部模块
mod common;
mod error;
mod file;
mod io_module;
mod sections;
mod types;
