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

    #[error("Parent GUID mismatch")]
    ParentGuidMismatch,

    #[error("Invalid sector bitmap")]
    InvalidSectorBitmap,

    #[error("Invalid virtual offset: {0}")]
    InvalidOffset(u64),

    #[error("File too small")]
    FileTooSmall,

    #[error("Alignment error: {0} is not aligned to {1}")]
    Alignment(u64, u64),
}
