use crate::types::Guid;

/// Signature error position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignaturePosition {
    FileTypeIdentifier,
    Header,
    RegionTable,
    MetadataTable,
    LogEntry,
    Descriptor,
    DataSector,
}

/// Pad a 4-byte signature to 8 bytes by zero-padding high bytes.
///
/// Used for `InvalidSignature` where the `expected` and `found` fields
/// must be 8 bytes. 4-byte signatures (e.g. "head", "regi") are padded
/// by setting the upper 4 bytes to zero.
pub(crate) fn pad_signature_4to8(sig: [u8; 4]) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&sig);
    out
}

/// Error type for VHDX operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("invalid VHDX file: {0}")]
    InvalidFile(String),

    #[error("signature mismatch at {position:?}: expected {expected:?}, found {found:?}")]
    InvalidSignature {
        position: SignaturePosition,
        expected: [u8; 8],
        found: [u8; 8],
    },

    #[error("corrupted header: {0}")]
    CorruptedHeader(String),

    #[error("header LogGuid mismatch: header1={header1_log_guid}, header2={header2_log_guid}")]
    HeaderLogGuidMismatch {
        header1_log_guid: Guid,
        header2_log_guid: Guid,
    },

    #[error("header sequence number invalid: seq1={sequence_number_1}, seq2={sequence_number_2}")]
    HeaderSequenceNumberInvalid {
        sequence_number_1: u64,
        sequence_number_2: u64,
    },

    #[error("unsupported VHDX version: {version}")]
    UnsupportedVersion { version: u16 },

    #[error("unsupported log version: {version}")]
    UnsupportedLogVersion { version: u16 },

    /// Header `LogLength` or `LogOffset` is not aligned to 1 MB.
    ///
    /// Standard: MS-VHDX-校验扩展标准 §4.1 — `HEADER_LOG_LENGTH_NOT_ALIGNED` / `HEADER_LOG_OFFSET_NOT_ALIGNED`
    #[error("header log field not 1MB-aligned: {field}={value}")]
    HeaderLogNotAligned {
        /// Which field is misaligned ("`log_length`" or "`log_offset`").
        field: String,
        /// The value of the misaligned field.
        value: u64,
    },

    #[error("checksum mismatch: expected {expected:#010x}, actual {actual:#010x}")]
    InvalidChecksum { expected: u32, actual: u32 },

    #[error("invalid BAT block state: {0:#04x}")]
    InvalidBlockState(u8),

    #[error("invalid sector bitmap block state: {0:#04x}")]
    InvalidSectorBitmapState(u8),

    /// BAT block state is valid but incompatible with the disk type
    /// (e.g. Unmapped on non-differencing disk, or sector bitmap Present on non-differencing).
    ///
    /// Standard: MS-VHDX-校验扩展标准 §4.3 — `BAT_ENTRY_STATE_MISMATCH`
    #[error("BAT state mismatch: state={state:#04x}, {description}")]
    StateMismatch {
        /// The raw 3-bit state value.
        state: u8,
        /// Human-readable description of the mismatch (e.g. "sector bitmap state not `NotPresent` on non-differencing disk").
        description: String,
    },

    /// BAT entry file offset is not aligned to the block size boundary.
    ///
    /// Standard: MS-VHDX-校验扩展标准 §4.3 — `BAT_ENTRY_FILE_OFFSET_UNALIGNED`
    #[error("BAT entry file offset not aligned: offset_mb={offset_mb}, block_size={block_size}")]
    BatFileOffsetUnaligned {
        /// The file offset in MB from the BAT entry.
        offset_mb: u64,
        /// The block size in bytes.
        block_size: u32,
    },

    #[error("BAT entry count insufficient: actual={actual}, expected={expected}")]
    BatEntryCountInsufficient { actual: u64, expected: u64 },

    #[error("BAT file offset duplicate: offset_mb={offset_mb}")]
    BatFileOffsetDuplicate { offset_mb: u64 },

    #[error("invalid region table: {0}")]
    InvalidRegionTable(String),

    #[error("unknown required region: {guid}")]
    RegionRequiredUnknown { guid: Guid },

    #[error("unknown optional region: {guid}")]
    RegionOptionalUnknown { guid: Guid },

    #[error("invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("unknown metadata GUID: {guid}")]
    MetadataGuidUnknown { guid: Guid },

    #[error("required metadata item missing: {guid}")]
    MetadataRequiredMissing { guid: Guid },

    #[error("unknown required metadata item: {guid}")]
    MetadataRequiredUnknown { guid: Guid },

    #[error("unknown optional metadata item: {guid}")]
    MetadataOptionalUnknown { guid: Guid },

    #[error("metadata reserved flags set: {flags:#010x}")]
    MetadataReservedFlagsSet { flags: u32 },

    /// Metadata Table Entry Reserved field is not zero.
    ///
    /// Standard: MS-VHDX-校验扩展标准 §4.4 — `METADATA_ENTRY_RESERVED_NONZERO`
    #[error("metadata entry reserved field non-zero: {reserved:#010x}")]
    MetadataEntryReservedNonzero { reserved: u32 },

    /// `FileParameters` item reserved flags (bits 2-31) are set.
    ///
    /// Standard: MS-VHDX-校验扩展标准 §4.4 — `METADATA_FILE_PARAMETERS_RESERVED_FLAGS`
    #[error("FileParameters reserved flags set: {flags:#010x}")]
    FileParametersReservedFlags { flags: u32 },

    #[error("invalid parent locator: {0}")]
    InvalidParentLocator(String),

    #[error("metadata item not found: {guid}")]
    MetadataNotFound { guid: Guid },

    #[error("log replay is required")]
    LogReplayRequired,

    #[error("corrupted log entry: {0}")]
    LogEntryCorrupted(String),

    #[error("log sequence gap: expected={expected}, found={found}")]
    LogSequenceGap { expected: u64, found: u64 },

    #[error("log sequence GUID mismatch: entry={entry_log_guid}, header={header_log_guid}")]
    LogSequenceGuidMismatch {
        entry_log_guid: Guid,
        header_log_guid: Guid,
    },

    #[error("log active sequence empty")]
    LogActiveSequenceEmpty,

    #[error("BAT entry not found: index {index}")]
    BatEntryNotFound { index: u64 },

    #[error("block not present: block {block_idx}, state={state}")]
    BlockNotPresent { block_idx: u64, state: String },

    #[error("sector out of bounds: {sector} (max={max})")]
    SectorOutOfBounds { sector: u64, max: u64 },

    #[error("parent disk not found")]
    ParentNotFound,

    #[error("parent resolver required")]
    ParentResolverRequired,

    #[error("parent logical sector size mismatch: child={child}, parent={parent}")]
    ParentSectorSizeMismatch { child: u32, parent: u32 },

    #[error("parent GUID mismatch: expected {expected}, found {actual}")]
    ParentMismatch { expected: Guid, actual: Guid },

    #[error("parent locator GUID mismatch: expected {expected}, found {actual}")]
    ParentLocatorGuidMismatch { expected: Guid, actual: Guid },

    #[error("parent linkage key missing")]
    ParentLocatorMissingLinkage,

    #[error("parent_linkage2 conflict (merge transition)")]
    ParentLocatorLinkage2Conflict,

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("read-only mode")]
    ReadOnly,
}

/// Result type alias for VHDX operations.
pub type Result<T> = std::result::Result<T, Error>;

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        match e {
            // Io variant — return inner error directly
            Error::Io(io_err) => io_err,

            // Invalid inputs
            Error::InvalidParameter(msg) => Self::new(std::io::ErrorKind::InvalidData, msg),

            // Invalid data / corruption
            Error::InvalidFile(..)
            | Error::InvalidSignature { .. }
            | Error::CorruptedHeader(..)
            | Error::HeaderLogGuidMismatch { .. }
            | Error::HeaderSequenceNumberInvalid { .. }
            | Error::UnsupportedVersion { .. }
            | Error::UnsupportedLogVersion { .. }
            | Error::HeaderLogNotAligned { .. }
            | Error::InvalidChecksum { .. }
            | Error::InvalidBlockState(..)
            | Error::InvalidSectorBitmapState(..)
            | Error::StateMismatch { .. }
            | Error::InvalidRegionTable(..)
            | Error::RegionRequiredUnknown { .. }
            | Error::RegionOptionalUnknown { .. }
            | Error::InvalidMetadata(..)
            | Error::MetadataGuidUnknown { .. }
            | Error::MetadataRequiredMissing { .. }
            | Error::MetadataRequiredUnknown { .. }
            | Error::MetadataOptionalUnknown { .. }
            | Error::MetadataReservedFlagsSet { .. }
            | Error::MetadataEntryReservedNonzero { .. }
            | Error::FileParametersReservedFlags { .. }
            | Error::InvalidParentLocator(..)
            | Error::BatFileOffsetUnaligned { .. }
            | Error::BatEntryCountInsufficient { .. }
            | Error::BatFileOffsetDuplicate { .. }
            | Error::LogEntryCorrupted(..)
            | Error::LogSequenceGap { .. }
            | Error::LogSequenceGuidMismatch { .. }
            | Error::LogActiveSequenceEmpty
            | Error::ParentLocatorMissingLinkage
            | Error::ParentLocatorLinkage2Conflict
            | Error::ParentSectorSizeMismatch { .. }
            | Error::ParentLocatorGuidMismatch { .. }
            | Error::ParentMismatch { .. } => {
                Self::new(std::io::ErrorKind::InvalidData, e.to_string())
            }

            // Not found
            Error::MetadataNotFound { .. }
            | Error::BatEntryNotFound { .. }
            | Error::BlockNotPresent { .. }
            | Error::ParentResolverRequired
            | Error::ParentNotFound => Self::new(std::io::ErrorKind::NotFound, e.to_string()),

            // Permission
            Error::ReadOnly | Error::LogReplayRequired => {
                Self::new(std::io::ErrorKind::PermissionDenied, e.to_string())
            }

            // Bounds
            Error::SectorOutOfBounds { .. } => {
                Self::new(std::io::ErrorKind::UnexpectedEof, e.to_string())
            }
        }
    }
}
