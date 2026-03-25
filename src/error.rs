//! Error types for VHDX operations

use std::path::PathBuf;
use thiserror::Error;

use crate::types::Guid;

/// Result type alias for VHDX operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for VHDX operations
#[derive(Error, Debug)]
pub enum Error {
    /// IO error from underlying file operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// File is locked by another process (Windows-specific)
    #[error(
        "File is locked by another process. The VHDX file may be mounted or in use by another application. Close any applications using this file and try again."
    )]
    FileLocked,

    /// Invalid VHDX file format
    #[error("Invalid VHDX file: {0}")]
    InvalidFile(String),

    /// Corrupted header section
    #[error("Corrupted header: {0}")]
    CorruptedHeader(String),

    /// Invalid checksum
    #[error("Invalid checksum: expected {expected:08x}, actual {actual:08x}")]
    InvalidChecksum { expected: u32, actual: u32 },

    /// Unsupported version
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u16),

    /// Invalid block state
    #[error("Invalid block state: {0}")]
    InvalidBlockState(u8),

    /// Parent disk not found
    #[error("Parent disk not found: {path}")]
    ParentNotFound { path: PathBuf },

    /// Parent disk GUID mismatch
    #[error("Parent disk mismatch: expected {expected}, actual {actual}")]
    ParentMismatch { expected: Guid, actual: Guid },

    /// Log replay required
    #[error(
        "Log replay required: The VHDX file has pending changes from an interrupted write. Open with write access to replay the log and recover the file."
    )]
    LogReplayRequired,

    /// Invalid parameter
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// Metadata item not found
    #[error("Metadata not found: {guid}")]
    MetadataNotFound { guid: Guid },

    /// File is read-only
    #[error("File is read-only")]
    ReadOnly,

    /// Invalid signature
    #[error("Invalid signature: expected '{expected}', found '{found}'")]
    InvalidSignature { expected: String, found: String },

    /// BAT entry not found
    #[error("BAT entry not found at index {index}")]
    BatEntryNotFound { index: u64 },

    /// Invalid region table
    #[error("Invalid region table: {0}")]
    InvalidRegionTable(String),

    /// Invalid metadata
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    /// Log entry corrupted
    #[error("Log entry corrupted: {0}")]
    LogEntryCorrupted(String),

    /// Sector out of bounds
    #[error("Sector {sector} out of bounds (max: {max})")]
    SectorOutOfBounds { sector: u64, max: u64 },

    /// Block not allocated
    #[error("Block {block_idx} not allocated (state: {state:?})")]
    BlockNotPresent { block_idx: u64, state: String },
}
