use std::path::PathBuf;
use thiserror::Error;

use crate::types::Guid;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "File is locked by another process. The VHDX file may be mounted or in use by another application. Close any applications using this file and try again."
    )]
    FileLocked,

    #[error("Invalid VHDX file: {0}")]
    InvalidFile(String),

    #[error("Corrupted header: {0}")]
    CorruptedHeader(String),

    #[error("Invalid checksum: expected {expected:08x}, actual {actual:08x}")]
    InvalidChecksum { expected: u32, actual: u32 },

    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u16),

    #[error("Invalid block state: {0}")]
    InvalidBlockState(u8),

    #[error("Parent disk not found: {path}")]
    ParentNotFound { path: PathBuf },

    #[error("Parent disk mismatch: expected {expected}, actual {actual}")]
    ParentMismatch { expected: Guid, actual: Guid },

    #[error(
        "Log replay required: The VHDX file has pending changes from an interrupted write. Run 'vhdx-tool repair <file>' to replay the log and recover the file."
    )]
    LogReplayRequired,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Metadata not found: {guid}")]
    MetadataNotFound { guid: Guid },

    #[error("File is read-only")]
    ReadOnly,

    #[error("Invalid signature: expected '{expected}', found '{found}'")]
    InvalidSignature { expected: String, found: String },

    #[error("BAT entry not found at index {index}")]
    BatEntryNotFound { index: u64 },

    #[error("Invalid region table: {0}")]
    InvalidRegionTable(String),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Log entry corrupted: {0}")]
    LogEntryCorrupted(String),

    #[error("Sector {sector} out of bounds (max: {max})")]
    SectorOutOfBounds { sector: u64, max: u64 },

    #[error("Block {block_idx} not allocated (state: {state:?})")]
    BlockNotPresent { block_idx: u64, state: String },
}
