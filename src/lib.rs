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

// 区域和结构解析类型
pub use sections::{
    Bat, BatEntry, BatState, DataDescriptor, DataSector, Descriptor, EntryFlags, FileParameters,
    FileTypeIdentifier, Header, HeaderStructure, KeyValueEntry, LocatorHeader, Log, LogEntry,
    LogEntryHeader, Metadata, MetadataItems, MetadataTable, ParentLocator, PayloadBlockState,
    RegionTable, RegionTableEntry, RegionTableHeader, Sections, SectorBitmapState, TableEntry,
    TableHeader, ZeroDescriptor,
};

// IO 抽象类型
pub use io_module::{IO, PayloadBlock, Sector};

// 文件操作类型
pub use file::File;

// 内部模块
mod common;
mod error;
mod file;
mod io_module;
mod sections;
mod types;
