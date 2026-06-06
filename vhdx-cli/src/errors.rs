use vhdx::Error;

pub(crate) fn report_error(err: &Error) {
    match err {
        Error::Io(inner) => {
            eprintln!("IO error: {inner}");
        }
        Error::InvalidFile(msg) => {
            eprintln!("INVALID_FILE: {msg}");
        }
        Error::InvalidSignature {
            position,
            expected,
            found,
        } => {
            eprintln!(
                "HEADER_SIGNATURE_INVALID: at {position:?}: expected signature {expected:?}, found {found:?}"
            );
        }
        Error::CorruptedHeader(msg) | Error::LogEntryCorrupted(msg) => {
            eprintln!("{msg}");
        }
        Error::InvalidChecksum { expected, actual } => {
            eprintln!("CHECKSUM_MISMATCH: expected {expected:#010x}, actual {actual:#010x}");
        }
        Error::InvalidBlockState(state) => {
            eprintln!("BAT_BLOCK_STATE_INVALID: invalid block state {state:#04x}");
        }
        Error::InvalidRegionTable(msg) => {
            eprintln!("REGION_TABLE_INVALID: {msg}");
        }
        Error::InvalidMetadata(msg) => {
            eprintln!("METADATA_INVALID: {msg}");
        }
        Error::MetadataNotFound { guid } => {
            eprintln!("METADATA_NOT_FOUND: GUID {guid}");
        }
        Error::LogReplayRequired => {
            eprintln!("LOG_REPLAY_REQUIRED: pending log entries exist. Use --log-replay.");
        }
        Error::BatEntryNotFound { index } => {
            eprintln!("BAT_ENTRY_NOT_FOUND: index {index}");
        }
        Error::BlockNotPresent { block_idx, state } => {
            eprintln!("BLOCK_NOT_PRESENT: block {block_idx}, state={state}");
        }
        Error::SectorOutOfBounds { sector, max } => {
            eprintln!("SECTOR_OUT_OF_BOUNDS: sector {sector} (max={max})");
        }
        Error::ParentNotFound => {
            eprintln!("PARENT_NOT_FOUND: parent disk not found (all candidate paths inaccessible)");
        }
        Error::ParentMismatch { expected, actual } => {
            eprintln!("PARENT_GUID_MISMATCH: expected {expected}, actual {actual}");
        }
        Error::InvalidParameter(msg) => {
            eprintln!("INVALID_PARAMETER: {msg}");
        }
        Error::ReadOnly => {
            eprintln!("READ_ONLY: operation not supported in read-only mode");
        }
        Error::StateMismatch { state, description } => {
            eprintln!("STATE_MISMATCH: state={state:#04x}, {description}");
        }
        e => report_error_misc(e),
    }
}

pub(crate) fn report_error_misc(err: &Error) {
    match err {
        Error::BatFileOffsetUnaligned {
            offset_mb,
            block_size,
        } => {
            eprintln!("BAT_FILE_OFFSET_UNALIGNED: offset_mb={offset_mb}, block_size={block_size}");
        }
        Error::InvalidParentLocator(msg) => {
            eprintln!("PARENT_LOCATOR_INVALID: {msg}");
        }
        Error::HeaderLogGuidMismatch {
            header1_log_guid,
            header2_log_guid,
        } => {
            eprintln!(
                "HEADER_LOG_GUID_MISMATCH: header1={header1_log_guid}, header2={header2_log_guid}"
            );
        }
        Error::HeaderSequenceNumberInvalid {
            sequence_number_1,
            sequence_number_2,
        } => {
            eprintln!(
                "HEADER_SEQUENCE_INVALID: seq1={sequence_number_1}, seq2={sequence_number_2}"
            );
        }
        Error::UnsupportedVersion { version } => {
            eprintln!("UNSUPPORTED_VERSION: VHDX version {version} is not supported");
        }
        Error::UnsupportedLogVersion { version } => {
            eprintln!("UNSUPPORTED_LOG_VERSION: log version {version} is not supported");
        }
        Error::InvalidSectorBitmapState(state) => {
            eprintln!("SECTOR_BITMAP_STATE_INVALID: invalid sector bitmap state {state:#04x}");
        }
        Error::BatEntryCountInsufficient { actual, expected } => {
            eprintln!("BAT_ENTRY_COUNT_INSUFFICIENT: actual={actual}, expected={expected}");
        }
        Error::BatFileOffsetDuplicate { offset_mb } => {
            eprintln!("BAT_FILE_OFFSET_DUPLICATE: offset_mb={offset_mb}");
        }
        Error::RegionRequiredUnknown { guid } => {
            eprintln!("REGION_REQUIRED_UNKNOWN: GUID {guid}");
        }
        Error::RegionOptionalUnknown { guid } => {
            eprintln!("REGION_OPTIONAL_UNKNOWN: GUID {guid}");
        }
        Error::MetadataGuidUnknown { guid } => {
            eprintln!("METADATA_GUID_UNKNOWN: GUID {guid}");
        }
        Error::MetadataRequiredMissing { guid } => {
            eprintln!("METADATA_REQUIRED_MISSING: GUID {guid}");
        }
        Error::MetadataRequiredUnknown { guid } => {
            eprintln!("METADATA_REQUIRED_UNKNOWN: GUID {guid}");
        }
        Error::MetadataOptionalUnknown { guid } => {
            eprintln!("METADATA_OPTIONAL_UNKNOWN: GUID {guid}");
        }
        Error::MetadataReservedFlagsSet { flags } => {
            eprintln!("METADATA_RESERVED_FLAGS_SET: flags={flags:#010x}");
        }
        Error::LogSequenceGap { expected, found } => {
            eprintln!("LOG_SEQUENCE_GAP: expected={expected}, found={found}");
        }
        Error::LogSequenceGuidMismatch {
            entry_log_guid,
            header_log_guid,
        } => {
            eprintln!(
                "LOG_SEQUENCE_GUID_MISMATCH: entry={entry_log_guid}, header={header_log_guid}"
            );
        }
        Error::LogActiveSequenceEmpty => {
            eprintln!("LOG_ACTIVE_SEQUENCE_EMPTY: log active sequence is empty");
        }
        Error::ParentLocatorMissingLinkage => {
            eprintln!("PARENT_LOCATOR_MISSING_LINKAGE: parent linkage key missing");
        }
        Error::ParentLocatorLinkage2Conflict => {
            eprintln!(
                "PARENT_LOCATOR_LINKAGE2_CONFLICT: parent_linkage2 merge transition conflict"
            );
        }
        _ => {
            eprintln!("Error: {err}");
        }
    }
}
