//! VHDX 文件操作错误类型定义
//!
//! 本模块定义了 VHDX 文件操作过程中可能产生的所有错误类型。
//! 使用 [`thiserror`] 库实现统一的错误处理，所有错误均实现 [`std::error::Error`] trait。
//!
//! # 错误分类
//!
//! - **IO 错误**: 底层文件系统错误
//! - **格式错误**: 文件格式验证失败（签名、校验和、版本等）
//! - **状态错误**: 数据结构状态不一致（块状态、日志回放等）
//! - **参数错误**: 用户提供的参数无效

use std::path::PathBuf;
use thiserror::Error;

use crate::types::Guid;

/// VHDX 操作的统一结果类型
///
/// 所有 VHDX 库函数均返回此类型，错误时包含 [`Error`] 枚举。
pub type Result<T> = std::result::Result<T, Error>;

/// VHDX 文件操作的统一错误类型
///
/// 涵盖了从底层 IO 到格式验证、状态管理的所有错误场景。
/// 每个变体对应一种具体的错误情况。
#[derive(Error, Debug)]
pub enum Error {
    /// 底层 IO 错误
    ///
    /// 包装 [`std::io::Error`]，来自文件读写操作。
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 文件被其他进程锁定
    ///
    /// 当 VHDX 文件被挂载或被其他应用程序占用时触发。
    /// 需要关闭占用文件的应用程序后重试。
    #[error(
        "File is locked by another process. The VHDX file may be mounted or in use by another application. Close any applications using this file and try again."
    )]
    FileLocked,

    /// 无效的 VHDX 文件
    ///
    /// 文件不符合 VHDX 格式要求，如文件过小或结构异常。
    #[error("Invalid VHDX file: {0}")]
    InvalidFile(String),

    /// 头部结构损坏
    ///
    /// VHDX 头部数据损坏或不一致（MS-VHDX §2.2.2），
    /// 如两个头部均无法通过校验。
    #[error("Corrupted header: {0}")]
    CorruptedHeader(String),

    /// 校验和验证失败
    ///
    /// 结构的 CRC32C 校验和不匹配（MS-VHDX §2.2.2、§2.2.3.1、§2.3.1.1）。
    /// 通常表示文件存在数据损坏。
    #[error("Invalid checksum: expected {expected:08x}, actual {actual:08x}")]
    InvalidChecksum { expected: u32, actual: u32 },

    /// 不支持的 VHDX 版本
    ///
    /// 遇到了库不支持的 VHDX 版本号。
    /// 当前仅支持版本 1（MS-VHDX §2.2.2）。
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u16),

    /// 无效的块状态值
    ///
    /// BAT 条目中遇到了未定义的块状态值（MS-VHDX §2.5.1）。
    /// 有效的 Payload 块状态为 0-7，扇区位图状态为 0-3。
    #[error("Invalid block state: {0}")]
    InvalidBlockState(u8),

    /// 父磁盘未找到
    ///
    /// 差分 VHDX 文件引用的父磁盘文件不存在（MS-VHDX §2.6.2.6）。
    #[error("Parent disk not found: {path}")]
    ParentNotFound { path: PathBuf },

    /// 父磁盘不匹配
    ///
    /// 差分 VHDX 文件引用的父磁盘 GUID 与实际文件不匹配（MS-VHDX §2.6.2.6）。
    #[error("Parent disk mismatch: expected {expected}, actual {actual}")]
    ParentMismatch { expected: Guid, actual: Guid },

    /// 需要日志回放
    ///
    /// VHDX 文件存在未完成的事务日志，需要先回放日志以恢复一致性（MS-VHDX §2.3.3）。
    /// 可通过 `vhdx-tool repair` 命令触发日志回放。
    #[error(
        "Log replay required: The VHDX file has pending changes from an interrupted write. Run 'vhdx-tool repair <file>' to replay the log and recover the file."
    )]
    LogReplayRequired,

    /// 无效的参数
    ///
    /// 用户提供的参数不符合要求，如块大小不在有效范围内。
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// 元数据项未找到
    ///
    /// 在元数据表中未找到指定 GUID 的元数据项（MS-VHDX §2.6.1）。
    #[error("Metadata not found: {guid}")]
    MetadataNotFound { guid: Guid },

    /// 文件为只读模式
    ///
    /// 尝试对以只读模式打开的文件执行写操作。
    #[error("File is read-only")]
    ReadOnly,

    /// 无效的签名
    ///
    /// 结构的签名不匹配，表示文件格式错误或数据损坏。
    #[error("Invalid signature: expected '{expected}', found '{found}'")]
    InvalidSignature { expected: String, found: String },

    /// BAT 条目未找到
    ///
    /// 指定索引处的 BAT 条目不存在或未分配。
    #[error("BAT entry not found at index {index}")]
    BatEntryNotFound { index: u64 },

    /// 无效的区域表
    ///
    /// 区域表数据不一致或格式错误（MS-VHDX §2.2.3）。
    #[error("Invalid region table: {0}")]
    InvalidRegionTable(String),

    /// 无效的元数据
    ///
    /// 元数据内容不符合格式要求（MS-VHDX §2.6）。
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    /// 日志条目损坏
    ///
    /// 日志条目的数据损坏或校验失败（MS-VHDX §2.3.1）。
    #[error("Log entry corrupted: {0}")]
    LogEntryCorrupted(String),

    /// 扇区索引超出范围
    ///
    /// 请求访问的扇区索引超过了虚拟磁盘的最大扇区数。
    #[error("Sector {sector} out of bounds (max: {max})")]
    SectorOutOfBounds { sector: u64, max: u64 },

    /// 数据块未分配
    ///
    /// 请求访问的数据块处于未分配状态（如 `NotPresent` 或 Zero 状态），
    /// 无法提供实际数据（MS-VHDX §2.5.1.1）。
    #[error("Block {block_idx} not allocated (state: {state:?})")]
    BlockNotPresent { block_idx: u64, state: String },
}
