//! Error types for VHDX operations

use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, VhdxError>;

#[derive(Error, Debug)]
pub enum VhdxError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid signature: expected {expected:?}, got {got:?}")]
    InvalidSignature { expected: String, got: String },

    #[error("Invalid checksum")]
    InvalidChecksum,

    #[error("Invalid file type identifier")]
    InvalidFileType,

    #[error("No valid header found")]
    NoValidHeader,

    #[error("Corrupt VHDX file: {0}")]
    Corrupt(String),

    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),

    #[error("Invalid region: {0}")]
    InvalidRegion(String),

    #[error("Required region not found: {0}")]
    RequiredRegionNotFound(String),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Invalid BAT entry")]
    InvalidBatEntry,

    #[error("Block not present")]
    BlockNotPresent,

    #[error("Log replay failed: {0}")]
    LogReplayFailed(String),

    #[error("Invalid log entry")]
    InvalidLogEntry,

    #[error("Parent disk not found: {0}")]
    ParentNotFound(String),

    #[error("Parent GUID mismatch: expected {expected}, found {found}")]
    ParentGuidMismatch { expected: String, found: String },

    #[error("Invalid parent locator: {0}")]
    InvalidParentLocator(String),

    #[error("Parent/child sector size mismatch: parent={parent}, child={child}")]
    SectorSizeMismatch { parent: u32, child: u32 },

    #[error("Invalid sector bitmap")]
    InvalidSectorBitmap,

    #[error("Invalid virtual offset: {0}")]
    InvalidOffset(u64),

    #[error("File too small: {0}")]
    FileTooSmall(String),

    #[error("Alignment error: {0} is not aligned to {1}")]
    Alignment(u64, u64),

    #[error("Unknown required metadata item: {guid}")]
    UnknownRequiredMetadata { guid: String },

    #[error("Circular parent chain detected")]
    CircularParentChain,

    #[error("Parent chain too deep: {depth} (max 16)")]
    ParentChainTooDeep { depth: usize },

    #[error("Invalid parent path: {0}")]
    InvalidParentPath(String),

    #[error("Invalid block size: {0} (must be power of 2, 1MB-256MB)")]
    InvalidBlockSize(u32),

    #[error("Invalid disk size: {size} (must be {min}-{max} bytes and sector-aligned)")]
    InvalidDiskSize { size: u64, min: u64, max: u64 },

    #[error("Invalid file: {0}")]
    InvalidFile(String),

    #[error("Header inconsistency detected: {0}")]
    HeaderInconsistent(String),
}
