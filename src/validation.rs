//! VHDX specification compliance validator (read-only).
//!
//! `SpecValidator` performs structural validation against MS-VHDX and
//! companion standards. All validation is non-destructive and does not
//! modify file state.
//!
//! # Standard references
//!
//! - MS-VHDX (baseline specification)
//! - MS-VHDX-校验扩展标准 (this module's error code dictionary)
//! - MS-VHDX-宽松扩展标准 (permissive validation, RELAX)
//! - MS-VHDX-只读扩展标准 (read-only semantics, ROEXT)

use crate::error::{Error, Result, SignaturePosition};
use crate::header::{Header, HeaderStructure};
use crate::types::{StandardItems, Guid};

// ---------------------------------------------------------------------------
// ValidationIssue – structured diagnostic output
// ---------------------------------------------------------------------------

/// A structured validation issue for diagnostics and auditing.
///
/// Each issue carries a standardised error code, a human-readable message,
/// and a reference to the relevant section of the MS-VHDX specification
/// (or companion standard).
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Validation phase (e.g. `"header"`, `"bat"`, `"log"`).
    section: &'static str,
    /// Standardised error code (e.g. `"HEADER_SIGNATURE_INVALID"`).
    code: &'static str,
    /// Human-readable description, SHOULD include context values.
    message: String,
    /// Specification reference (e.g. `"MS-VHDX/2.2"`).
    spec_ref: &'static str,
}

impl ValidationIssue {
    /// Create a new validation issue.
    pub(crate) fn new(
        section: &'static str,
        code: &'static str,
        message: impl Into<String>,
        spec_ref: &'static str,
    ) -> Self {
        Self {
            section,
            code,
            message: message.into(),
            spec_ref,
        }
    }

    /// Validation phase (e.g. `"header"`, `"bat"`, `"log"`).
    pub fn section(&self) -> &'static str {
        self.section
    }

    /// Standardised error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Human-readable description.
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// Specification reference.
    pub fn spec_ref(&self) -> &'static str {
        self.spec_ref
    }
}

// ---------------------------------------------------------------------------
// SpecValidator
// ---------------------------------------------------------------------------

/// VHDX specification compliance validator.
///
/// Holds a reference to the file data buffer and performs read-only
/// structural checks against MS-VHDX and companion standards.
///
/// # Construction
///
/// Typically constructed with the file's full data buffer and configuration
/// flags (strict mode, whether the disk is differencing).
pub struct SpecValidator<'a> {
    /// Full file data buffer (must include header, log, BAT, metadata regions).
    data: &'a [u8],
    /// Whether strict validation mode is enabled.
    strict: bool,
    /// Optional path to the child file (used in parent chain validation).
    child_path: Option<std::path::PathBuf>,
}

impl<'a> SpecValidator<'a> {
    /// Create a new `SpecValidator`.
    ///
    /// `data` must be at least 1 MB (the header section). For full validation
    /// it should include the log, BAT, and metadata regions.
    pub(crate) fn new(data: &'a [u8], strict: bool) -> Self {
        Self {
            data,
            strict,
            child_path: None,
        }
    }

    /// Create a `SpecValidator` from a `File` reference.
    ///
    /// Collects all cached region buffers (header, log, BAT, metadata) from the
    /// file and assembles them into a contiguous view at their correct file offsets,
    /// so that region lookups by absolute offset work correctly.
    ///
    /// The returned validator borrows from the `File`'s internal `validator_buf`
    /// cache, which is built lazily on first access.
    pub(crate) fn from_file(file: &'a crate::file::File) -> Result<Self> {
        Ok(Self::new(file.validator_buf(), file.is_strict())
            .with_child_path(file.path().to_path_buf()))
    }

    /// Set the child file path for parent chain validation.
    pub(crate) fn with_child_path(mut self, path: std::path::PathBuf) -> Self {
        self.child_path = Some(path);
        self
    }

    /// Push a non-fatal validation issue into the collection.
    fn push_issue(issues: &mut Vec<ValidationIssue>, issue: ValidationIssue) {
        issues.push(issue);
    }

    /// Map a header validation [`Error`] to the correct [`ValidationIssue`] and push it.
    ///
    /// Distinguishes `InvalidSignature` (header position) from `InvalidChecksum` and
    /// version errors, producing the appropriate per-header issue code so that each
    /// invalid header gets a *specific* diagnostic rather than the generic
    /// `HEADER_SEQUENCE_NUMBER_INVALID` catch-all.
    fn push_header_issue(issues: &mut Vec<ValidationIssue>, header_idx: u32, err: &Error) {
        let issue = match err {
            Error::InvalidSignature {
                position: SignaturePosition::Header,
                ..
            } => ValidationIssue::new(
                "header",
                "HEADER_SIGNATURE_INVALID",
                format!("header {header_idx} signature error: {err}"),
                "MS-VHDX/2.2.2",
            ),
            Error::InvalidChecksum { .. } => ValidationIssue::new(
                "header",
                "HEADER_CHECKSUM_MISMATCH",
                format!("header {header_idx} checksum error: {err}"),
                "MS-VHDX/2.2.2",
            ),
            Error::UnsupportedVersion { version } => ValidationIssue::new(
                "header",
                "HEADER_VERSION_UNSUPPORTED",
                format!(
                    "header {header_idx} version {version} is not supported (expected 1)"
                ),
                "MS-VHDX/2.2.2",
            ),
            Error::UnsupportedLogVersion { version } => ValidationIssue::new(
                "header",
                "HEADER_LOG_VERSION_UNSUPPORTED",
                format!(
                    "header {header_idx} log version {version} is not supported (expected 0)"
                ),
                "MS-VHDX/2.2.2",
            ),
            _ => ValidationIssue::new(
                "header",
                "HEADER_CORRUPTED",
                format!("header {header_idx} error: {err}"),
                "MS-VHDX/2.2.2",
            ),
        };
        issues.push(issue);
    }

    // -----------------------------------------------------------------------
    // Orchestrator
    // -----------------------------------------------------------------------

    /// Run all structural validations.
    ///
    /// Calls each sub-validation in order. Returns the first error encountered.
    ///
    /// # Standard coverage
    ///
    /// - Layout:   MS-VHDX/2.1 (alignment, non-overlap)
    /// - Header:   MS-VHDX/2.2
    /// - Log:      MS-VHDX/2.3
    /// - BAT:      MS-VHDX/2.5
    /// - Metadata: MS-VHDX/2.6
    /// - Differencing: parent locator + parent chain (when applicable)
    pub fn validate_file(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        issues.extend(self.validate_header()?);
        issues.extend(self.validate_region_table()?);
        issues.extend(self.validate_log()?);
        issues.extend(self.validate_bat()?);
        issues.extend(self.validate_metadata()?);
        issues.extend(self.validate_required_metadata_items()?);
        // Parent locator is only applicable for differencing disks.
        if self.has_parent() {
            issues.extend(self.validate_parent_locator()?);
        }
        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // Header validation
    // -----------------------------------------------------------------------

    /// Validate the header section.
    ///
    /// Checks:
    /// - File type identifier signature ("vhdxfile")
    /// - Header 1 and Header 2 signatures, CRC-32C, version
    /// - Sequence number comparison (both headers valid)
    /// - LogGuid consistency between headers
    pub fn validate_header(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let header = self.parse_header()?;

        // -- File type identifier --
        let ft = header.file_type();
        if ft.signature() != b"vhdxfile" {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "header",
                "HEADER_FILE_TYPE_ID_INVALID",
                format!(
                    "invalid signature at offset 0: expected \"vhdxfile\", found {:?}",
                    std::str::from_utf8(ft.signature()).unwrap_or("<binary>")
                ),
                "MS-VHDX/2.2.1",
            ));
            return Err(Error::InvalidSignature {
                position: SignaturePosition::FileTypeIdentifier,
                expected: *b"vhdxfile",
                found: *ft.signature(),
            });
        }

        // -- Validate both headers individually --
        let h1 = header.header(1);
        let h2 = header.header(2);

        // Apply wrapper validation (version/log_version checks) to each
        let v1 = h1.and_then(|h| self.validate_single_header(Ok(h)));
        let v2 = h2.and_then(|h| self.validate_single_header(Ok(h)));

        let h1_valid = v1.is_ok();
        let h2_valid = v2.is_ok();

        // Both invalid → file corrupt (produce per-header specific issues)
        if !h1_valid && !h2_valid {
            Self::push_header_issue(&mut issues, 1, v1.as_ref().err().unwrap());
            Self::push_header_issue(&mut issues, 2, v2.as_ref().err().unwrap());
            return Err(Error::CorruptedHeader("both headers are invalid".into()));
        }

        // One invalid → push issue for the bad one, continue with valid header
        if !h1_valid {
            Self::push_header_issue(&mut issues, 1, v1.as_ref().err().unwrap());
        }
        if !h2_valid {
            Self::push_header_issue(&mut issues, 2, v2.as_ref().err().unwrap());
        }

        // Only check sequence number equality + LogGuid consistency when BOTH are valid
        if h1_valid && h2_valid {
            let v1 = v1.unwrap();
            let v2 = v2.unwrap();

            if v1.sequence_number() == v2.sequence_number() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "header",
                    "HEADER_SEQUENCE_NUMBER_INVALID",
                    "both headers have same sequence number",
                    "MS-VHDX/2.2.2",
                ));
                return Err(Error::HeaderSequenceNumberInvalid {
                    sequence_number_1: v1.sequence_number(),
                    sequence_number_2: v2.sequence_number(),
                });
            }

            let log_guid = self.current_log_guid(&header)?;
            if log_guid != v1.log_guid() || log_guid != v2.log_guid() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "header",
                    "HEADER_LOG_GUID_MISMATCH",
                    "LogGuid differs between headers",
                    "MS-VHDX/2.2.2",
                ));
                return Err(Error::HeaderLogGuidMismatch {
                    header1_log_guid: v1.log_guid(),
                    header2_log_guid: v2.log_guid(),
                });
            }
        }

        // LogOffset and LogLength must be 1MB aligned (MS-VHDX §2.2.2)
        let current = header.header(0)?;
        let log_offset = current.log_offset();
        let log_length = current.log_length();
        let mb: u64 = 1024 * 1024;
        if log_length > 0 && log_length as u64 % mb != 0 {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "header",
                "HEADER_LOG_LENGTH_NOT_ALIGNED",
                format!("log_length {log_length} is not a multiple of 1MB"),
                "MS-VHDX/2.2.2",
            ));
            return Err(Error::CorruptedHeader(format!(
                "LOG_LENGTH_NOT_ALIGNED: log_length {log_length} is not a multiple of 1MB"
            )));
        }
        if log_offset > 0 && log_offset % mb != 0 {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "header",
                "HEADER_LOG_OFFSET_NOT_ALIGNED",
                format!("log_offset {log_offset} is not a multiple of 1MB"),
                "MS-VHDX/2.2.2",
            ));
            return Err(Error::CorruptedHeader(format!(
                "LOG_OFFSET_NOT_ALIGNED: log_offset {log_offset} is not a multiple of 1MB"
            )));
        }

        Ok(issues)
    }

    /// Validate a single header structure (signature, CRC, version, log_version).
    fn validate_single_header(
        &self,
        result: Result<HeaderStructure<'a>>,
    ) -> Result<HeaderStructure<'a>> {
        let mut issues = Vec::new();
        let h = result?;

        // Signature check is done by Header::validate_header_at (returns
        // CorruptedHeader on mismatch). Version and log_version are additional
        // checks performed here.

        // Version must be 1
        if h.version() != 1 {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "header",
                "HEADER_VERSION_UNSUPPORTED",
                    format!(
                        "version {} is not supported (expected 1)",
                        h.version()
                    ),
                    "MS-VHDX/2.2.2",
                ));
                return Err(Error::UnsupportedVersion {
                version: h.version(),
            });
        }

        // Log version must be 0 (MS-VHDX §2.2.2: MUST NOT continue UNLESS LogGuid==0)
        if h.log_version() != 0 && h.log_guid() != Guid::zero() {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "header",
                "HEADER_LOG_VERSION_UNSUPPORTED",
                    format!(
                        "log version {} is not supported (expected 0)",
                        h.log_version()
                    ),
                    "MS-VHDX/2.2.2",
                ));
                return Err(Error::UnsupportedLogVersion {
                version: h.log_version(),
            });
        }

        Ok(h)
    }

    // -----------------------------------------------------------------------
    // Region table validation
    // -----------------------------------------------------------------------

    /// Validate the region tables.
    ///
    /// Checks:
    /// - "regi" signature and CRC-32C for both region tables
    /// - Entry alignment (1 MB)
    /// - Entry overlap (no two regions' ranges overlap)
    /// - Entry count <= 2047
    /// - Required unknown region handling (strict mode)
    pub fn validate_region_table(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let header = self.parse_header()?;

        // Check both region tables.
        let rt1 = header.region_table(1);
        let rt2 = header.region_table(2);

        match (&rt1, &rt2) {
            (Err(e), _) | (_, Err(e)) => {
                if let Error::InvalidSignature { position: SignaturePosition::RegionTable, .. } = e {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "region_table",
                        "REGION_SIGNATURE_INVALID",
                        format!("region table signature error: {e}"),
                        "MS-VHDX/2.2.3.1",
                    ));
                    return Err(Error::InvalidRegionTable(format!("{e}")));
                }
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_CHECKSUM_MISMATCH",
                    format!("{e}"),
                    "MS-VHDX/2.2.3.1",
                ));
                return Err(Error::InvalidRegionTable(format!("{e}")));
            }
            _ => {}
        }

        let rt1 = rt1.unwrap();
        let rt2 = rt2.unwrap();

        // Entry count <= 2047 (already checked by Header::validate_region_table_at,
        // but re-verify here for completeness)
        for (idx, rt) in [rt1, rt2].iter().enumerate() {
            let count = rt.header().entry_count();
            if count > 2047 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM",
                    format!("region table {idx} entry count {count} exceeds maximum of 2047"),
                    "MS-VHDX/2.2.3.1",
                ));
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM: region table {idx} entry count {count} exceeds maximum of 2047"
                )));
            }
        }

        // Check entries for alignment and overlap in the CURRENT region table.
        let current_rt = match header.region_table(0) {
            Ok(rt) => rt,
            Err(e) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_CHECKSUM_MISMATCH",
                    format!("current region table: {e}"),
                    "MS-VHDX/2.2.3.1",
                ));
                return Err(Error::InvalidRegionTable(format!("current region table: {e}")));
            }
        };

        issues.extend(self.validate_region_entries(current_rt)?);

        Ok(issues)
    }

    /// Validate a region table's entries for alignment, overlap, and required-unknown.
    fn validate_region_entries(
        &self,
        rt: crate::header::RegionTable<'a>,
    ) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let mb: u64 = 1024 * 1024;
        let entries: Vec<_> = rt.entries().collect();

        for (i, entry) in entries.iter().enumerate() {
            let file_offset = entry.file_offset();
            let length = entry.length();

            // Alignment: file_offset must be 1 MB aligned
            if file_offset % mb != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_ALIGNMENT",
                    format!("entry {i} file_offset {file_offset:#x} not 1MB-aligned"),
                    "MS-VHDX/2.2.3.2",
                ));
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_ALIGNMENT: entry {i} file_offset {file_offset:#x} not 1MB-aligned"
                )));
            }

            // T15: file_offset must be >= 1 MB (MS-VHDX §2.4)
            if file_offset < mb {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_OFFSET_MINIMUM",
                    format!("entry {i} file_offset {file_offset} < 1MB minimum"),
                    "MS-VHDX/2.2.3.2",
                ));
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_OFFSET_MINIMUM: entry {i} file_offset {file_offset} < 1MB minimum"
                )));
            }

            // Length must be a multiple of 1 MB
            if u64::from(length) % mb != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_ALIGNMENT",
                    format!("entry {i} length {length} not 1MB-aligned"),
                    "MS-VHDX/2.2.3.2",
                ));
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_ALIGNMENT: entry {i} length {length} not 1MB-aligned"
                )));
            }

            // Overlap check: compare against all previous entries
            let end = file_offset + u64::from(length);
            for (j, prev) in entries[..i].iter().enumerate() {
                let prev_end = prev.file_offset() + u64::from(prev.length());
                if file_offset < prev_end && prev.file_offset() < end {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "region_table",
                        "REGION_ENTRY_OVERLAP",
                        format!("entries {j} and {i} overlap"),
                        "MS-VHDX/2.1",
                    ));
                    return Err(Error::InvalidRegionTable(format!(
                        "REGION_ENTRY_OVERLAP: entries {j} and {i} overlap"
                    )));
                }
            }

            // Required unknown: if required=1 and GUID is unknown → always error
            // Optional unknown: if required=0 and GUID is unknown → error only in strict mode
            if !is_known_region_guid(&entry.guid()) {
                if entry.required() {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "region_table",
                        "REGION_REQUIRED_UNKNOWN",
                        format!("required unknown region GUID {}", entry.guid()),
                        "RELAX",
                    ));
                    return Err(Error::RegionRequiredUnknown {
                        guid: entry.guid(),
                    });
                }
                if self.strict {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "region_table",
                        "REGION_OPTIONAL_UNKNOWN",
                        format!("optional unknown region GUID {} in strict mode", entry.guid()),
                        "RELAX",
                    ));
                    return Err(Error::RegionOptionalUnknown {
                        guid: entry.guid(),
                    });
                }
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "region_table",
                    "REGION_OPTIONAL_UNKNOWN",
                    format!(
                        "optional unknown region GUID {} tolerated in non-strict mode",
                        entry.guid()
                    ),
                    "RELAX",
                ));
            }
        }

        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // BAT validation
    // -----------------------------------------------------------------------

    /// Validate the Block Allocation Table.
    ///
    /// Checks:
    /// - Entry states are valid values
    /// - State matches disk type (e.g., fixed disk has no Unmapped)
    /// - Sector bitmap entries in non-differencing disks are NotPresent
    /// - File offsets are aligned
    pub fn validate_bat(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let bat_data = match self.bat_region() {
            Some(d) => d,
            None => return Ok(issues), // No BAT region found; skip
        };

        let chunk_ratio = self.chunk_ratio();
        if chunk_ratio == 0 {
            return Ok(issues); // Cannot validate without chunk ratio
        }

        let bat = crate::bat::Bat::new(bat_data, chunk_ratio);
        let has_parent = self.has_parent();
        let block_size = self.block_size() as u64;

        // T17: BAT entry count vs VirtualDiskSize/BlockSize
        let virtual_disk_size = self.virtual_disk_size();
        if virtual_disk_size > 0 && block_size > 0 {
            let min_entries = (virtual_disk_size + block_size - 1) / block_size;
            if bat.len() < min_entries as usize {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "bat",
                    "BAT_ENTRY_COUNT_INSUFFICIENT",
                    format!("BAT has {} entries but virtual disk requires at least {}", bat.len(), min_entries),
                    "MS-VHDX/2.5",
                ));
                return Err(Error::BatEntryCountInsufficient { actual: bat.len() as u64, expected: min_entries });
            }
        }

        // T16: Collect non-zero file_offset_mb values for uniqueness check
        let mut seen_offsets = std::collections::HashSet::new();

        for (_i, entry) in bat.entries().enumerate() {
            let raw_state = entry.raw_state();

            if entry.is_sector_bitmap() {
                // Sector bitmap entry validation
                let sb_state = entry.sector_bitmap_state();
                if sb_state.is_none() {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "bat",
                        "BAT_SECTOR_BITMAP_INVALID_STATE",
                        format!("invalid sector bitmap state: {raw_state}"),
                        "MS-VHDX/2.5.1.2",
                    ));
                    return Err(Error::InvalidSectorBitmapState(raw_state));
                }
                let sb_state = sb_state.unwrap();

                use crate::bat::SectorBitmapState;
                if !has_parent && sb_state != SectorBitmapState::NotPresent {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "bat",
                        "BAT_ENTRY_STATE_MISMATCH",
                        format!("sector bitmap state not NotPresent on non-differencing disk"),
                        "MS-VHDX/2.5.1.1",
                    ));
                    return Err(Error::StateMismatch {
                        state: raw_state,
                        description: "sector bitmap state not NotPresent on non-differencing disk".into(),
                    });
                }
            } else {
                // Payload entry validation
                let p_state = entry.payload_state();
                if p_state.is_none() {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "bat",
                        "BAT_ENTRY_INVALID_STATE",
                        format!("invalid payload block state: {raw_state}"),
                        "MS-VHDX/2.5.1.1",
                    ));
                    return Err(Error::InvalidBlockState(raw_state));
                }
                let p_state = p_state.unwrap();

                use crate::bat::PayloadBlockState;
                // Fixed/dynamic (non-differencing) disk: no Unmapped or PartiallyPresent
                if !has_parent {
                    match p_state {
                        PayloadBlockState::Unmapped | PayloadBlockState::PartiallyPresent => {
                            Self::push_issue(&mut issues, ValidationIssue::new(
                                "bat",
                                "BAT_ENTRY_STATE_MISMATCH",
                                format!("payload state Unmapped/PartiallyPresent on non-differencing disk"),
                                "MS-VHDX/2.5.1.1",
                            ));
                            return Err(Error::StateMismatch {
                                state: raw_state,
                                description: "payload state Unmapped/PartiallyPresent on non-differencing disk".into(),
                            });
                        }
                        _ => {}
                    }
                }

                // File offset alignment check (for entries with data)
                match p_state {
                    PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent => {
                        let offset_mb = entry.file_offset_mb();

                        // T16: check file_offset_mb uniqueness
                        if offset_mb != 0 && !seen_offsets.insert(offset_mb) {
                            Self::push_issue(&mut issues, ValidationIssue::new(
                                "bat",
                                "BAT_FILE_OFFSET_DUPLICATE",
                                format!("duplicate file_offset_mb {offset_mb} in BAT"),
                                "MS-VHDX/2.5",
                            ));
                            return Err(Error::BatFileOffsetDuplicate { offset_mb });
                        }

                        // Per MS-VHDX §2.5.1.1, FileOffsetMB is in units of 1 MB.
                        // Windows places blocks at MB-aligned offsets (e.g. 4 MB)
                        // which need not be a multiple of BlockSize — only the
                        // MB alignment itself is required. Skip block-size alignment
                        // check; keep a duplicate-offset check above.
                    }
                    _ => {}
                }
            }
        }

        // T13: Sector bitmap consistency for differencing disks
        if has_parent {
            let stride = chunk_ratio + 1;
            // Iterate through chunks. Each chunk has chunk_ratio payload entries
            // followed by 1 sector bitmap entry.
            let total_entries = bat.len() as u64;
            let num_chunks = total_entries / stride;

            for chunk_idx in 0..num_chunks {
                let sb_bat_idx = chunk_idx * stride + chunk_ratio;
                if sb_bat_idx >= total_entries {
                    break;
                }
                let sb_entry = match bat.entry(sb_bat_idx) {
                    Ok(e) => e,
                    Err(_) => break,
                };

                // Check each payload entry in this chunk for PartiallyPresent
                let mut any_partially_present = false;
                for payload_offset_in_chunk in 0..chunk_ratio {
                    let payload_bat_idx = chunk_idx * stride + payload_offset_in_chunk;
                    if payload_bat_idx >= total_entries {
                        break;
                    }
                    let payload_entry = match bat.entry(payload_bat_idx) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    if !payload_entry.is_sector_bitmap() {
                        if let Some(crate::bat::PayloadBlockState::PartiallyPresent) =
                            payload_entry.payload_state()
                        {
                            any_partially_present = true;
                            break;
                        }
                    }
                }

                // If any payload is PartiallyPresent, the sector bitmap MUST be Present
                if any_partially_present {
                    let sb_state = sb_entry.sector_bitmap_state();
                    match sb_state {
                        Some(crate::bat::SectorBitmapState::Present) => {}
                        _ => {
                            Self::push_issue(&mut issues, ValidationIssue::new(
                                "bat",
                                "BAT_SECTOR_BITMAP_INVALID_STATE",
                                format!(
                                    "chunk {chunk_idx}: payload entry is PartiallyPresent but sector bitmap state is {:?}",
                                    sb_state
                                ),
                                "MS-VHDX/2.5.1.2",
                            ));
                        }
                    }
                }
            }
        }

        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // Metadata validation
    // -----------------------------------------------------------------------

    /// Validate the metadata table and item structure.
    ///
    /// Checks:
    /// - Table signature "metadata"
    /// - Entry count <= 2047
    /// - Entry offset/length bounds (within metadata region)
    /// - No items extend beyond the region
    pub fn validate_metadata(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let meta_data = match self.metadata_region() {
            Some(d) => d,
            None => return Ok(issues), // No metadata region; skip
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let table = meta.table();

        // Table signature
        if let Err(e) = table.header().validate_signature() {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "metadata",
                "METADATA_TABLE_SIGNATURE_INVALID",
                format!("{e}"),
                "MS-VHDX/2.6.1.1",
            ));
            return Err(e);
        }

        // Entry count
        let entry_count = table.header().entry_count();
        if entry_count > 2047 {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "metadata",
                "METADATA_ENTRY_INVALID",
                format!("entry count {entry_count} > 2047"),
                "MS-VHDX/2.6.1.2",
            ));
            return Err(Error::InvalidMetadata(format!(
                "METADATA_ENTRY_INVALID: entry count {entry_count} > 2047"
            )));
        }

        // Validate each entry and collect ranges for overlap check (T18)
        let region_len = meta_data.len();
        let mut ranges: Vec<(u32, u32, Guid)> = Vec::new();

        for entry in table.entries() {
            let offset = entry.offset() as usize;
            let length = entry.length() as usize;

            // Length=0 → Offset must also be 0 (MS-VHDX §2.6.1.2)
            if length == 0 && offset != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata",
                    "METADATA_ENTRY_INVALID",
                    format!("length=0 but offset={offset} (expected 0)"),
                    "MS-VHDX/2.6.1.2",
                ));
                return Err(Error::InvalidMetadata(format!(
                    "METADATA_ENTRY_INVALID: length=0 but offset={offset} (expected 0)"
                )));
            }

            // Offset + length must fit in the metadata region
            if length > 0 {
                // T19: metadata entry offset must be >= 64KB minimum (MS-VHDX §2.6.1.2)
                if offset < 65536 {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "metadata",
                        "METADATA_ENTRY_OFFSET_MINIMUM",
                        format!("metadata entry offset {offset} < 64KB minimum"),
                        "MS-VHDX/2.6.1.2",
                    ));
                    return Err(Error::InvalidMetadata(format!(
                        "METADATA_ENTRY_OFFSET_MINIMUM: metadata entry offset {offset} < 64KB minimum"
                    )));
                }

                let end = match offset.checked_add(length) {
                    Some(e) => e,
                    None => {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata",
                            "METADATA_ENTRY_INVALID",
                            "offset+length overflow",
                            "MS-VHDX/2.6.1.2",
                        ));
                        return Err(Error::InvalidMetadata(
                            "METADATA_ENTRY_INVALID: offset+length overflow".into(),
                        ));
                    }
                };
                if end > region_len {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "metadata",
                        "METADATA_ENTRY_INVALID",
                        format!("item extent [{offset}..{end}] exceeds region ({region_len})"),
                        "MS-VHDX/2.6.1.2",
                    ));
                    return Err(Error::InvalidMetadata(format!(
                        "METADATA_ENTRY_INVALID: item extent [{offset}..{end}] exceeds region ({region_len})"
                    )));
                }

                // Collect range for overlap check
                ranges.push((offset as u32, (offset + length) as u32, entry.item_id()));
            }

            // Check flags reserved bits (bits 3-31 are reserved per MS-VHDX §2.6.1.2:
            // the diagram puts A=IsUser(bit0), B=IsVirtualDisk(bit1), C=IsRequired(bit2);
            // bits 3-31 are Reserved and MUST be 0).
            if entry.flags().has_reserved_bits() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata",
                    "METADATA_RESERVED_FLAGS_SET",
                    format!(
                        "metadata entry GUID {} has reserved flags bits set: {:#010x}",
                        entry.item_id(),
                        entry.flags_bits()
                    ),
                    "MS-VHDX/2.6.1.2",
                ));
                return Err(Error::MetadataReservedFlagsSet {
                    flags: entry.flags_bits(),
                });
            }

            // Check entry reserved field (MS-VHDX §2.6.1.2: reserved must be 0)
            if entry.reserved() != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata",
                    "METADATA_ENTRY_RESERVED_NONZERO",
                    format!(
                        "metadata entry GUID {} has reserved field set to {:#010x}",
                        entry.item_id(),
                        entry.reserved()
                    ),
                    "MS-VHDX/2.6.1.2",
                ));
                return Err(Error::InvalidMetadata(format!(
                    "METADATA_ENTRY_RESERVED_NONZERO: metadata entry GUID {} has reserved field set to {:#010x}",
                    entry.item_id(),
                    entry.reserved()
                )));
            }

            // Unknown metadata entry handling (strict mode per MS-VHDX-宽松扩展标准 §3)
            if !is_known_metadata_guid(&entry.item_id()) {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata",
                    "METADATA_GUID_UNKNOWN",
                    format!("unknown metadata GUID {}", entry.item_id()),
                    "MS-VHDX/2.6.2",
                ));
                if entry.flags().is_required() {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "metadata",
                        "METADATA_REQUIRED_UNKNOWN",
                        format!("required unknown metadata GUID {}", entry.item_id()),
                        "RELAX",
                    ));
                    return Err(Error::MetadataRequiredUnknown {
                        guid: entry.item_id(),
                    });
                }
                if self.strict {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "metadata",
                        "METADATA_OPTIONAL_UNKNOWN",
                        format!("optional unknown metadata GUID {} in strict mode", entry.item_id()),
                        "RELAX",
                    ));
                    return Err(Error::MetadataOptionalUnknown {
                        guid: entry.item_id(),
                    });
                }
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata",
                    "METADATA_OPTIONAL_UNKNOWN",
                    format!(
                        "optional unknown metadata GUID {} tolerated in non-strict mode",
                        entry.item_id()
                    ),
                    "RELAX",
                ));
            }
        }

        // T18: pairwise overlap check for metadata items
        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                let (s1, e1, g1) = &ranges[i];
                let (s2, e2, g2) = &ranges[j];
                if *s1 < *e2 && *s2 < *e1 {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "metadata",
                        "METADATA_ITEMS_OVERLAP",
                        format!("metadata items overlap: {g1} and {g2}"),
                        "MS-VHDX/2.6.2",
                    ));
                    return Err(Error::InvalidMetadata(format!(
                        "METADATA_ITEMS_OVERLAP: metadata items overlap: {g1} and {g2}"
                    )));
                }
            }
        }

        // Check for corrupted (undersized) known required metadata items.
        // Non-blocking: push_issue and continue for each undersized item.
        {
            let known_items: &[(&Guid, &str, u32)] = &[
                (&StandardItems::FILE_PARAMETERS, "FileParameters", 8),
                (&StandardItems::VIRTUAL_DISK_SIZE, "VirtualDiskSize", 8),
                (&StandardItems::VIRTUAL_DISK_ID, "VirtualDiskId", 16),
                (&StandardItems::LOGICAL_SECTOR_SIZE, "LogicalSectorSize", 4),
                (&StandardItems::PHYSICAL_SECTOR_SIZE, "PhysicalSectorSize", 4),
            ];
            for &(guid, name, min_len) in known_items {
                if let Ok(entry) = table.entry(guid) {
                    if entry.length() > 0 && entry.length() < min_len {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata",
                            "METADATA_ITEM_CORRUPTED",
                            format!(
                                "{name}: data length {} < expected minimum {} bytes",
                                entry.length(),
                                min_len
                            ),
                            "MS-VHDX/2.6.2",
                        ));
                    }
                }
            }
        }

        Ok(issues)
    }

    /// Validate that all required metadata items are present.
    ///
    /// Required items (MS-VHDX §2.6.2):
    /// - FileParameters
    /// - VirtualDiskSize
    /// - VirtualDiskId
    /// - LogicalSectorSize
    /// - PhysicalSectorSize
    /// - ParentLocator (if differencing disk)
    pub fn validate_required_metadata_items(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let meta_data = match self.metadata_region() {
            Some(d) => d,
            None => return Ok(issues),
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let items = meta.items();

        // Check each required item
        let required_items: &[(&Guid, &str)] = &[
            (&StandardItems::FILE_PARAMETERS, "FileParameters"),
            (&StandardItems::VIRTUAL_DISK_SIZE, "VirtualDiskSize"),
            (&StandardItems::VIRTUAL_DISK_ID, "VirtualDiskId"),
            (&StandardItems::LOGICAL_SECTOR_SIZE, "LogicalSectorSize"),
            (&StandardItems::PHYSICAL_SECTOR_SIZE, "PhysicalSectorSize"),
        ];

        for (guid, name) in required_items {
            if meta.table().entry(guid).is_err() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    format!("{name} entry not found in metadata table"),
                    "RELAX",
                ));
                return Err(Error::MetadataRequiredMissing {
                    guid: **guid,
                });
            }
            // Also verify data is present
            match name {
                &"FileParameters" => {
                    let fp = match items.file_parameters() {
                        Ok(fp) => fp,
                        Err(_) => {
                            Self::push_issue(&mut issues, ValidationIssue::new(
                                "metadata_required",
                                "METADATA_REQUIRED_MISSING",
                                "FileParameters data not present",
                                "RELAX",
                            ));
                            return Err(Error::MetadataRequiredMissing {
                                guid: StandardItems::FILE_PARAMETERS,
                            });
                        }
                    };
                    // MS-VHDX §2.6.2.1: bits 2-31 of flags are reserved (MUST be 0).
                    if fp.has_reserved_bits_set() {
                        let fp_flags = fp.flags();
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata_required",
                            "METADATA_FILE_PARAMETERS_RESERVED_FLAGS",
                            format!(
                                "FileParameters reserved flags (bits 2-31) are set: {:#010x}",
                                fp_flags
                            ),
                            "MS-VHDX/2.6.2.1",
                        ));
                    }
                }
                &"VirtualDiskSize" => {
                    if items.virtual_disk_size().is_err() {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata_required",
                            "METADATA_REQUIRED_MISSING",
                            format!("{name} data not present"),
                            "RELAX",
                        ));
                        return Err(Error::MetadataRequiredMissing { guid: **guid });
                    }
                }
                &"VirtualDiskId" => {
                    if items.virtual_disk_id().is_err() {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata_required",
                            "METADATA_REQUIRED_MISSING",
                            format!("{name} data not present"),
                            "RELAX",
                        ));
                        return Err(Error::MetadataRequiredMissing { guid: **guid });
                    }
                }
                &"LogicalSectorSize" => {
                    if items.logical_sector_size().is_err() {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata_required",
                            "METADATA_REQUIRED_MISSING",
                            format!("{name} data not present"),
                            "RELAX",
                        ));
                        return Err(Error::MetadataRequiredMissing { guid: **guid });
                    }
                }
                &"PhysicalSectorSize" => {
                    if items.physical_sector_size().is_err() {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "metadata_required",
                            "METADATA_REQUIRED_MISSING",
                            format!("{name} data not present"),
                            "RELAX",
                        ));
                        return Err(Error::MetadataRequiredMissing { guid: **guid });
                    }
                }
                _ => {}
            }
        }

        // ParentLocator required for differencing disks
        if self.has_parent() {
            if meta.table().entry(&StandardItems::PARENT_LOCATOR).is_err() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    "ParentLocator entry not found for differencing disk",
                    "RELAX",
                ));
                return Err(Error::MetadataRequiredMissing {
                    guid: StandardItems::PARENT_LOCATOR,
                });
            }
            if items.parent_locator().is_err() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    "ParentLocator data not present for differencing disk",
                    "RELAX",
                ));
                return Err(Error::MetadataRequiredMissing {
                    guid: StandardItems::PARENT_LOCATOR,
                });
            }
        }

        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // Log validation
    // -----------------------------------------------------------------------

    /// Validate the log section.
    ///
    /// Checks:
    /// - Entry signatures ("loge")
    /// - Entry CRC-32C
    /// - Entry length (4KB multiple)
    /// - Tail (4KB multiple)
    /// - Descriptor signatures ("desc"/"zero")
    /// - Data sector signatures ("data") and SequenceHigh/Low consistency
    /// - Sequence continuity
    /// - LogGuid matching header LogGuid
    /// - Active sequence non-empty
    pub fn validate_log(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let log_data = match self.log_region() {
            Some(d) => d,
            None => return Ok(issues), // No log or empty log
        };

        // Empty log is valid
        if log_data.is_empty() {
            return Ok(issues);
        }

        // Check if log is "all zeros" — this indicates no log entries
        let is_all_zero = log_data.iter().all(|&b| b == 0);
        if is_all_zero {
            return Ok(issues);
        }

        let log = crate::log::Log::new(log_data)?;
        let header = self.parse_header()?;
        let current = header.header(0)?;
        let header_log_guid = current.log_guid();

        // Raw pre-scan: detect entries with invalid signatures that
        // LogEntryIter silently skips (sets done=true, returns None).
        // Log entry header layout (MS-VHDX §2.3.1.1):
        //   [0..4]  Signature  [4..8]  Checksum  [8..12]  EntryLength
        {
            let mut scan_offset: usize = 0;
            while scan_offset + 64 <= log_data.len() {
                let sig = &log_data[scan_offset..scan_offset + 4];
                if sig == b"loge" {
                    // Valid signature — read EntryLength at offset+8 to advance
                    let entry_length = u32::from_le_bytes(log_data[scan_offset + 8..scan_offset + 12].try_into().unwrap()) as usize;
                    if entry_length > 0 && entry_length % 4096 == 0
                        && scan_offset + entry_length <= log_data.len()
                    {
                        scan_offset += entry_length;
                    } else {
                        // Bad entry_length — skip to next 4KB boundary
                        scan_offset += 4096;
                    }
                } else if sig == [0u8; 4] {
                    // All-zero padding — end of log entries
                    break;
                } else if sig == b"data" {
                    // Data sector signature (MS-VHDX §2.3.1.4) — not an
                    // entry header; skip to next 4KB boundary.
                    scan_offset += 4096;
                } else {
                    // Invalid signature
                    let mut found = [0u8; 4];
                    found.copy_from_slice(sig);
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "log",
                        "LOG_SIGNATURE_INVALID",
                        format!("expected \"loge\", found {:?}", found),
                        "MS-VHDX/2.3.1.1",
                    ));
                    // Skip to next 4KB boundary
                    scan_offset += 4096;
                }
            }
        }

        let entries: Vec<_> = log.entries().collect();

        if entries.is_empty() {
            return Ok(issues); // No valid entries found
        }

        let mut prev_seq: Option<u64> = None;

        for entry in &entries {
            // Entry signature already validated by Log::parse_entry_at
            // Verify CRC
            if let Err(_e) = entry.verify_checksum() {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_CHECKSUM_MISMATCH",
                    "entry CRC-32C mismatch",
                    "MS-VHDX/2.3.1.1",
                ));
                return Err(Error::LogEntryCorrupted(
                    "LOG_ENTRY_CHECKSUM_MISMATCH: entry CRC-32C mismatch".into(),
                ));
            }

            let hdr = entry.header();

            // Entry length must be 4KB multiple
            let entry_length = hdr.entry_length();
            if entry_length == 0 || entry_length % 4096 != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_LENGTH_INVALID",
                    format!("entry_length={entry_length}"),
                    "MS-VHDX/2.3.1.1",
                ));
                return Err(Error::LogEntryCorrupted(format!(
                    "LOG_ENTRY_LENGTH_INVALID: entry_length={entry_length}"
                )));
            }

            // Tail must be 4KB multiple (or 0)
            let tail = hdr.tail();
            if tail % 4096 != 0 {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_TAIL_INVALID",
                    format!("tail={tail}"),
                    "MS-VHDX/2.3.1.1",
                ));
                return Err(Error::LogEntryCorrupted(format!(
                    "LOG_ENTRY_TAIL_INVALID: tail={tail}"
                )));
            }

            // LogGuid must match header LogGuid
            let entry_log_guid = hdr.log_guid();
            let is_zero_guid = entry_log_guid.to_bytes() == [0u8; 16];
            if !is_zero_guid && entry_log_guid != header_log_guid {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "log",
                    "LOG_SEQUENCE_GUID_MISMATCH",
                    format!("entry LogGuid {entry_log_guid} != header LogGuid {header_log_guid}"),
                    "MS-VHDX/2.3.2",
                ));
                return Err(Error::LogSequenceGuidMismatch { entry_log_guid, header_log_guid });
            }

            // Sequence number continuity
            let seq = hdr.sequence_number();
            if let Some(prev) = prev_seq {
                if seq != prev + 1 {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "log",
                        "LOG_SEQUENCE_GAP",
                        format!("seq {seq} does not follow {prev}"),
                        "MS-VHDX/2.3.2",
                    ));
                    return Err(Error::LogSequenceGap { expected: prev + 1, found: seq });
                }
            }
            prev_seq = Some(seq);

            // Descriptor count validation
            let _desc_count = hdr.descriptor_count();
            let actual_data_descs: usize = entry
                .descriptors()
                .filter_map(|d| d.ok())
                .filter(|d| {
                    matches!(d, crate::log::Descriptor::Data(_))
                })
                .count();

            // Check data sector count matches data descriptors
            let data_sectors: Vec<_> = entry.data().collect();
            if data_sectors.len() != actual_data_descs {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "log",
                    "LOG_DESCRIPTOR_COUNT_MISMATCH",
                    format!(
                        "data sectors ({}) != data descriptors ({})",
                        data_sectors.len(),
                        actual_data_descs
                    ),
                    "MS-VHDX/2.3.1",
                ));
                return Err(Error::LogEntryCorrupted(format!(
                    "LOG_DESCRIPTOR_COUNT_MISMATCH: data sectors ({}) != data descriptors ({})",
                    data_sectors.len(),
                    actual_data_descs
                )));
            }

            // Validate data sectors
            for sector in &data_sectors {
                let sig = sector.signature();
                if sig != b"data" {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "log",
                        "LOG_DATA_SECTOR_INVALID",
                        "invalid data sector signature",
                        "MS-VHDX/2.3.1.4",
                    ));
                    return Err(Error::InvalidSignature {
                        position: SignaturePosition::DataSector,
                        expected: crate::error::pad_signature_4to8(b"data"),
                        found: crate::error::pad_signature_4to8(sig),
                    });
                }

                // SequenceHigh + SequenceLow should match entry sequence number
                let sector_seq = sector.sequence_number();
                if sector_seq != seq {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "log",
                        "LOG_DATA_SECTOR_INVALID",
                        format!("sector seq {sector_seq} != entry seq {seq}"),
                        "MS-VHDX/2.3.1.4",
                    ));
                    return Err(Error::LogEntryCorrupted(format!(
                        "LOG_DATA_SECTOR_INVALID: sector seq {sector_seq} != entry seq {seq}"
                    )));
                }
            }

            // Validate descriptors
            for desc_result in entry.descriptors() {
                let desc = match desc_result {
                    Ok(d) => d,
                    Err(e) => {
                        Self::push_issue(&mut issues, ValidationIssue::new(
                            "log",
                            "LOG_DESCRIPTOR_SIGNATURE_INVALID",
                            format!("{e}"),
                            "MS-VHDX/2.3.1",
                        ));
                        return Err(Error::LogEntryCorrupted(format!(
                            "LOG_DESCRIPTOR_SIGNATURE_INVALID: {e}"
                        )));
                    }
                };
                // Each descriptor's sequence number must match entry
                let desc_seq = desc.sequence_number();
                if desc_seq != seq {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "log",
                        "LOG_DESCRIPTOR_SEQUENCE_MISMATCH",
                        format!("descriptor seq {desc_seq} != entry seq {seq}"),
                        "MS-VHDX/2.3.1",
                    ));
                    return Err(Error::LogEntryCorrupted(format!(
                        "LOG_DESCRIPTOR_SEQUENCE_MISMATCH: descriptor seq {desc_seq} != entry seq {seq}"
                    )));
                }
            }
        }

        // Active sequence non-empty check
        if entries.is_empty() {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "log",
                "LOG_ACTIVE_SEQUENCE_EMPTY",
                "no valid log entries",
                "MS-VHDX/2.3.3",
            ));
            return Err(Error::LogActiveSequenceEmpty);
        }

        // LOG_REPLAY_REQUIRED — non-blocking status hint
        //
        // Per MS-VHDX-校验扩展标准 §4.5:
        // When a replayable log exists, emit LOG_REPLAY_REQUIRED as a
        // non-blocking ValidationIssue.  This is distinct from the blocking
        // Error::LogReplayRequired that the open path returns for Require
        // policy — here it is purely informational.
        if header_log_guid != Guid::zero() {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "log",
                "LOG_REPLAY_REQUIRED",
                "replayable log entries exist (use --log-replay to replay)",
                "ROEXT",
            ));
        }

        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // Parent locator validation
    // -----------------------------------------------------------------------

    /// Validate the parent locator for differencing disks.
    ///
    /// Checks:
    /// - `parent_linkage` key exists
    /// - `parent_linkage2` key absent (conflict)
    /// - At least one path entry (relative_path, volume_path, absolute_win32_path)
    pub fn validate_parent_locator(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let meta_data = match self.metadata_region() {
            Some(d) => d,
            None => return Ok(issues),
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let locator = match meta.items().parent_locator() {
            Ok(l) => l,
            Err(_) => return Ok(issues), // No parent locator → nothing to validate
        };

        let kv_data = locator.key_value_data();
        let mut has_parent_linkage = false;
        let mut has_path = false;
        let mut parent_linkage_guid: Option<Guid> = None;

        for kv in locator.entries() {
            let key = kv.key(kv_data)?;

            match key.as_str() {
                "parent_linkage" => {
                    has_parent_linkage = true;
                    // Capture the GUID value for DataWriteGuid check
                    if let Ok(value) = kv.value(kv_data) {
                        parent_linkage_guid = parse_guid_from_braced_string(&value);
                    }
                }
                "parent_linkage2" => {
                    Self::push_issue(&mut issues, ValidationIssue::new(
                        "parent_locator",
                        "PARENT_LOCATOR_LINKAGE2_CONFLICT",
                        "parent_linkage2 present",
                        "MS-VHDX/2.6.2.6.3",
                    ));
                    return Err(Error::ParentLocatorLinkage2Conflict);
                }
                "relative_path" | "volume_path" | "absolute_win32_path" => {
                    has_path = true;
                }
                _ => {}
            }
        }

        if !has_parent_linkage {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "parent_locator",
                "PARENT_LOCATOR_MISSING_LINKAGE",
                "parent_linkage key not found",
                "MS-VHDX/2.6.2.6.3",
            ));
            return Err(Error::ParentLocatorMissingLinkage);
        }

        if !has_path {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "parent_locator",
                "PARENT_LOCATOR_NO_VALID_PATH",
                "no valid parent path (relative_path/volume_path/absolute_win32_path)",
                "MS-VHDX/2.6.2.6.3",
            ));
            return Err(Error::ParentNotFound);
        }

        // DataWriteGuid check: compare child's parent_linkage GUID with
        // parent's actual DataWriteGuid. Skip if parent file is inaccessible.
        if let Some(expected_linkage) = parent_linkage_guid {
            if let Ok(parent_path_buf) = locator.resolve_parent_path() {
                use std::io::Read;
                if let Ok(mut parent_file) = std::fs::File::open(&parent_path_buf) {
                    let mut parent_header_buf = vec![0u8; 1024 * 1024];
                    if let Ok(bytes_read) = parent_file.read(&mut parent_header_buf) {
                        if bytes_read >= 8 {
                            parent_header_buf.truncate(bytes_read);
                            let expected_sig: [u8; 8] = [
                                0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6C, 0x65,
                            ];
                            if parent_header_buf[..8] == expected_sig {
                                if let Ok(parent_header) = crate::header::Header::new(&parent_header_buf) {
                                    if let Ok(parent_current) = parent_header.header(0) {
                                        let parent_data_write_guid = parent_current.data_write_guid();
                                        if parent_data_write_guid != expected_linkage {
                                            Self::push_issue(&mut issues, ValidationIssue::new(
                                                "parent_locator",
                                                "PARENT_LOCATOR_GUID_MISMATCH",
                                                format!(
                                                    "DataWriteGuid mismatch: expected {}, actual {}",
                                                    expected_linkage, parent_data_write_guid
                                                ),
                                                "MS-VHDX/2.6.2.6",
                                            ));
                                            return Err(Error::ParentMismatch {
                                                expected: expected_linkage,
                                                actual: parent_data_write_guid,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(issues)
    }

    // -----------------------------------------------------------------------
    // Parent chain validation
    // -----------------------------------------------------------------------

    /// Validate the parent chain (differencing disks).
    ///
    /// Opens the parent file, reads its `DataWriteGuid`, and compares it
    /// with the child's expected `parent_linkage` GUID.
    ///
    /// Returns [`ParentChainInfo`](crate::ParentChainInfo) on success.
    pub(crate) fn validate_parent_chain(&self) -> Result<crate::file::ParentChainInfo> {
        let mut issues = Vec::new();
        let meta_data = match self.metadata_region() {
            Some(d) => d,
            None => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    "no metadata region",
                    "VALEXT",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let locator = match meta.items().parent_locator() {
            Ok(l) => l,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_MISSING_LINKAGE",
                    "no parent locator",
                    "MS-VHDX/2.6.2.6.3",
                ));
                return Err(Error::ParentLocatorMissingLinkage);
            }
        };

        // Extract the expected parent_linkage GUID from the locator BEFORE
        // resolving the path, so format errors are caught regardless of
        // whether the parent file exists on disk.
        // The value is a UTF-16LE string: "{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}"
        // with lowercase hex digits and enclosing braces.
        let kv_data = locator.key_value_data();
        let mut expected_linkage: Option<Guid> = None;

        for kv in locator.entries() {
            let key = match kv.key(kv_data) {
                Ok(k) => k,
                Err(_) => continue,
            };
            if key == "parent_linkage2" {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_LINKAGE2_CONFLICT",
                    "parent_linkage2 present",
                    "MS-VHDX/2.6.2.6.3",
                ));
                return Err(Error::ParentLocatorLinkage2Conflict);
            }
            if key == "parent_linkage" {
                let value = match kv.value(kv_data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                expected_linkage = parse_guid_from_braced_string(&value);
            }
        }

        let expected_linkage = match expected_linkage {
            Some(g) => g,
            None => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    "parent_linkage value is not a valid GUID format",
                    "VALEXT",
                ));
                return Err(Error::InvalidParentLocator(
                    "parent_linkage value is not a valid GUID format".into(),
                ));
            }
        };

        // Resolve the parent path (checks accessibility via std::fs::metadata)
        let parent_path_buf = match locator.resolve_parent_path() {
            Ok(p) => p,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_NO_VALID_PATH",
                    "unresolvable parent path",
                    "MS-VHDX/2.6.2.6.3",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        // Try to open parent and get its DataWriteGuid
        use std::io::Read;

        let mut parent_file = match std::fs::File::open(&parent_path_buf) {
            Ok(f) => f,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_NO_VALID_PATH",
                    format!("unable to open parent file: {}", parent_path_buf.display()),
                    "MS-VHDX/2.6.2.6.3",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        // Read the parent's first 1 MB header section
        let mut parent_header_buf = vec![0u8; 1024 * 1024];
        let bytes_read = match parent_file.read(&mut parent_header_buf) {
            Ok(n) => n,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("failed to read parent file: {}", parent_path_buf.display()),
                    "VALEXT",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        if bytes_read < 8 {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("parent file too small ({} bytes): {}", bytes_read, parent_path_buf.display()),
                    "VALEXT",
            ));
            return Err(Error::ParentNotFound);
        }

        parent_header_buf.truncate(bytes_read);

        // Validate parent's vhdxfile signature
        let sig = &parent_header_buf[..8];
        let expected_sig: [u8; 8] = [
            0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6C, 0x65,
        ];
        if sig != expected_sig {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("parent file is not a valid VHDX: {}", parent_path_buf.display()),
                    "VALEXT",
            ));
            return Err(Error::ParentNotFound);
        }

        // Parse the parent header to get DataWriteGuid
        let parent_header = match Header::new(&parent_header_buf) {
            Ok(h) => h,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("failed to parse parent header: {}", parent_path_buf.display()),
                    "VALEXT",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        let parent_current = match parent_header.header(0) {
            Ok(h) => h,
            Err(_) => {
                Self::push_issue(&mut issues, ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("failed to get current parent header: {}", parent_path_buf.display()),
                    "VALEXT",
                ));
                return Err(Error::ParentNotFound);
            }
        };

        let parent_data_write_guid = parent_current.data_write_guid();

        // Compare with expected linkage
        let linkage_matched = parent_data_write_guid == expected_linkage;

        if !linkage_matched {
            Self::push_issue(&mut issues, ValidationIssue::new(
                "parent_locator",
                "PARENT_LOCATOR_GUID_MISMATCH",
                format!(
                    "DataWriteGuid mismatch: expected {}, actual {}",
                    expected_linkage, parent_data_write_guid
                ),
                "MS-VHDX/2.6.2.6",
            ));
            return Err(Error::ParentMismatch {
                expected: expected_linkage,
                actual: parent_data_write_guid,
            });
        }

        let child = self.child_path.clone().unwrap_or_else(|| std::path::PathBuf::from("<unknown>"));
        Ok(crate::file::ParentChainInfo::new(
            child,
            parent_path_buf,
            linkage_matched,
        ))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Parse the header section from the data buffer.
    fn parse_header(&self) -> Result<Header<'a>> {
        Header::new(self.data)
    }

    /// Resolve the log region slice from the data buffer.
    fn log_region(&self) -> Option<&'a [u8]> {
        let header = self.parse_header().ok()?;
        let current = header.header(0).ok()?;
        let log_offset = current.log_offset() as usize;
        let log_length = current.log_length() as usize;

        if log_offset == 0 && log_length == 0 {
            return None;
        }

        let end = log_offset.checked_add(log_length)?;
        if end > self.data.len() {
            return None;
        }

        Some(&self.data[log_offset..end])
    }

    /// Resolve a region by GUID from the current region table.
    fn region_for_guid(&self, guid: &Guid) -> Option<&'a [u8]> {
        let header = self.parse_header().ok()?;
        let rt = header.region_table(0).ok()?;
        for entry in rt.entries() {
            if entry.guid() == *guid {
                let offset = entry.file_offset() as usize;
                let length = entry.length() as usize;
                let end = offset.checked_add(length)?;
                if end <= self.data.len() {
                    return Some(&self.data[offset..end]);
                }
            }
        }
        None
    }

    /// Resolve the BAT region data.
    fn bat_region(&self) -> Option<&'a [u8]> {
        // BAT region GUID: 2DC27766-F623-4200-9D64-115E9BFD4A08
        const BAT_GUID: Guid = Guid::from_bytes([
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42,
            0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
        ]);
        self.region_for_guid(&BAT_GUID)
    }

    /// Resolve the metadata region data.
    fn metadata_region(&self) -> Option<&'a [u8]> {
        // Metadata region GUID: 8B7CA206-4790-4B9A-B8FE-575F050F886E
        const METADATA_GUID: Guid = Guid::from_bytes([
            0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B,
            0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
        ]);
        self.region_for_guid(&METADATA_GUID)
    }

    /// Determine whether this is a differencing disk (has_parent flag).
    fn has_parent(&self) -> bool {
        if let Some(meta_data) = self.metadata_region() {
            if let Ok(meta) = crate::metadata::Metadata::new(meta_data) {
                if let Ok(fp) = meta.items().file_parameters() {
                    return fp.has_parent();
                }
            }
        }
        false
    }

    /// Extract the current header's LogGuid.
    fn current_log_guid(&self, header: &Header<'a>) -> Result<Guid> {
        let current = header.header(0)?;
        Ok(current.log_guid())
    }

    /// Compute the chunk ratio for BAT interpretation.
    fn chunk_ratio(&self) -> u64 {
        let block_size = self.block_size() as u64;
        let logical_sector_size = self.logical_sector_size() as u64;
        if block_size == 0 || logical_sector_size == 0 {
            return 0;
        }
        crate::common::compute_chunk_ratio(block_size, logical_sector_size)
    }

    /// Get block size from metadata.
    fn block_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region() {
            if let Ok(meta) = crate::metadata::Metadata::new(meta_data) {
                if let Ok(fp) = meta.items().file_parameters() {
                    return fp.block_size();
                }
            }
        }
        0
    }

    /// Get logical sector size from metadata.
    fn logical_sector_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region() {
            if let Ok(meta) = crate::metadata::Metadata::new(meta_data) {
                if let Ok(lss) = meta.items().logical_sector_size() {
                    return lss;
                }
            }
        }
        0
    }

    /// Get virtual disk size from metadata.
    fn virtual_disk_size(&self) -> u64 {
        if let Some(meta_data) = self.metadata_region() {
            if let Ok(meta) = crate::metadata::Metadata::new(meta_data) {
                if let Ok(vds) = meta.items().virtual_disk_size() {
                    return vds;
                }
            }
        }
        0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a GUID from a braced lowercase hex string like
/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}`.
///
/// Returns `None` if the string is not in the expected format.
fn parse_guid_from_braced_string(s: &str) -> Option<Guid> {
    let s = s.trim();
    // Strip enclosing braces
    let inner = s.strip_prefix('{').and_then(|s| s.strip_suffix('}'))?;
    // Remove hyphens and parse as 32 hex digits
    let hex: String = inner.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for i in 0..16 {
        let byte_str = &hex[i * 2..i * 2 + 2];
        bytes[i] = u8::from_str_radix(byte_str, 16).ok()?;
    }
    Some(Guid::from_bytes(bytes))
}

/// Check whether a GUID corresponds to a known region type.
fn is_known_region_guid(guid: &Guid) -> bool {
    // BAT and Metadata are the only required regions per MS-VHDX.
    const KNOWN: &[Guid] = &[
        Guid::from_bytes([
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42,
            0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
        ]), // BAT
        Guid::from_bytes([
            0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B,
            0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
        ]), // Metadata
    ];
    KNOWN.contains(guid)
}

/// Check whether a GUID corresponds to a known metadata item type.
fn is_known_metadata_guid(guid: &Guid) -> bool {
    const KNOWN: &[Guid] = &[
        StandardItems::FILE_PARAMETERS,
        StandardItems::VIRTUAL_DISK_SIZE,
        StandardItems::VIRTUAL_DISK_ID,
        StandardItems::LOGICAL_SECTOR_SIZE,
        StandardItems::PHYSICAL_SECTOR_SIZE,
        StandardItems::PARENT_LOCATOR,
    ];
    KNOWN.contains(guid)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::crc32c;
    use bitvec::prelude::*;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    const KB: usize = 1024;
    const MB: usize = 1024 * KB;

    /// Build a minimal valid VHDX file in memory for validation testing.
    fn build_test_vhdx() -> Vec<u8> {
        let virtual_size: u64 = 1 * 1024 * 1024 * 1024; // 1 GB
        let block_size: u32 = 32 * 1024 * 1024; // 32 MB
        let logical_sector_size: u32 = 4096;
        let bat_entry_count =
            (virtual_size + block_size as u64 - 1) / block_size as u64;
        let chunk_ratio =
            (1u64 << 23) * logical_sector_size as u64 / block_size as u64;
        let sector_bitmap_count = (bat_entry_count + chunk_ratio - 1) / chunk_ratio;
        let total_bat_entries = (bat_entry_count + sector_bitmap_count) as usize;
        let bat_bytes = total_bat_entries * 8;
        let bat_size = std::cmp::max(
            ((bat_bytes as u64 + MB as u64 - 1) / MB as u64) as u32,
            1,
        ) * (MB as u32);

        let header_size = 4 * KB;
        let region_table_size = 64 * KB;

        let header1_offset = 64 * KB;
        let header2_offset = 128 * KB;
        let rt1_offset = 192 * KB;
        let rt2_offset = 256 * KB;
        let log_offset: u64 = 1 * MB as u64;
        let log_length: u32 = 1 * MB as u32;
        let bat_offset: u64 = 2 * MB as u64;
        let metadata_offset: u64 = bat_offset + bat_size as u64;
        let metadata_size: u32 = 1 * MB as u32;

        let file_end = metadata_offset + metadata_size as u64;
        let mut buf = vec![0u8; file_end as usize];

        // File type identifier "vhdxfile"
        buf[0..8].copy_from_slice(b"vhdxfile");

        // Write headers
        write_header(&mut buf, header1_offset, 5);
        write_header(&mut buf, header2_offset, 3);

        // Write region tables
        write_region_table(&mut buf, rt1_offset, bat_offset, bat_size, metadata_offset, metadata_size);
        write_region_table(&mut buf, rt2_offset, bat_offset, bat_size, metadata_offset, metadata_size);

        // Write minimal BAT: payload entries = FullyPresent with block-aligned
        // offsets, sector bitmap entries = NotPresent.
        let bat_start = bat_offset as usize;
        let block_size_mb = block_size as u64 / (MB as u64);
        let metadata_end_mb =
            (metadata_offset + metadata_size as u64 + MB as u64 - 1) / MB as u64;
        // Align first payload offset to block_size boundary.
        let first_payload_mb =
            ((metadata_end_mb + block_size_mb - 1) / block_size_mb) * block_size_mb;
        let mut sb_written: u64 = 0;
        let mut payload_idx: u64 = 0;
        for i in 0..total_bat_entries {
            let entry_offset = bat_start + i * 8;
            let payloads_before = i as u64 - sb_written;
            let is_sb = payloads_before > 0
                && payloads_before % chunk_ratio == 0
                && sb_written < sector_bitmap_count;
            let entry_val: u64 = if is_sb {
                // Sector bitmap: NotPresent
                sb_written += 1;
                0
            } else {
                // Payload: FullyPresent at block-aligned offset
                let offset_mb = first_payload_mb + payload_idx * block_size_mb;
                payload_idx += 1;
                {
                    let mut entry_buf = [0u8; 8];
                    let bits = entry_buf.view_bits_mut::<Lsb0>();
                    bits[0..3].store::<u8>(6u8); // FullyPresent state
                    bits[20..64].store::<u64>(offset_mb);
                    u64::from_le_bytes(entry_buf)
                }
            };
            buf[entry_offset..entry_offset + 8].copy_from_slice(&entry_val.to_le_bytes());
        }

        // Write metadata table + items
        write_metadata(&mut buf, metadata_offset as usize, block_size, logical_sector_size);

        buf
    }

    fn write_header(buf: &mut [u8], offset: usize, seq: u64) {
        let header_size = 4 * KB;
        let slice = &mut buf[offset..][..header_size];
        slice[..4].copy_from_slice(b"head");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice[8..16].copy_from_slice(&seq.to_le_bytes());
        slice[64..66].copy_from_slice(&0u16.to_le_bytes()); // log_version
        slice[66..68].copy_from_slice(&1u16.to_le_bytes()); // version
        slice[68..72].copy_from_slice(&(1u32 * MB as u32).to_le_bytes()); // log_length
        slice[72..80].copy_from_slice(&(1u64 * MB as u64).to_le_bytes()); // log_offset

        let checksum = crc32c(slice);
        slice[4..8].copy_from_slice(&checksum.to_le_bytes());
    }

    fn write_region_table(
        buf: &mut [u8],
        offset: usize,
        bat_offset: u64,
        bat_size: u32,
        metadata_offset: u64,
        metadata_size: u32,
    ) {
        let region_table_size = 64 * KB;
        let slice = &mut buf[offset..][..region_table_size];

        slice[..4].copy_from_slice(b"regi");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
        slice[8..12].copy_from_slice(&2u32.to_le_bytes()); // 2 entries
        slice[12..16].copy_from_slice(&0u32.to_le_bytes()); // reserved

        // BAT region GUID
        let bat_guid: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42,
            0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
        ];
        // Metadata region GUID
        let meta_guid: [u8; 16] = [
            0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B,
            0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
        ];

        let mut entry_off = 16;
        // Entry 0: BAT
        slice[entry_off..entry_off + 16].copy_from_slice(&bat_guid);
        slice[entry_off + 16..entry_off + 24].copy_from_slice(&bat_offset.to_le_bytes());
        slice[entry_off + 24..entry_off + 28].copy_from_slice(&bat_size.to_le_bytes());
        slice[entry_off + 28..entry_off + 32].copy_from_slice(&1u32.to_le_bytes()); // required

        entry_off += 32;
        // Entry 1: Metadata
        slice[entry_off..entry_off + 16].copy_from_slice(&meta_guid);
        slice[entry_off + 16..entry_off + 24].copy_from_slice(&metadata_offset.to_le_bytes());
        slice[entry_off + 24..entry_off + 28].copy_from_slice(&metadata_size.to_le_bytes());
        slice[entry_off + 28..entry_off + 32].copy_from_slice(&1u32.to_le_bytes()); // required

        let checksum = crc32c(slice);
        slice[4..8].copy_from_slice(&checksum.to_le_bytes());
    }

    fn write_metadata(buf: &mut [u8], offset: usize, block_size: u32, logical_sector_size: u32) {
        let metadata_table_size = 64 * KB;

        // Table header
        buf[offset..offset + 8].copy_from_slice(b"metadata");
        buf[offset + 10..offset + 12].copy_from_slice(&6u16.to_le_bytes()); // 6 entries

        // Write 6 table entries. Item offsets are relative to the start of the
        // metadata region (which includes the 64KB table).
        let mut entry_off = offset + 32;
        let item_base = metadata_table_size as u32; // items start right after the 64KB table

        // Entry 0: FileParameters (relative offset = 64KB+0, length=8)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::FILE_PARAMETERS,
            item_base + 0,
            8,
            0x0000_0004, // is_required
        );

        // Entry 1: VirtualDiskSize (relative offset = 64KB+8, length=8)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::VIRTUAL_DISK_SIZE,
            item_base + 8,
            8,
            0x0000_0006, // is_virtual_disk + is_required
        );

        // Entry 2: VirtualDiskId (relative offset = 64KB+16, length=16)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::VIRTUAL_DISK_ID,
            item_base + 16,
            16,
            0x0000_0006,
        );

        // Entry 3: LogicalSectorSize (relative offset = 64KB+32, length=4)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::LOGICAL_SECTOR_SIZE,
            item_base + 32,
            4,
            0x0000_0006,
        );

        // Entry 4: PhysicalSectorSize (relative offset = 64KB+40, length=4)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::PHYSICAL_SECTOR_SIZE,
            item_base + 40,
            4,
            0x0000_0006,
        );

        // Entry 5: ParentLocator (empty, offset=0, length=0)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::PARENT_LOCATOR,
            0,
            0,
            0x0000_0004,
        );

        // FileParameters per MS-VHDX §2.6.2.1: block_size first, flags second
        let items_base = offset + metadata_table_size;
        let fp_flags: u32 = 0; // dynamic disk
        buf[items_base..items_base + 4].copy_from_slice(&block_size.to_le_bytes());
        buf[items_base + 4..items_base + 8].copy_from_slice(&fp_flags.to_le_bytes());

        // VirtualDiskSize: 1 GB
        let disk_size: u64 = 1 * 1024 * 1024 * 1024;
        buf[items_base + 8..items_base + 16].copy_from_slice(&disk_size.to_le_bytes());

        // VirtualDiskId: zeros (already zeroed)
        // LogicalSectorSize
        buf[items_base + 32..items_base + 36].copy_from_slice(&logical_sector_size.to_le_bytes());
        // PhysicalSectorSize
        buf[items_base + 40..items_base + 44].copy_from_slice(&4096u32.to_le_bytes());
    }

    fn write_metadata_entry(
        buf: &mut [u8],
        entry_off: &mut usize,
        guid: &Guid,
        item_offset: u32,
        length: u32,
        flags: u32,
    ) {
        buf[*entry_off..*entry_off + 16].copy_from_slice(&guid.to_bytes());
        buf[*entry_off + 16..*entry_off + 20].copy_from_slice(&item_offset.to_le_bytes());
        buf[*entry_off + 20..*entry_off + 24].copy_from_slice(&length.to_le_bytes());
        buf[*entry_off + 24..*entry_off + 28].copy_from_slice(&flags.to_le_bytes());
        // reserved (4 bytes): 0
        *entry_off += 32;
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn validation_issue_fields() {
        let issue = ValidationIssue::new(
            "header",
            "HEADER_SIGNATURE_INVALID",
            "invalid header signature",
            "MS-VHDX/2.2.2",
        );
        assert_eq!(issue.section(), "header");
        assert_eq!(issue.code(), "HEADER_SIGNATURE_INVALID");
        assert_eq!(issue.message(), "invalid header signature");
        assert_eq!(issue.spec_ref(), "MS-VHDX/2.2.2");
    }

    #[test]
    fn validate_header_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_header().is_ok());
    }

    #[test]
    fn validate_header_bad_file_signature() {
        let mut buf = build_test_vhdx();
        buf[0..8].copy_from_slice(b"NOTAVHDX");
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_header().is_err());
    }

    #[test]
    fn validate_header_corrupted_header1() {
        let mut buf = build_test_vhdx();
        // Corrupt both header signatures so neither is valid
        buf[64 * KB] = 0xFF;
        buf[128 * KB] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        // Both headers invalid -> must fail
        assert!(validator.validate_header().is_err());
    }

    #[test]
    fn validate_header_bad_version() {
        let mut buf = build_test_vhdx();
        // Set version to 2 on both headers
        buf[64 * KB + 66..64 * KB + 68].copy_from_slice(&2u16.to_le_bytes());
        buf[128 * KB + 66..128 * KB + 68].copy_from_slice(&2u16.to_le_bytes());
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_header().is_err());
    }

    #[test]
    fn validate_region_table_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_region_table().is_ok());
    }

    #[test]
    fn validate_region_table_bad_signature() {
        let mut buf = build_test_vhdx();
        // Corrupt RT1 signature
        buf[192 * KB] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_region_table().is_err());
    }

    #[test]
    fn validate_region_table_bad_entry_count() {
        let mut buf = build_test_vhdx();
        // Set entry count to 3000 (> 2047) and fix CRC
        buf[192 * KB + 8..192 * KB + 12].copy_from_slice(&3000u32.to_le_bytes());
        let checksum = crc32c(&buf[192 * KB..][..64 * KB]);
        buf[192 * KB + 4..192 * KB + 8].copy_from_slice(&checksum.to_le_bytes());
        let validator = SpecValidator::new(&buf, true);
        // Entry count > 2047 should cause header.region_table() to fail
        assert!(validator.validate_region_table().is_err());
    }

    #[test]
    fn validate_bat_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_bat().is_ok());
    }

    #[test]
    fn validate_metadata_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_metadata().is_ok());
    }

    #[test]
    fn validate_required_metadata_items_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_required_metadata_items().is_ok());
    }

    #[test]
    fn validate_required_metadata_items_missing() {
        let mut buf = build_test_vhdx();
        // Find metadata offset from region table entry 1 (Metadata entry)
        // Region table starts at 192KB. Entry 1 (Metadata) starts at offset 16+32=48.
        // file_offset is at entry_start+16 = 64.
        let metadata_offset = u64::from_le_bytes(buf[192 * KB + 64..192 * KB + 72].try_into().unwrap());
        let mo = metadata_offset as usize;
        // Zero out the FileParameters entry GUID (first entry after header, at offset 32)
        buf[mo + 32..mo + 48].copy_from_slice(&[0u8; 16]);
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_required_metadata_items().is_err());
    }

    #[test]
    fn test_metadata_item_corrupted_file_parameters() {
        let mut buf = build_test_vhdx();
        // Find metadata offset from region table entry 1 (Metadata entry)
        let metadata_offset = u64::from_le_bytes(buf[192 * KB + 64..192 * KB + 72].try_into().unwrap());
        let mo = metadata_offset as usize;
        // Modify FileParameters entry (entry 0, at offset 32 from metadata start)
        // length field: bytes 20-23 of the entry → change from 8 to 4
        buf[mo + 32 + 20..mo + 32 + 24].copy_from_slice(&4u32.to_le_bytes());
        let validator = SpecValidator::new(&buf, true);
        let issues = validator.validate_metadata().unwrap();
        assert!(
            issues.iter().any(|i| i.code() == "METADATA_ITEM_CORRUPTED"),
            "expected METADATA_ITEM_CORRUPTED for undersized FileParameters"
        );
    }

    #[test]
    fn test_metadata_item_corrupted_not_for_valid_size() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        let issues = validator.validate_metadata().unwrap();
        let corrupted: Vec<_> = issues
            .iter()
            .filter(|i| i.code() == "METADATA_ITEM_CORRUPTED")
            .collect();
        assert!(
            corrupted.is_empty(),
            "expected no METADATA_ITEM_CORRUPTED for valid sizes, got: {:?}",
            corrupted
        );
    }

    #[test]
    fn test_metadata_item_corrupted_preserves_missing() {
        let mut buf = build_test_vhdx();
        let metadata_offset = u64::from_le_bytes(buf[192 * KB + 64..192 * KB + 72].try_into().unwrap());
        let mo = metadata_offset as usize;
        // Zero out FileParameters entry GUID in metadata table
        buf[mo + 32..mo + 48].copy_from_slice(&[0u8; 16]);
        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_required_metadata_items();
        assert!(result.is_err(), "expected error for missing FileParameters");
        assert!(
            format!("{:?}", result).contains("MetadataRequiredMissing"),
            "expected MetadataRequiredMissing, got: {:?}",
            result
        );
    }

    #[test]
    fn validate_log_empty_ok() {
        let buf = build_test_vhdx();
        // Log region is all zeros → should pass
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_log().is_ok());
    }

    #[test]
    fn validate_file_valid() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_file().is_ok());
    }

    #[test]
    fn has_parent_detection() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        assert!(!validator.has_parent());
    }

    #[test]
    fn spec_validator_new() {
        let data = vec![0u8; 1024 * 1024];
        let v = SpecValidator::new(&data, false);
        assert!(!v.strict);
        let v2 = SpecValidator::new(&data, true);
        assert!(v2.strict);
    }

    // -----------------------------------------------------------------------
    // Strict mode tests (MS-VHDX-宽松扩展标准 §3)
    // -----------------------------------------------------------------------

    /// strict=false, optional unknown region entry → Ok
    #[test]
    fn test_strict_false_optional_unknown_region_passes() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID (required=0)
        // Region table 1 is at 192KB. Current: 2 entries (header 16 bytes + 2*32 = 80 bytes used).
        let rt_offset = 192 * KB;
        // Update entry count: 2 → 3
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        // Write entry 2: unknown GUID, required=0, valid offset/length
        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        // file_offset: 4MB (aligned)
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        // length: 1MB (aligned)
        let length: u32 = 1 * 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        // required: 0 (optional)
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        // Fix CRC for RT1 (zero out checksum field first, then compute)
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..64 * KB]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // Do the same for RT2 at 256KB
        let rt2_offset = 256 * KB;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let entry2_start = rt2_offset + 16 + 2 * 32;
        buf[entry2_start..entry2_start + 16].copy_from_slice(&unknown_guid);
        buf[entry2_start + 16..entry2_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[entry2_start + 24..entry2_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry2_start + 28..entry2_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..64 * KB]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        // Extend buffer to cover the new region offset
        let needed = (offset + u64::from(length)) as usize;
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=false → optional unknown should pass
        let validator = SpecValidator::new(&buf, false);
        assert!(validator.validate_region_table().is_ok());
    }

    /// strict=false, required unknown region entry → Err
    #[test]
    fn test_strict_false_required_unknown_region_fails() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID and required=1
        let rt_offset = 192 * KB;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1 * 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        // required: 1 (required)
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&1u32.to_le_bytes());

        let checksum = crc32c(&{
            let mut slice = vec![0u8; 64 * KB];
            slice.copy_from_slice(&buf[rt_offset..][..64 * KB]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // RT2
        let rt2_offset = 256 * KB;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let entry2_start = rt2_offset + 16 + 2 * 32;
        buf[entry2_start..entry2_start + 16].copy_from_slice(&unknown_guid);
        buf[entry2_start + 16..entry2_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[entry2_start + 24..entry2_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry2_start + 28..entry2_start + 32].copy_from_slice(&1u32.to_le_bytes());
        let checksum2 = crc32c(&{
            let mut slice = vec![0u8; 64 * KB];
            slice.copy_from_slice(&buf[rt2_offset..][..64 * KB]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = (offset + u64::from(length)) as usize;
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=false but required unknown → should still fail
        let validator = SpecValidator::new(&buf, false);
        let result = validator.validate_region_table();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(msg.contains("RegionRequiredUnknown"), "expected RegionRequiredUnknown, got: {msg}");
    }

    /// strict=true, optional unknown region entry → Err
    #[test]
    fn test_strict_true_optional_unknown_region_fails() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID and required=0
        let rt_offset = 192 * KB;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1 * 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        let checksum = crc32c(&{
            let mut slice = vec![0u8; 64 * KB];
            slice.copy_from_slice(&buf[rt_offset..][..64 * KB]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // RT2
        let rt2_offset = 256 * KB;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let entry2_start = rt2_offset + 16 + 2 * 32;
        buf[entry2_start..entry2_start + 16].copy_from_slice(&unknown_guid);
        buf[entry2_start + 16..entry2_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[entry2_start + 24..entry2_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry2_start + 28..entry2_start + 32].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&{
            let mut slice = vec![0u8; 64 * KB];
            slice.copy_from_slice(&buf[rt2_offset..][..64 * KB]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = (offset + u64::from(length)) as usize;
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=true → optional unknown should still fail
        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_region_table();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(msg.contains("RegionOptionalUnknown"), "expected RegionOptionalUnknown, got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Parent locator / chain validation tests
    // -----------------------------------------------------------------------

    /// Build a VHDX buffer whose parent locator contains the given KV pairs.
    ///
    /// The base VHDX is modified to be a differencing disk (has_parent=1) and
    /// the existing empty parent locator metadata entry is replaced with one
    /// pointing to actual locator data at items_base + 48.
    fn build_vhdx_with_parent_locator(kvs: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = build_test_vhdx();

        // Locate the metadata region from region table 1 (at 192 KB).
        // RT: 16-byte header + 2×32-byte entries. Entry 1 (metadata) starts at offset 48.
        let rt_offset = 192 * KB;
        let metadata_offset = u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = metadata_offset as usize;
        let items_base = mo + 64 * KB; // items start after the 64 KB metadata table

        // Mark the disk as differencing: set FileParameters has_parent bit (bit 1).
        buf[items_base..items_base + 8].view_bits_mut::<Lsb0>().set(1, true);

        // Existing items occupy bytes 0..44 of the items area.
        // Place parent locator data right after, at offset 48 (4-byte aligned).
        let pl_start = items_base + 48;
        let pl_region_off = (pl_start - mo) as u32; // offset within metadata region

        // -- Build parent locator data ---------------------------------------
        let loc_hdr_size = 20usize;
        let kv_entry_size = 12usize;
        let num = kvs.len();
        let kv_tab_size = num * kv_entry_size;
        let kv_dat_base = loc_hdr_size + kv_tab_size;

        // Encode keys & values to UTF-16LE.
        struct Encoded {
            key: Vec<u8>,
            val: Vec<u8>,
        }
        let encoded: Vec<Encoded> = kvs
            .iter()
            .map(|(k, v)| Encoded {
                key: k.encode_utf16().flat_map(|c| c.to_le_bytes()).collect(),
                val: v.encode_utf16().flat_map(|c| c.to_le_bytes()).collect(),
            })
            .collect();

        let total_kv: usize = encoded.iter().map(|e| e.key.len() + e.val.len()).sum();
        let pl_size = kv_dat_base + total_kv;

        // Grow buffer if needed.
        let end = pl_start + pl_size;
        if buf.len() < end {
            buf.resize(end, 0);
        }

        // Write locator header (20 bytes).
        let pl = &mut buf[pl_start..pl_start + pl_size];
        pl[0..16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
        pl[16..18].copy_from_slice(&0u16.to_le_bytes()); // reserved
        pl[18..20].copy_from_slice(&(num as u16).to_le_bytes()); // entry count

        // Write KV entries and data.
        let mut kv_off = kv_dat_base;
        for (i, e) in encoded.iter().enumerate() {
            let entry_off = loc_hdr_size + i * kv_entry_size;
            let key_off = kv_off as u32;
            let val_off = (kv_off + e.key.len()) as u32;
            pl[entry_off..entry_off + 4].copy_from_slice(&key_off.to_le_bytes());
            pl[entry_off + 4..entry_off + 8].copy_from_slice(&val_off.to_le_bytes());
            pl[entry_off + 8..entry_off + 10]
                .copy_from_slice(&(e.key.len() as u16).to_le_bytes());
            pl[entry_off + 10..entry_off + 12]
                .copy_from_slice(&(e.val.len() as u16).to_le_bytes());

            pl[kv_off..kv_off + e.key.len()].copy_from_slice(&e.key);
            kv_off += e.key.len();
            pl[kv_off..kv_off + e.val.len()].copy_from_slice(&e.val);
            kv_off += e.val.len();
        }

        // Update the ParentLocator metadata table entry (entry index 5).
        let pl_entry = mo + 32 + 5 * 32;
        buf[pl_entry + 16..pl_entry + 20].copy_from_slice(&pl_region_off.to_le_bytes());
        buf[pl_entry + 20..pl_entry + 24].copy_from_slice(&(pl_size as u32).to_le_bytes());

        buf
    }

    #[test]
    fn test_validate_parent_locator_corrupt_key() {
        // Build a VHDX with a parent locator containing a valid KV entry,
        // then manually corrupt the key_length to an odd value so that
        // UTF-16LE decoding fails.
        let mut buf = build_vhdx_with_parent_locator(&[(
            "parent_linkage",
            "{00000000-0000-0000-0000-000000000000}",
        )]);

        // Locator data is at items_base + 48. KV entry 0 starts at
        // pl_start + 20 (after 20-byte locator header). key_length is at [8..10].
        let rt_offset = 192 * KB;
        let metadata_offset = u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = metadata_offset as usize;
        let kv0_off = mo + 64 * KB + 48 + 20;
        buf[kv0_off + 8..kv0_off + 10].copy_from_slice(&5u16.to_le_bytes()); // odd length

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_parent_locator();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(
            msg.contains("odd byte length"),
            "expected 'odd byte length', got: {msg}"
        );
    }

    #[test]
    fn test_validate_parent_chain_rejects_linkage2() {
        // Parent locator with both parent_linkage and parent_linkage2.
        // validate_parent_chain should reject parent_linkage2 before file I/O.
        let buf = build_vhdx_with_parent_locator(&[
            ("parent_linkage", "{01234567-89ab-cdef-0123-456789abcdef}"),
            ("parent_linkage2", "{00000000-0000-0000-0000-000000000000}"),
            ("relative_path", "child.vhdx"),
        ]);

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_parent_chain();
        assert!(result.is_err());
        let msg = format!("{:?}", result.err().unwrap());
        assert!(
            msg.contains("ParentLocatorLinkage2Conflict"),
            "expected ParentLocatorLinkage2Conflict, got: {msg}"
        );
    }

    #[test]
    fn test_validate_parent_chain_unparseable_linkage() {
        // Parent locator with parent_linkage value that is not a valid GUID.
        let buf = build_vhdx_with_parent_locator(&[
            ("parent_linkage", "not-a-guid"),
            ("relative_path", "child.vhdx"),
        ]);

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_parent_chain();
        assert!(result.is_err());
        let msg = format!("{:?}", result.err().unwrap());
        assert!(
            msg.contains("not a valid GUID"),
            "expected 'not a valid GUID', got: {msg}"
        );
    }

    #[test]
    fn test_from_file_constructor() -> Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-from-file.vhdx");

        // Create a valid dynamic VHDX file.
        crate::file::File::create(&path)
            .size(256 * 1024 * 1024) // 256 MB
            .block_size(32 * 1024 * 1024)
            .logical_sector_size(4096)
            .finish()?;

        // Open it.
        let file = crate::file::File::open(&path).finish()?;

        // Construct SpecValidator via from_file.
        let validator = SpecValidator::from_file(&file)?;

        // Verify that sub-validators actually execute (don't silently skip).
        validator.validate_bat()?;
        validator.validate_metadata()?;
        validator.validate_log()?;

        Ok(())
    }

    /// `File::create()` writes the same sequence number (0) to both headers,
    /// which the validator rejects. Patch header 2 to have seq=1 so validation
    /// can proceed.
    fn patch_header2_sequence(path: &std::path::Path) -> std::io::Result<()> {
        use std::io::{Read, Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path)?;

        // Read header 2 (4 KB at offset 128 KB).
        let mut h2 = [0u8; 4096];
        f.seek(SeekFrom::Start(128 * 1024))?;
        f.read_exact(&mut h2)?;

        // Bump sequence number from 0 to 1.
        h2[8..16].copy_from_slice(&1u64.to_le_bytes());

        // Recompute CRC-32C.
        h2[4..8].copy_from_slice(&[0u8; 4]);
        let crc = crc32c(&h2);
        h2[4..8].copy_from_slice(&crc.to_le_bytes());

        f.seek(SeekFrom::Start(128 * 1024))?;
        f.write_all(&h2)?;

        Ok(())
    }

    #[test]
    fn test_validate_file_covers_parent_chain() -> Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-parent-chain.vhdx");

        // Create a dynamic disk (no parent — not a differencing disk).
        crate::file::File::create(&path)
            .size(256 * 1024 * 1024)
            .finish()?;

        // Patch header 2 sequence number so the validator passes.
        patch_header2_sequence(&path)?;

        let file = crate::file::File::open(&path).finish()?;
        // validate_file should pass (no parent chain for a non-differencing disk).
        file.validator().validate_file()?;

        Ok(())
    }

    #[test]
    fn test_file_validator_integration() -> Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test-validator-int.vhdx");

        // Create a valid dynamic VHDX.
        crate::file::File::create(&path)
            .size(256 * 1024 * 1024)
            .block_size(32 * 1024 * 1024)
            .logical_sector_size(4096)
            .physical_sector_size(4096)
            .finish()?;

        // Patch header 2 sequence number so the validator passes.
        patch_header2_sequence(&path)?;

        // Open and validate.
        let file = crate::file::File::open(&path).finish()?;
        file.validator().validate_file()?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // ValidationIssue collection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_valid_vhdx_no_issues() {
        let buf = build_test_vhdx();
        let validator = SpecValidator::new(&buf, true);
        let issues = validator.validate_file().unwrap();
        assert!(issues.is_empty(), "valid VHDX should produce no issues");
    }

    #[test]
    fn test_optional_unknown_region_pushes_issue() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID (required=0)
        let rt_offset = 192 * KB;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1 * 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        // Fix CRC for RT1
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..64 * KB]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // Fix CRC for RT2
        let rt2_offset = 256 * KB;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let entry2_start = rt2_offset + 16 + 2 * 32;
        buf[entry2_start..entry2_start + 16].copy_from_slice(&unknown_guid);
        buf[entry2_start + 16..entry2_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[entry2_start + 24..entry2_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry2_start + 28..entry2_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..64 * KB]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        // Extend buffer to cover the new region
        let needed = (offset + u64::from(length)) as usize;
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=false → optional unknown passes but should push an issue
        let validator = SpecValidator::new(&buf, false);
        let issues = validator.validate_region_table().unwrap();
        assert!(!issues.is_empty(), "expected at least one issue for optional unknown region");
        let found = issues.iter().any(|i| i.code() == "REGION_OPTIONAL_UNKNOWN");
        assert!(found, "expected REGION_OPTIONAL_UNKNOWN issue, got: {:?}", issues.iter().map(|i| i.code()).collect::<Vec<_>>());

        // Verify issue fields
        let issue = issues.iter().find(|i| i.code() == "REGION_OPTIONAL_UNKNOWN").unwrap();
        assert_eq!(issue.section(), "region_table");
        assert_eq!(issue.spec_ref(), "RELAX");
        assert!(issue.message().contains("tolerated"));
    }

    #[test]
    fn test_optional_unknown_metadata_pushes_issue() {
        let mut buf = build_test_vhdx();

        // Get metadata offset from region table
        let rt_offset = 192 * KB;
        let metadata_offset = u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = metadata_offset as usize;

        // The metadata table has 6 entries. Change entry count to 7 and add an unknown optional one.
        buf[mo + 10..mo + 12].copy_from_slice(&7u16.to_le_bytes());

        // Write a 7th entry at the next slot (entries start at offset 32, each 32 bytes)
        let entry_off = mo + 32 + 6 * 32;
        let unknown_guid: [u8; 16] = [
            0xDE, 0xAD, 0xBE, 0xEF, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_off..entry_off + 16].copy_from_slice(&unknown_guid);
        // offset=0, length=0 (empty optional entry)
        buf[entry_off + 16..entry_off + 20].copy_from_slice(&0u32.to_le_bytes());
        buf[entry_off + 20..entry_off + 24].copy_from_slice(&0u32.to_le_bytes());
        // flags: NOT required (bit 28 = 0) → optional
        buf[entry_off + 24..entry_off + 28].copy_from_slice(&0u32.to_le_bytes());
        buf[entry_off + 28..entry_off + 32].copy_from_slice(&0u32.to_le_bytes());

        // strict=false → optional unknown metadata passes but should push an issue
        let validator = SpecValidator::new(&buf, false);
        let issues = validator.validate_metadata().unwrap();
        let found = issues.iter().any(|i| i.code() == "METADATA_OPTIONAL_UNKNOWN");
        assert!(found, "expected METADATA_OPTIONAL_UNKNOWN issue, got: {:?}", issues.iter().map(|i| i.code()).collect::<Vec<_>>());

        let issue = issues.iter().find(|i| i.code() == "METADATA_OPTIONAL_UNKNOWN").unwrap();
        assert_eq!(issue.section(), "metadata");
        assert_eq!(issue.spec_ref(), "RELAX");
    }

    #[test]
    fn test_strict_true_no_issue_for_optional_unknown_region() {
        // strict=true: optional unknown should Err, not push issue
        let mut buf = build_test_vhdx();

        let rt_offset = 192 * KB;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1 * 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..64 * KB]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        let rt2_offset = 256 * KB;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let entry2_start = rt2_offset + 16 + 2 * 32;
        buf[entry2_start..entry2_start + 16].copy_from_slice(&unknown_guid);
        buf[entry2_start + 16..entry2_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[entry2_start + 24..entry2_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry2_start + 28..entry2_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..64 * KB]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = (offset + u64::from(length)) as usize;
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_region_table().is_err());
    }

    // -----------------------------------------------------------------------
    // Reserved field validation tests (MS-VHDX §2.6.1.2 / §2.6.2.1)
    // -----------------------------------------------------------------------

    #[test]
    fn test_metadata_entry_reserved_nonzero() {
        let mut buf = build_test_vhdx();

        // Get metadata offset from region table
        let rt_offset = 192 * KB;
        let metadata_offset = u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = metadata_offset as usize;

        // Entry 0 (FileParameters) reserved field is at entry start (offset+32) + 28
        let reserved_off = mo + 32 + 28;
        buf[reserved_off..reserved_off + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_metadata();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(
            msg.contains("METADATA_ENTRY_RESERVED_NONZERO"),
            "expected METADATA_ENTRY_RESERVED_NONZERO, got: {msg}"
        );
    }

    #[test]
    fn test_metadata_file_parameters_reserved_flags() {
        let mut buf = build_test_vhdx();

        // Get metadata offset from region table
        let rt_offset = 192 * KB;
        let metadata_offset = u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = metadata_offset as usize;

        // FileParameters data starts at mo + 64KB (after the 64KB metadata table)
        let fp_data_off = mo + 64 * KB;
        // Set bit 2 of BitFields (bit 34 in the 8-byte Lsb0 view), which falls
        // in reserved bits 2-31. Per MS-VHDX §2.6.2.1 these bits MUST be 0;
        // BitFields is the second u32 (bytes 4-7, bits 32-63).
        buf[fp_data_off..fp_data_off + 8].view_bits_mut::<Lsb0>().set(34, true);

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_required_metadata_items();
        assert!(result.is_ok(), "expected Ok despite reserved flags, got: {result:?}");
        let issues = result.unwrap();
        assert!(
            issues.iter().any(|i| i.code() == "METADATA_FILE_PARAMETERS_RESERVED_FLAGS"),
            "expected METADATA_FILE_PARAMETERS_RESERVED_FLAGS issue"
        );
    }

    #[test]
    fn validate_header_accepts_single_valid_header() {
        // Build VHDX with header1 valid, header2 corrupted
        let mut buf = build_test_vhdx();
        // Corrupt header2 signature at offset 128 KB
        buf[128 * KB] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        // Should succeed — single valid header is OK per MS-VHDX §2.2.2
        assert!(
            validator.validate_header().is_ok(),
            "validate_header should accept single valid header"
        );
    }
}
