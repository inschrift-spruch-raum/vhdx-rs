//! VHDX specification compliance validator (read-only).
//!
//! `SpecValidator` performs structural validation against MS-VHDX and
//! companion standards. All validation is non-destructive and does not
//! modify file state.
//!
//! # Standard references
//!
//! - MS-VHDX (baseline specification)
//! - MS-VHDX-`校验扩展标准` (this module's error code dictionary)
//! - MS-VHDX-`宽松扩展标准` (permissive validation, RELAX)
//! - MS-VHDX-`只读扩展标准` (read-only semantics, ROEXT)

use crate::constants::{BAT_REGION_GUID, METADATA_REGION_GUID, MIB};
use crate::error::{Error, Result, SignaturePosition};
use crate::file::{is_known_metadata_guid, is_known_region_guid};
use crate::header::{Header, HeaderStructure};
use crate::types::{Guid, StandardItems};
use crate::{bat::PayloadBlockState, bat::SectorBitmapState};

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
        section: &'static str, code: &'static str, message: impl Into<String>,
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
    #[must_use]
    pub fn section(&self) -> &'static str {
        self.section
    }

    /// Standardised error code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Human-readable description.
    #[must_use]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// Specification reference.
    #[must_use]
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
    pub(crate) fn from_file(file: &'a crate::file::File) -> Self {
        Self::new(file.validator_buf(), file.is_strict()).with_child_path(file.path().to_path_buf())
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
                format!("header {header_idx} version {version} is not supported (expected 1)"),
                "MS-VHDX/2.2.2",
            ),
            Error::UnsupportedLogVersion { version } => ValidationIssue::new(
                "header",
                "HEADER_LOG_VERSION_UNSUPPORTED",
                format!("header {header_idx} log version {version} is not supported (expected 0)"),
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
    ///
    /// # Errors
    ///
    /// Returns the first hard validation error from a sub-validation stage.
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
    /// - `LogGuid` consistency between headers
    ///
    /// # Errors
    ///
    /// Returns an error when required header invariants fail.
    ///
    /// # Panics
    ///
    /// Panics on internal invariant violations where code unwraps a known error
    /// branch after `is_ok()` checks.
    pub fn validate_header(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let header = self.parse_header()?;
        Self::validate_file_type_identifier(&header, &mut issues)?;
        Self::validate_header_pair(&header, &mut issues)?;
        Self::validate_log_alignment(&header, &mut issues)?;

        Ok(issues)
    }

    fn validate_file_type_identifier(
        header: &Header<'a>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let ft = header.file_type();
        if ft.signature() == b"vhdxfile" {
            return Ok(());
        }
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "header",
                "HEADER_FILE_TYPE_ID_INVALID",
                format!(
                    "invalid signature at offset 0: expected \"vhdxfile\", found {:?}",
                    std::str::from_utf8(ft.signature()).unwrap_or("<binary>")
                ),
                "MS-VHDX/2.2.1",
            ),
        );
        Err(Error::InvalidSignature {
            position: SignaturePosition::FileTypeIdentifier,
            expected: *b"vhdxfile",
            found: *ft.signature(),
        })
    }

    fn validate_header_pair(header: &Header<'a>, issues: &mut Vec<ValidationIssue>) -> Result<()> {
        let v1 = header
            .header(1)
            .and_then(|h| Self::validate_single_header(Ok(h)));
        let v2 = header
            .header(2)
            .and_then(|h| Self::validate_single_header(Ok(h)));
        let h1_valid = v1.is_ok();
        let h2_valid = v2.is_ok();
        if !h1_valid && !h2_valid {
            Self::push_header_issue(issues, 1, v1.as_ref().err().unwrap());
            Self::push_header_issue(issues, 2, v2.as_ref().err().unwrap());
            return Err(Error::CorruptedHeader("both headers are invalid".into()));
        }
        if !h1_valid {
            Self::push_header_issue(issues, 1, v1.as_ref().err().unwrap());
        }
        if !h2_valid {
            Self::push_header_issue(issues, 2, v2.as_ref().err().unwrap());
        }
        if h1_valid && h2_valid {
            Self::validate_header_pair_consistency(header, issues, &v1.unwrap(), &v2.unwrap())?;
        }
        Ok(())
    }

    fn validate_header_pair_consistency(
        header: &Header<'a>, issues: &mut Vec<ValidationIssue>, v1: &HeaderStructure<'a>,
        v2: &HeaderStructure<'a>,
    ) -> Result<()> {
        if v1.sequence_number() == v2.sequence_number() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "header",
                    "HEADER_SEQUENCE_NUMBER_INVALID",
                    "both headers have same sequence number",
                    "MS-VHDX/2.2.2",
                ),
            );
            return Err(Error::HeaderSequenceNumberInvalid {
                sequence_number_1: v1.sequence_number(),
                sequence_number_2: v2.sequence_number(),
            });
        }
        let log_guid = Self::current_log_guid(header)?;
        if log_guid == v1.log_guid() && log_guid == v2.log_guid() {
            return Ok(());
        }
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "header",
                "HEADER_LOG_GUID_MISMATCH",
                "LogGuid differs between headers",
                "MS-VHDX/2.2.2",
            ),
        );
        Err(Error::HeaderLogGuidMismatch {
            header1_log_guid: v1.log_guid(),
            header2_log_guid: v2.log_guid(),
        })
    }

    fn validate_log_alignment(
        header: &Header<'a>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let current = header.header(0)?;
        let log_offset = current.log_offset();
        let log_length = current.log_length();
        if log_length > 0 && u64::from(log_length) % u64::from(MIB) != 0 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "header",
                    "HEADER_LOG_LENGTH_NOT_ALIGNED",
                    format!("log_length {log_length} is not a multiple of 1MB"),
                    "MS-VHDX/2.2.2",
                ),
            );
            return Err(Error::HeaderLogNotAligned {
                field: "log_length".to_string(),
                value: u64::from(log_length),
            });
        }
        if log_offset > 0 && log_offset % u64::from(MIB) != 0 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "header",
                    "HEADER_LOG_OFFSET_NOT_ALIGNED",
                    format!("log_offset {log_offset} is not a multiple of 1MB"),
                    "MS-VHDX/2.2.2",
                ),
            );
            return Err(Error::HeaderLogNotAligned {
                field: "log_offset".to_string(),
                value: log_offset,
            });
        }
        Ok(())
    }

    /// Validate a single header structure (signature, CRC, version, `log_version`).
    fn validate_single_header(result: Result<HeaderStructure<'a>>) -> Result<HeaderStructure<'a>> {
        let mut issues = Vec::new();
        let h = result?;

        // Signature check is done by Header::validate_header_at (returns
        // CorruptedHeader on mismatch). Version and log_version are additional
        // checks performed here.

        // Version must be 1
        if h.version() != 1 {
            Self::push_issue(
                &mut issues,
                ValidationIssue::new(
                    "header",
                    "HEADER_VERSION_UNSUPPORTED",
                    format!("version {} is not supported (expected 1)", h.version()),
                    "MS-VHDX/2.2.2",
                ),
            );
            return Err(Error::UnsupportedVersion {
                version: h.version(),
            });
        }

        // Log version must be 0 (MS-VHDX §2.2.2: MUST NOT continue UNLESS LogGuid==0)
        if h.log_version() != 0 && h.log_guid() != Guid::zero() {
            Self::push_issue(
                &mut issues,
                ValidationIssue::new(
                    "header",
                    "HEADER_LOG_VERSION_UNSUPPORTED",
                    format!(
                        "log version {} is not supported (expected 0)",
                        h.log_version()
                    ),
                    "MS-VHDX/2.2.2",
                ),
            );
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
    ///
    /// # Errors
    ///
    /// Returns an error when region table integrity checks fail.
    ///
    /// # Panics
    ///
    /// Panics on internal invariant violations where code unwraps region tables
    /// after prior successful checks.
    pub fn validate_region_table(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let header = self.parse_header()?;

        // Check both region tables.
        let rt1 = header.region_table(1);
        let rt2 = header.region_table(2);

        match (&rt1, &rt2) {
            (Err(e), _) | (_, Err(e)) => {
                if let Error::InvalidSignature {
                    position: SignaturePosition::RegionTable,
                    ..
                } = e
                {
                    Self::push_issue(
                        &mut issues,
                        ValidationIssue::new(
                            "region_table",
                            "REGION_SIGNATURE_INVALID",
                            format!("region table signature error: {e}"),
                            "MS-VHDX/2.2.3.1",
                        ),
                    );
                    return Err(Error::InvalidRegionTable(format!("{e}")));
                }
                Self::push_issue(
                    &mut issues,
                    ValidationIssue::new(
                        "region_table",
                        "REGION_CHECKSUM_MISMATCH",
                        format!("{e}"),
                        "MS-VHDX/2.2.3.1",
                    ),
                );
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
                Self::push_issue(
                    &mut issues,
                    ValidationIssue::new(
                        "region_table",
                        "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM",
                        format!("region table {idx} entry count {count} exceeds maximum of 2047"),
                        "MS-VHDX/2.2.3.1",
                    ),
                );
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM: region table {idx} entry count {count} exceeds maximum of 2047"
                )));
            }
        }

        // Check entries for alignment and overlap in the CURRENT region table.
        let current_rt = match header.region_table(0) {
            Ok(rt) => rt,
            Err(e) => {
                Self::push_issue(
                    &mut issues,
                    ValidationIssue::new(
                        "region_table",
                        "REGION_CHECKSUM_MISMATCH",
                        format!("current region table: {e}"),
                        "MS-VHDX/2.2.3.1",
                    ),
                );
                return Err(Error::InvalidRegionTable(format!(
                    "current region table: {e}"
                )));
            }
        };

        issues.extend(self.validate_region_entries(&current_rt)?);

        Ok(issues)
    }

    /// Validate a region table's entries for alignment, overlap, and required-unknown.
    fn validate_region_entries(
        &self, rt: &crate::header::RegionTable<'a>,
    ) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let entries: Vec<_> = rt.entries().collect();

        for (i, entry) in entries.iter().enumerate() {
            self.validate_region_entry(i, entry, &entries, &mut issues)?;
        }

        Ok(issues)
    }

    fn validate_region_entry(
        &self, i: usize, entry: &crate::header::RegionTableEntry<'a>,
        entries: &[crate::header::RegionTableEntry<'a>], issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let file_offset = entry.file_offset();
        let length = entry.length();

        if !file_offset.is_multiple_of(u64::from(MIB)) {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_ALIGNMENT",
                    format!("entry {i} file_offset {file_offset:#x} not 1MB-aligned"),
                    "MS-VHDX/2.2.3.2",
                ),
            );
            return Err(Error::InvalidRegionTable(format!(
                "REGION_ENTRY_ALIGNMENT: entry {i} file_offset {file_offset:#x} not 1MB-aligned"
            )));
        }
        if file_offset < u64::from(MIB) {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_OFFSET_MINIMUM",
                    format!("entry {i} file_offset {file_offset} < 1MB minimum"),
                    "MS-VHDX/2.2.3.2",
                ),
            );
            return Err(Error::InvalidRegionTable(format!(
                "REGION_ENTRY_OFFSET_MINIMUM: entry {i} file_offset {file_offset} < 1MB minimum"
            )));
        }
        if u64::from(length) % u64::from(MIB) != 0 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "region_table",
                    "REGION_ENTRY_ALIGNMENT",
                    format!("entry {i} length {length} not 1MB-aligned"),
                    "MS-VHDX/2.2.3.2",
                ),
            );
            return Err(Error::InvalidRegionTable(format!(
                "REGION_ENTRY_ALIGNMENT: entry {i} length {length} not 1MB-aligned"
            )));
        }

        Self::validate_region_entry_overlap(i, file_offset, length, entries, issues)?;
        self.validate_region_entry_guid(entry, issues)
    }

    fn validate_region_entry_overlap(
        i: usize, file_offset: u64, length: u32, entries: &[crate::header::RegionTableEntry<'a>],
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let end = file_offset + u64::from(length);
        for (j, prev) in entries[..i].iter().enumerate() {
            let prev_end = prev.file_offset() + u64::from(prev.length());
            if file_offset < prev_end && prev.file_offset() < end {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "region_table",
                        "REGION_ENTRY_OVERLAP",
                        format!("entries {j} and {i} overlap"),
                        "MS-VHDX/2.1",
                    ),
                );
                return Err(Error::InvalidRegionTable(format!(
                    "REGION_ENTRY_OVERLAP: entries {j} and {i} overlap"
                )));
            }
        }
        Ok(())
    }

    fn validate_region_entry_guid(
        &self, entry: &crate::header::RegionTableEntry<'a>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if is_known_region_guid(&entry.guid()) {
            return Ok(());
        }
        if entry.required() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "region_table",
                    "REGION_REQUIRED_UNKNOWN",
                    format!("required unknown region GUID {}", entry.guid()),
                    "RELAX",
                ),
            );
            return Err(Error::RegionRequiredUnknown { guid: entry.guid() });
        }
        if self.strict {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "region_table",
                    "REGION_OPTIONAL_UNKNOWN",
                    format!(
                        "optional unknown region GUID {} in strict mode",
                        entry.guid()
                    ),
                    "RELAX",
                ),
            );
            return Err(Error::RegionOptionalUnknown { guid: entry.guid() });
        }
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "region_table",
                "REGION_OPTIONAL_UNKNOWN",
                format!(
                    "optional unknown region GUID {} tolerated in non-strict mode",
                    entry.guid()
                ),
                "RELAX",
            ),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // BAT validation
    // -----------------------------------------------------------------------

    /// Validate the Block Allocation Table.
    ///
    /// Checks:
    /// - Entry states are valid values
    /// - State matches disk type (e.g., fixed disk has no Unmapped)
    /// - Sector bitmap entries in non-differencing disks are `NotPresent`
    /// - File offsets are aligned
    ///
    /// # Errors
    ///
    /// Returns an error when BAT structure or state rules are violated.
    ///
    /// # Panics
    ///
    /// Panics if integer conversion for minimum BAT entry count overflows `usize`
    /// (should not occur with valid metadata).
    pub fn validate_bat(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(bat_data) = self.bat_region() else {
            return Ok(issues);
        };

        let chunk_ratio = self.chunk_ratio();
        if chunk_ratio == 0 {
            return Ok(issues); // Cannot validate without chunk ratio
        }

        let bat = crate::bat::Bat::new(bat_data, chunk_ratio);
        let has_parent = self.has_parent();
        let block_size = u64::from(self.block_size());

        self.validate_bat_entry_count(&bat, block_size, &mut issues)?;
        let mut seen_offsets = std::collections::HashSet::new();
        for entry in bat.entries() {
            Self::validate_bat_entry(
                entry,
                has_parent,
                block_size,
                &mut seen_offsets,
                &mut issues,
            )?;
        }
        Self::validate_bat_sector_bitmap_consistency(&bat, has_parent, chunk_ratio, &mut issues);

        Ok(issues)
    }

    fn validate_bat_entry_count(
        &self, bat: &crate::bat::Bat<'_>, block_size: u64, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let virtual_disk_size = self.virtual_disk_size();
        if virtual_disk_size > 0 && block_size > 0 {
            let min_entries = virtual_disk_size.div_ceil(block_size);
            if bat.len() < usize::try_from(min_entries).expect("minimum BAT entries fit usize") {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "bat",
                        "BAT_ENTRY_COUNT_INSUFFICIENT",
                        format!(
                            "BAT has {} entries but virtual disk requires at least {}",
                            bat.len(),
                            min_entries
                        ),
                        "MS-VHDX/2.5",
                    ),
                );
                return Err(Error::BatEntryCountInsufficient {
                    actual: bat.len() as u64,
                    expected: min_entries,
                });
            }
        }
        Ok(())
    }

    fn validate_bat_entry(
        entry: crate::bat::BatEntry<'_>, has_parent: bool, block_size: u64,
        seen_offsets: &mut std::collections::HashSet<u64>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let raw_state = entry.raw_state();
        if entry.is_sector_bitmap() {
            return Self::validate_bat_sector_bitmap_entry(raw_state, entry, has_parent, issues);
        }
        Self::validate_bat_payload_entry(
            raw_state,
            entry,
            has_parent,
            block_size,
            seen_offsets,
            issues,
        )
    }

    fn validate_bat_sector_bitmap_entry(
        raw_state: u8, entry: crate::bat::BatEntry<'_>, has_parent: bool,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let Some(sb_state) = entry.sector_bitmap_state() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "bat",
                    "BAT_SECTOR_BITMAP_INVALID_STATE",
                    format!("invalid sector bitmap state: {raw_state}"),
                    "MS-VHDX/2.5.1.2",
                ),
            );
            return Err(Error::InvalidSectorBitmapState(raw_state));
        };
        if !has_parent && sb_state != SectorBitmapState::NotPresent {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "bat",
                    "BAT_ENTRY_STATE_MISMATCH",
                    "sector bitmap state not NotPresent on non-differencing disk".to_string(),
                    "MS-VHDX/2.5.1.1",
                ),
            );
            return Err(Error::StateMismatch {
                state: raw_state,
                description: "sector bitmap state not NotPresent on non-differencing disk".into(),
            });
        }
        Ok(())
    }

    fn validate_bat_payload_entry(
        raw_state: u8, entry: crate::bat::BatEntry<'_>, has_parent: bool, block_size: u64,
        seen_offsets: &mut std::collections::HashSet<u64>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let Some(p_state) = entry.payload_state() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "bat",
                    "BAT_ENTRY_INVALID_STATE",
                    format!("invalid payload block state: {raw_state}"),
                    "MS-VHDX/2.5.1.1",
                ),
            );
            return Err(Error::InvalidBlockState(raw_state));
        };
        Self::validate_bat_payload_state_for_disk_type(raw_state, p_state, has_parent, issues)?;
        Self::validate_bat_payload_offset_alignment(entry, p_state, block_size, issues)?;
        Self::validate_bat_payload_offset_uniqueness(entry, p_state, seen_offsets, issues)
    }

    fn validate_bat_payload_offset_alignment(
        entry: crate::bat::BatEntry<'_>, p_state: PayloadBlockState, block_size: u64,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        match p_state {
            PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent => {
                let offset_mb = entry.file_offset_mb();
                if block_size > 0 && offset_mb != 0 {
                    let offset_bytes = offset_mb * 1024 * 1024;
                    if !offset_bytes.is_multiple_of(block_size) {
                        Self::push_issue(
                            issues,
                            ValidationIssue::new(
                                "bat",
                                "BAT_ENTRY_FILE_OFFSET_UNALIGNED",
                                format!(
                                    "payload block file offset {offset_mb} MB ({offset_bytes} bytes) not aligned to block size {block_size}"
                                ),
                                "MS-VHDX/2.5",
                            ),
                        );
                        return Err(Error::BatFileOffsetUnaligned {
                            offset_mb,
                            block_size: u32::try_from(block_size).unwrap_or(u32::MAX),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_bat_payload_state_for_disk_type(
        raw_state: u8, p_state: PayloadBlockState, has_parent: bool,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if !has_parent {
            match p_state {
                PayloadBlockState::Unmapped | PayloadBlockState::PartiallyPresent => {
                    Self::push_issue(
                        issues,
                        ValidationIssue::new(
                            "bat",
                            "BAT_ENTRY_STATE_MISMATCH",
                            "payload state Unmapped/PartiallyPresent on non-differencing disk"
                                .to_string(),
                            "MS-VHDX/2.5.1.1",
                        ),
                    );
                    return Err(Error::StateMismatch {
                        state: raw_state,
                        description:
                            "payload state Unmapped/PartiallyPresent on non-differencing disk"
                                .into(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_bat_payload_offset_uniqueness(
        entry: crate::bat::BatEntry<'_>, p_state: PayloadBlockState,
        seen_offsets: &mut std::collections::HashSet<u64>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        match p_state {
            PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent => {
                let offset_mb = entry.file_offset_mb();
                if offset_mb != 0 && !seen_offsets.insert(offset_mb) {
                    Self::push_issue(
                        issues,
                        ValidationIssue::new(
                            "bat",
                            "BAT_FILE_OFFSET_DUPLICATE",
                            format!("duplicate file_offset_mb {offset_mb} in BAT"),
                            "MS-VHDX/2.5",
                        ),
                    );
                    return Err(Error::BatFileOffsetDuplicate { offset_mb });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_bat_sector_bitmap_consistency(
        bat: &crate::bat::Bat<'_>, has_parent: bool, chunk_ratio: u64,
        issues: &mut Vec<ValidationIssue>,
    ) {
        if !has_parent {
            return;
        }
        let stride = chunk_ratio + 1;
        let total_entries = bat.len() as u64;
        let num_chunks = total_entries / stride;
        for chunk_idx in 0..num_chunks {
            if !Self::chunk_has_partially_present_payload(
                bat,
                chunk_idx,
                stride,
                chunk_ratio,
                total_entries,
            ) {
                continue;
            }
            let sb_bat_idx = chunk_idx * stride + chunk_ratio;
            if sb_bat_idx >= total_entries {
                break;
            }
            let Ok(sb_entry) = bat.entry(sb_bat_idx) else {
                break;
            };
            let sb_state = sb_entry.sector_bitmap_state();
            if !matches!(sb_state, Some(crate::bat::SectorBitmapState::Present)) {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "bat",
                        "BAT_SECTOR_BITMAP_INVALID_STATE",
                        format!(
                            "chunk {chunk_idx}: payload entry is PartiallyPresent but sector bitmap state is {sb_state:?}"
                        ),
                        "MS-VHDX/2.5.1.2",
                    ),
                );
            }
        }
    }

    fn chunk_has_partially_present_payload(
        bat: &crate::bat::Bat<'_>, chunk_idx: u64, stride: u64, chunk_ratio: u64,
        total_entries: u64,
    ) -> bool {
        for payload_offset_in_chunk in 0..chunk_ratio {
            let payload_bat_idx = chunk_idx * stride + payload_offset_in_chunk;
            if payload_bat_idx >= total_entries {
                break;
            }
            let Ok(payload_entry) = bat.entry(payload_bat_idx) else {
                continue;
            };
            if !payload_entry.is_sector_bitmap()
                && let Some(crate::bat::PayloadBlockState::PartiallyPresent) =
                    payload_entry.payload_state()
            {
                return true;
            }
        }
        false
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
    ///
    /// # Errors
    ///
    /// Returns an error when metadata table or item constraints are violated.
    ///
    /// # Panics
    ///
    /// Panics if checked integer conversions for metadata range bookkeeping are
    /// violated unexpectedly.
    pub fn validate_metadata(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(meta_data) = self.metadata_region() else {
            return Ok(issues);
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let table = meta.table();

        Self::validate_metadata_header_checks(&table, &mut issues)?;
        let mut ranges: Vec<(u32, u32, Guid)> = Vec::new();
        for entry in table.entries() {
            self.validate_metadata_entry(&entry, meta_data.len(), &mut ranges, &mut issues)?;
        }
        Self::validate_metadata_ranges_overlap(&ranges, &mut issues)?;
        Self::push_corrupted_known_metadata_items(&table, &mut issues);

        Ok(issues)
    }

    fn validate_metadata_header_checks(
        table: &crate::metadata::MetadataTable<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if let Err(e) = table.header().validate_signature() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_TABLE_SIGNATURE_INVALID",
                    format!("{e}"),
                    "MS-VHDX/2.6.1.1",
                ),
            );
            return Err(e);
        }
        let entry_count = table.header().entry_count();
        if entry_count > 2047 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_ENTRY_INVALID",
                    format!("entry count {entry_count} > 2047"),
                    "MS-VHDX/2.6.1.2",
                ),
            );
            return Err(Error::InvalidMetadata(format!(
                "METADATA_ENTRY_INVALID: entry count {entry_count} > 2047"
            )));
        }
        Ok(())
    }

    fn validate_metadata_entry(
        &self, entry: &crate::metadata::TableEntry<'_>, region_len: usize,
        ranges: &mut Vec<(u32, u32, Guid)>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let offset = entry.offset() as usize;
        let length = entry.length() as usize;
        Self::validate_metadata_offset_and_length(
            entry, offset, length, region_len, ranges, issues,
        )?;
        Self::validate_metadata_entry_reserved_flags(entry, issues)?;
        Self::validate_metadata_entry_reserved_field(entry, issues)?;
        self.validate_metadata_unknown_guid_policy(entry, issues)
    }

    fn validate_metadata_offset_and_length(
        entry: &crate::metadata::TableEntry<'_>, offset: usize, length: usize, region_len: usize,
        ranges: &mut Vec<(u32, u32, Guid)>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if length == 0 && offset != 0 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_ENTRY_INVALID",
                    format!("length=0 but offset={offset} (expected 0)"),
                    "MS-VHDX/2.6.1.2",
                ),
            );
            return Err(Error::InvalidMetadata(format!(
                "METADATA_ENTRY_INVALID: length=0 but offset={offset} (expected 0)"
            )));
        }
        if length > 0 {
            if offset < 65536 {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "metadata",
                        "METADATA_ENTRY_OFFSET_MINIMUM",
                        format!("metadata entry offset {offset} < 64KB minimum"),
                        "MS-VHDX/2.6.1.2",
                    ),
                );
                return Err(Error::InvalidMetadata(format!(
                    "METADATA_ENTRY_OFFSET_MINIMUM: metadata entry offset {offset} < 64KB minimum"
                )));
            }
            let Some(end) = offset.checked_add(length) else {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "metadata",
                        "METADATA_ENTRY_INVALID",
                        "offset+length overflow",
                        "MS-VHDX/2.6.1.2",
                    ),
                );
                return Err(Error::InvalidMetadata(
                    "METADATA_ENTRY_INVALID: offset+length overflow".into(),
                ));
            };
            if end > region_len {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "metadata",
                        "METADATA_ENTRY_INVALID",
                        format!("item extent [{offset}..{end}] exceeds region ({region_len})"),
                        "MS-VHDX/2.6.1.2",
                    ),
                );
                return Err(Error::InvalidMetadata(format!(
                    "METADATA_ENTRY_INVALID: item extent [{offset}..{end}] exceeds region ({region_len})"
                )));
            }
            ranges.push((
                u32::try_from(offset).expect("metadata item offset fits u32"),
                u32::try_from(offset + length).expect("metadata item end fits u32"),
                entry.item_id(),
            ));
        }
        Ok(())
    }

    fn validate_metadata_entry_reserved_flags(
        entry: &crate::metadata::TableEntry<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if entry.flags().has_reserved_bits() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_RESERVED_FLAGS_SET",
                    format!(
                        "metadata entry GUID {} has reserved flags bits set: {:#010x}",
                        entry.item_id(),
                        entry.flags_bits()
                    ),
                    "MS-VHDX/2.6.1.2",
                ),
            );
            return Err(Error::MetadataReservedFlagsSet {
                flags: entry.flags_bits(),
            });
        }
        Ok(())
    }

    fn validate_metadata_entry_reserved_field(
        entry: &crate::metadata::TableEntry<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if entry.reserved() != 0 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_ENTRY_RESERVED_NONZERO",
                    format!(
                        "metadata entry GUID {} has reserved field set to {:#010x}",
                        entry.item_id(),
                        entry.reserved()
                    ),
                    "MS-VHDX/2.6.1.2",
                ),
            );
            return Err(Error::MetadataEntryReservedNonzero {
                reserved: entry.reserved(),
            });
        }
        Ok(())
    }

    fn validate_metadata_unknown_guid_policy(
        &self, entry: &crate::metadata::TableEntry<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if is_known_metadata_guid(&entry.item_id()) {
            return Ok(());
        }
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "metadata",
                "METADATA_GUID_UNKNOWN",
                format!("unknown metadata GUID {}", entry.item_id()),
                "MS-VHDX/2.6.2",
            ),
        );
        if entry.flags().is_required() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_REQUIRED_UNKNOWN",
                    format!("required unknown metadata GUID {}", entry.item_id()),
                    "RELAX",
                ),
            );
            return Err(Error::MetadataRequiredUnknown {
                guid: entry.item_id(),
            });
        }
        if self.strict {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata",
                    "METADATA_OPTIONAL_UNKNOWN",
                    format!(
                        "optional unknown metadata GUID {} in strict mode",
                        entry.item_id()
                    ),
                    "RELAX",
                ),
            );
            return Err(Error::MetadataOptionalUnknown {
                guid: entry.item_id(),
            });
        }
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "metadata",
                "METADATA_OPTIONAL_UNKNOWN",
                format!(
                    "optional unknown metadata GUID {} tolerated in non-strict mode",
                    entry.item_id()
                ),
                "RELAX",
            ),
        );
        Ok(())
    }

    fn validate_metadata_ranges_overlap(
        ranges: &[(u32, u32, Guid)], issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                let (s1, e1, g1) = &ranges[i];
                let (s2, e2, g2) = &ranges[j];
                if *s1 < *e2 && *s2 < *e1 {
                    Self::push_issue(
                        issues,
                        ValidationIssue::new(
                            "metadata",
                            "METADATA_ITEMS_OVERLAP",
                            format!("metadata items overlap: {g1} and {g2}"),
                            "MS-VHDX/2.6.2",
                        ),
                    );
                    return Err(Error::InvalidMetadata(format!(
                        "METADATA_ITEMS_OVERLAP: metadata items overlap: {g1} and {g2}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn push_corrupted_known_metadata_items(
        table: &crate::metadata::MetadataTable<'_>, issues: &mut Vec<ValidationIssue>,
    ) {
        let known_items: &[(&Guid, &str, u32)] = &[
            (&StandardItems::FILE_PARAMETERS, "FileParameters", 8),
            (&StandardItems::VIRTUAL_DISK_SIZE, "VirtualDiskSize", 8),
            (&StandardItems::VIRTUAL_DISK_ID, "VirtualDiskId", 16),
            (&StandardItems::LOGICAL_SECTOR_SIZE, "LogicalSectorSize", 4),
            (
                &StandardItems::PHYSICAL_SECTOR_SIZE,
                "PhysicalSectorSize",
                4,
            ),
        ];
        for &(guid, name, min_len) in known_items {
            if let Ok(entry) = table.entry(guid)
                && entry.length() > 0
                && entry.length() < min_len
            {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "metadata",
                        "METADATA_ITEM_CORRUPTED",
                        format!(
                            "{name}: data length {} < expected minimum {} bytes",
                            entry.length(),
                            min_len
                        ),
                        "MS-VHDX/2.6.2",
                    ),
                );
            }
        }
    }

    /// Validate that all required metadata items are present.
    ///
    /// Required items (MS-VHDX §2.6.2):
    /// - `FileParameters`
    /// - `VirtualDiskSize`
    /// - `VirtualDiskId`
    /// - `LogicalSectorSize`
    /// - `PhysicalSectorSize`
    /// - `ParentLocator` (if differencing disk)
    ///
    /// # Errors
    ///
    /// Returns an error when required metadata entries or required payloads are
    /// missing.
    pub fn validate_required_metadata_items(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(meta_data) = self.metadata_region() else {
            return Ok(issues);
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let items = meta.items();

        Self::validate_required_metadata_core(&meta, &items, &mut issues)?;
        self.validate_required_parent_locator_item(&meta, &items, &mut issues)?;

        Ok(issues)
    }

    fn validate_required_metadata_core(
        meta: &crate::metadata::Metadata<'_>, items: &crate::metadata::MetadataItems<'_>,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let required_items: &[(&Guid, &str)] = &[
            (&StandardItems::FILE_PARAMETERS, "FileParameters"),
            (&StandardItems::VIRTUAL_DISK_SIZE, "VirtualDiskSize"),
            (&StandardItems::VIRTUAL_DISK_ID, "VirtualDiskId"),
            (&StandardItems::LOGICAL_SECTOR_SIZE, "LogicalSectorSize"),
            (&StandardItems::PHYSICAL_SECTOR_SIZE, "PhysicalSectorSize"),
        ];
        for (guid, name) in required_items {
            Self::ensure_required_metadata_entry_present(meta, guid, name, issues)?;
            Self::ensure_required_metadata_item_data_present(items, guid, name, issues)?;
        }
        Ok(())
    }

    fn ensure_required_metadata_entry_present(
        meta: &crate::metadata::Metadata<'_>, guid: &Guid, name: &str,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if meta.table().entry(guid).is_err() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    format!("{name} entry not found in metadata table"),
                    "RELAX",
                ),
            );
            return Err(Error::MetadataRequiredMissing { guid: *guid });
        }
        Ok(())
    }

    fn ensure_required_metadata_item_data_present(
        items: &crate::metadata::MetadataItems<'_>, guid: &Guid, name: &str,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        match name {
            "FileParameters" => Self::ensure_file_parameters_data(items, issues),
            "VirtualDiskSize" if items.virtual_disk_size().is_err() => {
                Self::push_required_data_missing(issues, name, *guid)
            }
            "VirtualDiskId" if items.virtual_disk_id().is_err() => {
                Self::push_required_data_missing(issues, name, *guid)
            }
            "LogicalSectorSize" if items.logical_sector_size().is_err() => {
                Self::push_required_data_missing(issues, name, *guid)
            }
            "PhysicalSectorSize" if items.physical_sector_size().is_err() => {
                Self::push_required_data_missing(issues, name, *guid)
            }
            _ => Ok(()),
        }
    }

    fn ensure_file_parameters_data(
        items: &crate::metadata::MetadataItems<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let Ok(fp) = items.file_parameters() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    "FileParameters data not present",
                    "RELAX",
                ),
            );
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::FILE_PARAMETERS,
            });
        };
        if fp.has_reserved_bits_set() {
            let fp_flags = fp.flags();
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata_required",
                    "METADATA_FILE_PARAMETERS_RESERVED_FLAGS",
                    format!("FileParameters reserved flags (bits 2-31) are set: {fp_flags:#010x}"),
                    "MS-VHDX/2.6.2.1",
                ),
            );
            return Err(Error::FileParametersReservedFlags { flags: fp_flags });
        }
        Ok(())
    }

    fn push_required_data_missing(
        issues: &mut Vec<ValidationIssue>, name: &str, guid: Guid,
    ) -> Result<()> {
        Self::push_issue(
            issues,
            ValidationIssue::new(
                "metadata_required",
                "METADATA_REQUIRED_MISSING",
                format!("{name} data not present"),
                "RELAX",
            ),
        );
        Err(Error::MetadataRequiredMissing { guid })
    }

    fn validate_required_parent_locator_item(
        &self, meta: &crate::metadata::Metadata<'_>, items: &crate::metadata::MetadataItems<'_>,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if !self.has_parent() {
            return Ok(());
        }
        if meta.table().entry(&StandardItems::PARENT_LOCATOR).is_err() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    "ParentLocator entry not found for differencing disk",
                    "RELAX",
                ),
            );
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::PARENT_LOCATOR,
            });
        }
        if items.parent_locator().is_err() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "metadata_required",
                    "METADATA_REQUIRED_MISSING",
                    "ParentLocator data not present for differencing disk",
                    "RELAX",
                ),
            );
            return Err(Error::MetadataRequiredMissing {
                guid: StandardItems::PARENT_LOCATOR,
            });
        }
        Ok(())
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
    /// - `LogGuid` matching header `LogGuid`
    /// - Active sequence non-empty
    ///
    /// # Errors
    ///
    /// Returns an error when log entry integrity or sequencing checks fail.
    ///
    /// # Panics
    ///
    /// Panics if raw log entry pre-scan indexing invariants are violated.
    pub fn validate_log(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(log_data) = self.log_region() else {
            return Ok(issues);
        };
        if Self::log_region_is_empty_or_zero(log_data) {
            return Ok(issues);
        }
        let log = crate::log::Log::new(log_data)?;
        let header_log_guid = Self::read_current_header_log_guid(&self.parse_header()?)?;
        Self::prescan_log_signatures(log_data, &mut issues);
        let entries: Vec<_> = log.entries().collect();
        if entries.is_empty() {
            // If the header indicates a log should exist (LogGuid != 0) but the
            // log region contains no parseable entries, the active sequence is empty.
            if header_log_guid != Guid::zero() {
                Self::push_issue(
                    &mut issues,
                    ValidationIssue::new(
                        "log",
                        "LOG_ACTIVE_SEQUENCE_EMPTY",
                        "header LogGuid is non-zero but no valid log entries found".to_string(),
                        "MS-VHDX/2.3.3",
                    ),
                );
                return Err(Error::LogActiveSequenceEmpty);
            }
            return Ok(issues);
        }
        Self::validate_log_entries(&entries, header_log_guid, &mut issues)?;
        Self::push_log_replay_required_issue(header_log_guid, &mut issues);

        Ok(issues)
    }

    fn log_region_is_empty_or_zero(log_data: &[u8]) -> bool {
        log_data.is_empty() || log_data.iter().all(|&b| b == 0)
    }

    fn read_current_header_log_guid(header: &Header<'a>) -> Result<Guid> {
        Ok(header.header(0)?.log_guid())
    }

    fn prescan_log_signatures(log_data: &[u8], issues: &mut Vec<ValidationIssue>) {
        let mut scan_offset: usize = 0;
        while scan_offset + 64 <= log_data.len() {
            let sig = &log_data[scan_offset..scan_offset + 4];
            if sig == b"loge" {
                let entry_length = u32::from_le_bytes(
                    log_data[scan_offset + 8..scan_offset + 12]
                        .try_into()
                        .expect("slice length checked by loop guard"),
                ) as usize;
                if entry_length > 0
                    && entry_length.is_multiple_of(4096)
                    && scan_offset + entry_length <= log_data.len()
                {
                    scan_offset += entry_length;
                } else {
                    scan_offset += 4096;
                }
            } else if sig == [0u8; 4] {
                break;
            } else if sig == b"data" {
                scan_offset += 4096;
            } else {
                let mut found = [0u8; 4];
                found.copy_from_slice(sig);
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "log",
                        "LOG_SIGNATURE_INVALID",
                        format!("expected \"loge\", found {found:?}"),
                        "MS-VHDX/2.3.1.1",
                    ),
                );
                scan_offset += 4096;
            }
        }
    }

    fn validate_log_entries(
        entries: &[crate::log::Entry<'_>], header_log_guid: Guid, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let mut prev_seq: Option<u64> = None;
        for entry in entries {
            let seq = Self::validate_log_entry(entry, header_log_guid, prev_seq, issues)?;
            prev_seq = Some(seq);
        }
        Ok(())
    }

    fn validate_log_entry(
        entry: &crate::log::Entry<'_>, header_log_guid: Guid, prev_seq: Option<u64>,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<u64> {
        Self::validate_log_entry_checksum(entry, issues)?;
        let hdr = entry.header();
        Self::validate_log_entry_length_and_tail(&hdr, issues)?;
        Self::validate_log_entry_guid(&hdr, header_log_guid, issues)?;
        let seq = Self::validate_log_sequence_continuity(&hdr, prev_seq, issues)?;
        let data_sectors = Self::validate_log_data_sector_count(entry, issues)?;
        Self::validate_log_data_sectors(&data_sectors, seq, issues)?;
        Self::validate_log_descriptors(entry, seq, issues)?;
        Ok(seq)
    }

    fn validate_log_entry_checksum(
        entry: &crate::log::Entry<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if entry.verify_checksum().is_err() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_CHECKSUM_MISMATCH",
                    "entry CRC-32C mismatch",
                    "MS-VHDX/2.3.1.1",
                ),
            );
            return Err(Error::LogEntryCorrupted(
                "LOG_ENTRY_CHECKSUM_MISMATCH: entry CRC-32C mismatch".into(),
            ));
        }
        Ok(())
    }

    fn validate_log_entry_length_and_tail(
        hdr: &crate::log::LogEntryHeader<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let entry_length = hdr.entry_length();
        if entry_length == 0 || !entry_length.is_multiple_of(4096) {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_LENGTH_INVALID",
                    format!("entry_length={entry_length}"),
                    "MS-VHDX/2.3.1.1",
                ),
            );
            return Err(Error::LogEntryCorrupted(format!(
                "LOG_ENTRY_LENGTH_INVALID: entry_length={entry_length}"
            )));
        }
        let tail = hdr.tail();
        if !tail.is_multiple_of(4096) {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_ENTRY_TAIL_INVALID",
                    format!("tail={tail}"),
                    "MS-VHDX/2.3.1.1",
                ),
            );
            return Err(Error::LogEntryCorrupted(format!(
                "LOG_ENTRY_TAIL_INVALID: tail={tail}"
            )));
        }
        Ok(())
    }

    fn validate_log_entry_guid(
        hdr: &crate::log::LogEntryHeader<'_>, header_log_guid: Guid,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let entry_log_guid = hdr.log_guid();
        let is_zero_guid = entry_log_guid.to_bytes() == [0u8; 16];
        if !is_zero_guid && entry_log_guid != header_log_guid {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_SEQUENCE_GUID_MISMATCH",
                    format!("entry LogGuid {entry_log_guid} != header LogGuid {header_log_guid}"),
                    "MS-VHDX/2.3.2",
                ),
            );
            return Err(Error::LogSequenceGuidMismatch {
                entry_log_guid,
                header_log_guid,
            });
        }
        Ok(())
    }

    fn validate_log_sequence_continuity(
        hdr: &crate::log::LogEntryHeader<'_>, prev_seq: Option<u64>,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<u64> {
        let seq = hdr.sequence_number();
        if let Some(prev) = prev_seq
            && seq != prev + 1
        {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_SEQUENCE_GAP",
                    format!("seq {seq} does not follow {prev}"),
                    "MS-VHDX/2.3.2",
                ),
            );
            return Err(Error::LogSequenceGap {
                expected: prev + 1,
                found: seq,
            });
        }
        Ok(seq)
    }

    fn validate_log_data_sector_count<'b>(
        entry: &'b crate::log::Entry<'b>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<Vec<crate::log::DataSector<'b>>> {
        let _desc_count = entry.header().descriptor_count();
        let actual_data_descs: usize = entry
            .descriptors()
            .filter_map(std::result::Result::ok)
            .filter(|d| matches!(d, crate::log::Descriptor::Data(_)))
            .count();
        let data_sectors: Vec<_> = entry.data().collect();
        if data_sectors.len() != actual_data_descs {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_DESCRIPTOR_COUNT_MISMATCH",
                    format!(
                        "data sectors ({}) != data descriptors ({})",
                        data_sectors.len(),
                        actual_data_descs
                    ),
                    "MS-VHDX/2.3.1",
                ),
            );
            return Err(Error::LogEntryCorrupted(format!(
                "LOG_DESCRIPTOR_COUNT_MISMATCH: data sectors ({}) != data descriptors ({})",
                data_sectors.len(),
                actual_data_descs
            )));
        }
        Ok(data_sectors)
    }

    fn validate_log_data_sectors(
        data_sectors: &[crate::log::DataSector<'_>], seq: u64, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        for sector in data_sectors {
            let sig = sector.signature();
            if sig != b"data" {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "log",
                        "LOG_DATA_SECTOR_INVALID",
                        "invalid data sector signature",
                        "MS-VHDX/2.3.1.4",
                    ),
                );
                return Err(Error::InvalidSignature {
                    position: SignaturePosition::DataSector,
                    expected: crate::error::pad_signature_4to8(*b"data"),
                    found: crate::error::pad_signature_4to8(*sig),
                });
            }
            let sector_seq = sector.sequence_number();
            if sector_seq != seq {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "log",
                        "LOG_DATA_SECTOR_INVALID",
                        format!("sector seq {sector_seq} != entry seq {seq}"),
                        "MS-VHDX/2.3.1.4",
                    ),
                );
                return Err(Error::LogEntryCorrupted(format!(
                    "LOG_DATA_SECTOR_INVALID: sector seq {sector_seq} != entry seq {seq}"
                )));
            }
        }
        Ok(())
    }

    fn validate_log_descriptors(
        entry: &crate::log::Entry<'_>, seq: u64, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        for desc_result in entry.descriptors() {
            let desc = match desc_result {
                Ok(d) => d,
                Err(e) => {
                    Self::push_issue(
                        issues,
                        ValidationIssue::new(
                            "log",
                            "LOG_DESCRIPTOR_SIGNATURE_INVALID",
                            format!("{e}"),
                            "MS-VHDX/2.3.1",
                        ),
                    );
                    return Err(Error::LogEntryCorrupted(format!(
                        "LOG_DESCRIPTOR_SIGNATURE_INVALID: {e}"
                    )));
                }
            };
            let desc_seq = desc.sequence_number();
            if desc_seq != seq {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "log",
                        "LOG_DESCRIPTOR_SEQUENCE_MISMATCH",
                        format!("descriptor seq {desc_seq} != entry seq {seq}"),
                        "MS-VHDX/2.3.1",
                    ),
                );
                return Err(Error::LogEntryCorrupted(format!(
                    "LOG_DESCRIPTOR_SEQUENCE_MISMATCH: descriptor seq {desc_seq} != entry seq {seq}"
                )));
            }
        }
        Ok(())
    }

    fn push_log_replay_required_issue(header_log_guid: Guid, issues: &mut Vec<ValidationIssue>) {
        if header_log_guid != Guid::zero() {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "log",
                    "LOG_REPLAY_REQUIRED",
                    "replayable log entries exist (use --log-replay to replay)",
                    "ROEXT",
                ),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Parent locator validation
    // -----------------------------------------------------------------------

    /// Validate the parent locator for differencing disks.
    ///
    /// Checks:
    /// - `parent_linkage` key exists
    /// - `parent_linkage2` key absent (conflict)
    /// - At least one path entry (`relative_path`, `volume_path`, `absolute_win32_path`)
    ///
    /// # Errors
    ///
    /// Returns an error when required locator keys are missing, conflicting, or
    /// invalid.
    pub fn validate_parent_locator(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(meta_data) = self.metadata_region() else {
            return Ok(issues);
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let Ok(locator) = meta.items().parent_locator() else {
            return Ok(issues);
        };
        let parent_linkage_guid = Self::validate_parent_locator_keys(&locator, &mut issues)?;
        Self::validate_parent_locator_data_write_guid(parent_linkage_guid, &locator, &mut issues)?;

        Ok(issues)
    }

    fn validate_parent_locator_keys(
        locator: &crate::metadata::ParentLocator<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<Option<Guid>> {
        let kv_data = locator.key_value_data();
        let mut has_parent_linkage = false;
        let mut has_path = false;
        let mut parent_linkage_guid: Option<Guid> = None;
        for kv in locator.entries() {
            let key = kv.key(kv_data)?;
            match key.as_str() {
                "parent_linkage" => {
                    has_parent_linkage = true;
                    if let Ok(value) = kv.value(kv_data) {
                        parent_linkage_guid = parse_guid_from_braced_string(&value);
                    }
                }
                "parent_linkage2" => {
                    Self::push_issue(
                        issues,
                        ValidationIssue::new(
                            "parent_locator",
                            "PARENT_LOCATOR_LINKAGE2_CONFLICT",
                            "parent_linkage2 present",
                            "MS-VHDX/2.6.2.6.3",
                        ),
                    );
                    return Err(Error::ParentLocatorLinkage2Conflict);
                }
                "relative_path" | "volume_path" | "absolute_win32_path" => {
                    has_path = true;
                }
                _ => {}
            }
        }
        if !has_parent_linkage {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_MISSING_LINKAGE",
                    "parent_linkage key not found",
                    "MS-VHDX/2.6.2.6.3",
                ),
            );
            return Err(Error::ParentLocatorMissingLinkage);
        }
        if !has_path {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_NO_VALID_PATH",
                    "no valid parent path (relative_path/volume_path/absolute_win32_path)",
                    "MS-VHDX/2.6.2.6.3",
                ),
            );
            return Err(Error::ParentNotFound);
        }
        Ok(parent_linkage_guid)
    }

    fn validate_parent_locator_data_write_guid(
        parent_linkage_guid: Option<Guid>, locator: &crate::metadata::ParentLocator<'_>,
        issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        let Some(expected_linkage) = parent_linkage_guid else {
            return Ok(());
        };
        let Ok(parent_path_buf) = locator.resolve_parent_path() else {
            return Ok(());
        };
        let Some(parent_data_write_guid) = Self::read_parent_data_write_guid(&parent_path_buf)
        else {
            return Ok(());
        };
        if parent_data_write_guid != expected_linkage {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_GUID_MISMATCH",
                    format!(
                        "DataWriteGuid mismatch: expected {expected_linkage}, actual {parent_data_write_guid}"
                    ),
                    "MS-VHDX/2.6.2.6",
                ),
            );
            return Err(Error::ParentMismatch {
                expected: expected_linkage,
                actual: parent_data_write_guid,
            });
        }
        Ok(())
    }

    fn read_parent_data_write_guid(parent_path_buf: &std::path::Path) -> Option<Guid> {
        use std::io::Read;
        let mut parent_file = std::fs::File::open(parent_path_buf).ok()?;
        let mut parent_header_buf = vec![0u8; 1024 * 1024];
        let bytes_read = parent_file.read(&mut parent_header_buf).ok()?;
        if bytes_read < 8 {
            return None;
        }
        parent_header_buf.truncate(bytes_read);
        let expected_sig: [u8; 8] = [0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6C, 0x65];
        if parent_header_buf[..8] != expected_sig {
            return None;
        }
        let parent_header = crate::header::Header::new(&parent_header_buf).ok()?;
        let parent_current = parent_header.header(0).ok()?;
        Some(parent_current.data_write_guid())
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
    #[cfg(test)]
    pub(crate) fn validate_parent_chain(&self) -> Result<crate::file::ParentChainInfo> {
        let mut issues = Vec::new();
        let locator = self.load_parent_chain_locator(&mut issues)?;
        let expected_linkage = Self::extract_expected_parent_linkage(&locator, &mut issues)?;
        let parent_path_buf = Self::resolve_parent_chain_path(&locator, &mut issues)?;
        let parent_data_write_guid =
            Self::read_parent_chain_data_write_guid(&parent_path_buf, &mut issues)?;
        Self::validate_parent_chain_linkage(expected_linkage, parent_data_write_guid, &mut issues)?;

        let child = self
            .child_path
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("<unknown>"));
        Ok(crate::file::ParentChainInfo {
            _child_path: child,
            _parent_path: parent_path_buf,
            _linkage_matched: true,
        })
    }

    #[cfg(test)]
    fn load_parent_chain_locator<'b>(
        &'b self, issues: &mut Vec<ValidationIssue>,
    ) -> Result<crate::metadata::ParentLocator<'b>> {
        let Some(meta_data) = self.metadata_region() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    "no metadata region",
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        let meta = crate::metadata::Metadata::new(meta_data)?;
        let Ok(locator) = meta.items().parent_locator() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_MISSING_LINKAGE",
                    "no parent locator",
                    "MS-VHDX/2.6.2.6.3",
                ),
            );
            return Err(Error::ParentLocatorMissingLinkage);
        };
        Ok(locator)
    }

    #[cfg(test)]
    fn extract_expected_parent_linkage(
        locator: &crate::metadata::ParentLocator<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<Guid> {
        let kv_data = locator.key_value_data();
        let mut expected_linkage: Option<Guid> = None;
        for kv in locator.entries() {
            let Ok(key) = kv.key(kv_data) else { continue };
            if key == "parent_linkage2" {
                Self::push_issue(
                    issues,
                    ValidationIssue::new(
                        "parent_locator",
                        "PARENT_LOCATOR_LINKAGE2_CONFLICT",
                        "parent_linkage2 present",
                        "MS-VHDX/2.6.2.6.3",
                    ),
                );
                return Err(Error::ParentLocatorLinkage2Conflict);
            }
            if key == "parent_linkage" {
                let Ok(value) = kv.value(kv_data) else {
                    continue;
                };
                expected_linkage = parse_guid_from_braced_string(&value);
            }
        }
        let Some(expected_linkage) = expected_linkage else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    "parent_linkage value is not a valid GUID format",
                    "VALEXT",
                ),
            );
            return Err(Error::InvalidParentLocator(
                "parent_linkage value is not a valid GUID format".into(),
            ));
        };
        Ok(expected_linkage)
    }

    #[cfg(test)]
    fn resolve_parent_chain_path(
        locator: &crate::metadata::ParentLocator<'_>, issues: &mut Vec<ValidationIssue>,
    ) -> Result<std::path::PathBuf> {
        let Ok(parent_path_buf) = locator.resolve_parent_path() else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_NO_VALID_PATH",
                    "unresolvable parent path",
                    "MS-VHDX/2.6.2.6.3",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        Ok(parent_path_buf)
    }

    #[cfg(test)]
    fn read_parent_chain_data_write_guid(
        parent_path_buf: &std::path::PathBuf, issues: &mut Vec<ValidationIssue>,
    ) -> Result<Guid> {
        use std::io::Read;
        let Ok(mut parent_file) = std::fs::File::open(parent_path_buf) else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_NO_VALID_PATH",
                    format!("unable to open parent file: {}", parent_path_buf.display()),
                    "MS-VHDX/2.6.2.6.3",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        let mut parent_header_buf = vec![0u8; 1024 * 1024];
        let Ok(bytes_read) = parent_file.read(&mut parent_header_buf) else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!("failed to read parent file: {}", parent_path_buf.display()),
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        if bytes_read < 8 {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!(
                        "parent file too small ({} bytes): {}",
                        bytes_read,
                        parent_path_buf.display()
                    ),
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        }
        parent_header_buf.truncate(bytes_read);
        let expected_sig: [u8; 8] = [0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6C, 0x65];
        if parent_header_buf[..8] != expected_sig {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!(
                        "parent file is not a valid VHDX: {}",
                        parent_path_buf.display()
                    ),
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        }
        let Ok(parent_header) = Header::new(&parent_header_buf) else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!(
                        "failed to parse parent header: {}",
                        parent_path_buf.display()
                    ),
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        let Ok(parent_current) = parent_header.header(0) else {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_FORMAT_ERROR",
                    format!(
                        "failed to get current parent header: {}",
                        parent_path_buf.display()
                    ),
                    "VALEXT",
                ),
            );
            return Err(Error::ParentNotFound);
        };
        Ok(parent_current.data_write_guid())
    }

    #[cfg(test)]
    fn validate_parent_chain_linkage(
        expected_linkage: Guid, parent_data_write_guid: Guid, issues: &mut Vec<ValidationIssue>,
    ) -> Result<()> {
        if parent_data_write_guid != expected_linkage {
            Self::push_issue(
                issues,
                ValidationIssue::new(
                    "parent_locator",
                    "PARENT_LOCATOR_GUID_MISMATCH",
                    format!(
                        "DataWriteGuid mismatch: expected {expected_linkage}, actual {parent_data_write_guid}"
                    ),
                    "MS-VHDX/2.6.2.6",
                ),
            );
            return Err(Error::ParentMismatch {
                expected: expected_linkage,
                actual: parent_data_write_guid,
            });
        }
        Ok(())
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
        let log_offset = usize::try_from(current.log_offset()).ok()?;
        let log_length = usize::try_from(current.log_length()).ok()?;

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
                let offset = usize::try_from(entry.file_offset()).ok()?;
                let length = usize::try_from(entry.length()).ok()?;
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
        self.region_for_guid(&BAT_REGION_GUID)
    }

    /// Resolve the metadata region data.
    fn metadata_region(&self) -> Option<&'a [u8]> {
        self.region_for_guid(&METADATA_REGION_GUID)
    }

    /// Determine whether this is a differencing disk (`has_parent` flag).
    fn has_parent(&self) -> bool {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(fp) = meta.items().file_parameters()
        {
            return fp.has_parent();
        }
        false
    }

    /// Extract the current header's `LogGuid`.
    fn current_log_guid(header: &Header<'a>) -> Result<Guid> {
        let current = header.header(0)?;
        Ok(current.log_guid())
    }

    /// Compute the chunk ratio for BAT interpretation.
    fn chunk_ratio(&self) -> u64 {
        let block_size = u64::from(self.block_size());
        let logical_sector_size = u64::from(self.logical_sector_size());
        if block_size == 0 || logical_sector_size == 0 {
            return 0;
        }
        crate::common::compute_chunk_ratio(block_size, logical_sector_size)
    }

    /// Get block size from metadata.
    fn block_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(fp) = meta.items().file_parameters()
        {
            return fp.block_size();
        }
        0
    }

    /// Get logical sector size from metadata.
    fn logical_sector_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(lss) = meta.items().logical_sector_size()
        {
            return lss;
        }
        0
    }

    /// Get virtual disk size from metadata.
    fn virtual_disk_size(&self) -> u64 {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(vds) = meta.items().virtual_disk_size()
        {
            return vds;
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{
        HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, METADATA_TABLE_SIZE, MIB, REGION_TABLE_SIZE,
        REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET,
    };
    use bitvec::prelude::*;
    use crc32c::crc32c;

    struct Encoded {
        key: Vec<u8>,
        val: Vec<u8>,
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Build a minimal valid VHDX file in memory for validation testing.
    fn build_test_vhdx() -> Vec<u8> {
        let virtual_size: u64 = 1024 * 1024 * 1024; // 1 GB
        let block_size: u32 = 32 * 1024 * 1024; // 32 MB
        let logical_sector_size: u32 = 4096;
        let bat_entry_count = virtual_size.div_ceil(u64::from(block_size));
        let chunk_ratio = (1u64 << 23) * u64::from(logical_sector_size) / u64::from(block_size);
        let sector_bitmap_count = bat_entry_count.div_ceil(chunk_ratio);
        let total_bat_entries = usize::try_from(bat_entry_count + sector_bitmap_count)
            .expect("BAT entry count fits usize");
        let bat_bytes = total_bat_entries * 8;
        let bat_size = std::cmp::max(
            u32::try_from(
                u64::try_from(bat_bytes)
                    .expect("bat bytes fit u64")
                    .div_ceil(u64::from(MIB)),
            )
            .expect("BAT size in MB units fits u32"),
            1,
        ) * MIB;

        let header_size = HEADER_SIZE;
        let region_table_size = REGION_TABLE_SIZE;

        let header1_offset = HEADER1_OFFSET;
        let header2_offset = HEADER2_OFFSET;
        let rt1_offset = REGION_TABLE1_OFFSET;
        let rt2_offset = REGION_TABLE2_OFFSET;
        let log_offset: u64 = u64::from(MIB);
        let log_length: u32 = MIB;
        let bat_offset: u64 = 2 * u64::from(MIB);
        let metadata_offset: u64 = bat_offset + u64::from(bat_size);
        let metadata_size: u32 = MIB;

        let file_end = metadata_offset + u64::from(metadata_size);
        let mut buf = vec![0u8; usize::try_from(file_end).expect("file size fits usize")];

        // File type identifier "vhdxfile"
        buf[0..8].copy_from_slice(b"vhdxfile");

        // Write headers
        let _ = (header_size, region_table_size, log_offset, log_length);

        write_header(&mut buf, header1_offset as usize, 5);
        write_header(&mut buf, header2_offset as usize, 3);

        // Write region tables
        write_region_table(
            &mut buf,
            rt1_offset as usize,
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
        );
        write_region_table(
            &mut buf,
            rt2_offset as usize,
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
        );

        // Write minimal BAT: payload entries = FullyPresent with block-aligned
        // offsets, sector bitmap entries = NotPresent.
        let bat_start = usize::try_from(bat_offset).expect("BAT offset fits usize");
        let block_size_mb = u64::from(block_size / MIB);
        let metadata_end_mb = (metadata_offset + u64::from(metadata_size)).div_ceil(u64::from(MIB));
        // Align first payload offset to block_size boundary.
        let first_payload_mb = metadata_end_mb.div_ceil(block_size_mb) * block_size_mb;
        let mut sb_written: u64 = 0;
        let mut payload_idx: u64 = 0;
        for i in 0..total_bat_entries {
            let entry_offset = bat_start + i * 8;
            let payloads_before = u64::try_from(i).expect("index fits u64") - sb_written;
            let is_sb = payloads_before > 0
                && payloads_before.is_multiple_of(chunk_ratio)
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
        write_metadata(
            &mut buf,
            usize::try_from(metadata_offset).expect("metadata offset fits usize"),
            block_size,
            logical_sector_size,
        );

        buf
    }

    fn write_header(buf: &mut [u8], offset: usize, seq: u64) {
        let header_size = HEADER_SIZE;
        let slice = &mut buf[offset..][..header_size as usize];
        slice[..4].copy_from_slice(b"head");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice[8..16].copy_from_slice(&seq.to_le_bytes());
        slice[64..66].copy_from_slice(&0u16.to_le_bytes()); // log_version
        slice[66..68].copy_from_slice(&1u16.to_le_bytes()); // version
        slice[68..72].copy_from_slice(&MIB.to_le_bytes()); // log_length
        slice[72..80].copy_from_slice(&u64::from(MIB).to_le_bytes()); // log_offset

        let checksum = crc32c(slice);
        slice[4..8].copy_from_slice(&checksum.to_le_bytes());
    }

    fn write_region_table(
        buf: &mut [u8], offset: usize, bat_offset: u64, bat_size: u32, metadata_offset: u64,
        metadata_size: u32,
    ) {
        let region_table_size = REGION_TABLE_SIZE;
        let slice = &mut buf[offset..][..region_table_size as usize];

        slice[..4].copy_from_slice(b"regi");
        slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
        slice[8..12].copy_from_slice(&2u32.to_le_bytes()); // 2 entries
        slice[12..16].copy_from_slice(&0u32.to_le_bytes()); // reserved

        // BAT region GUID
        let bat_guid: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD,
            0x4A, 0x08,
        ];
        // Metadata region GUID
        let meta_guid: [u8; 16] = [
            0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F,
            0x88, 0x6E,
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
        let metadata_table_size = METADATA_TABLE_SIZE;

        // Table header
        buf[offset..offset + 8].copy_from_slice(b"metadata");
        buf[offset + 10..offset + 12].copy_from_slice(&6u16.to_le_bytes()); // 6 entries

        // Write 6 table entries. Item offsets are relative to the start of the
        // metadata region (which includes the 64KB table).
        let mut entry_off = offset + 32;
        let item_base = metadata_table_size; // items start right after the 64KB table

        // Entry 0: FileParameters (relative offset = 64KB+0, length=8)
        write_metadata_entry(
            buf,
            &mut entry_off,
            &StandardItems::FILE_PARAMETERS,
            item_base,
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
        let items_base = offset + metadata_table_size as usize;
        let fp_flags: u32 = 0; // dynamic disk
        buf[items_base..items_base + 4].copy_from_slice(&block_size.to_le_bytes());
        buf[items_base + 4..items_base + 8].copy_from_slice(&fp_flags.to_le_bytes());

        // VirtualDiskSize: 1 GB
        let disk_size: u64 = 1024 * 1024 * 1024;
        buf[items_base + 8..items_base + 16].copy_from_slice(&disk_size.to_le_bytes());

        // VirtualDiskId: zeros (already zeroed)
        // LogicalSectorSize
        buf[items_base + 32..items_base + 36].copy_from_slice(&logical_sector_size.to_le_bytes());
        // PhysicalSectorSize
        buf[items_base + 40..items_base + 44].copy_from_slice(&4096u32.to_le_bytes());
    }

    fn write_metadata_entry(
        buf: &mut [u8], entry_off: &mut usize, guid: &Guid, item_offset: u32, length: u32,
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
        buf[HEADER1_OFFSET as usize] = 0xFF;
        buf[HEADER2_OFFSET as usize] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        // Both headers invalid -> must fail
        assert!(validator.validate_header().is_err());
    }

    #[test]
    fn validate_header_bad_version() {
        let mut buf = build_test_vhdx();
        // Set version to 2 on both headers
        buf[HEADER1_OFFSET as usize + 66..HEADER1_OFFSET as usize + 68]
            .copy_from_slice(&2u16.to_le_bytes());
        buf[HEADER2_OFFSET as usize + 66..HEADER2_OFFSET as usize + 68]
            .copy_from_slice(&2u16.to_le_bytes());
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
        buf[REGION_TABLE1_OFFSET as usize] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_region_table().is_err());
    }

    #[test]
    fn validate_region_table_bad_entry_count() {
        let mut buf = build_test_vhdx();
        // Set entry count to 3000 (> 2047) and fix CRC
        buf[REGION_TABLE1_OFFSET as usize + 8..REGION_TABLE1_OFFSET as usize + 12]
            .copy_from_slice(&3000u32.to_le_bytes());
        let checksum = crc32c(&buf[REGION_TABLE1_OFFSET as usize..][..REGION_TABLE_SIZE as usize]);
        buf[REGION_TABLE1_OFFSET as usize + 4..REGION_TABLE1_OFFSET as usize + 8]
            .copy_from_slice(&checksum.to_le_bytes());
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
        let metadata_offset = u64::from_le_bytes(
            buf[REGION_TABLE1_OFFSET as usize + 64..REGION_TABLE1_OFFSET as usize + 72]
                .try_into()
                .unwrap(),
        );
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
        // Zero out the FileParameters entry GUID (first entry after header, at offset 32)
        buf[mo + 32..mo + 48].copy_from_slice(&[0u8; 16]);
        let validator = SpecValidator::new(&buf, true);
        assert!(validator.validate_required_metadata_items().is_err());
    }

    #[test]
    fn test_metadata_item_corrupted_file_parameters() {
        let mut buf = build_test_vhdx();
        // Find metadata offset from region table entry 1 (Metadata entry)
        let metadata_offset = u64::from_le_bytes(
            buf[REGION_TABLE1_OFFSET as usize + 64..REGION_TABLE1_OFFSET as usize + 72]
                .try_into()
                .unwrap(),
        );
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
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
            "expected no METADATA_ITEM_CORRUPTED for valid sizes, got: {corrupted:?}"
        );
    }

    #[test]
    fn test_metadata_item_corrupted_preserves_missing() {
        let mut buf = build_test_vhdx();
        let metadata_offset = u64::from_le_bytes(
            buf[REGION_TABLE1_OFFSET as usize + 64..REGION_TABLE1_OFFSET as usize + 72]
                .try_into()
                .unwrap(),
        );
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
        // Zero out FileParameters entry GUID in metadata table
        buf[mo + 32..mo + 48].copy_from_slice(&[0u8; 16]);
        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_required_metadata_items();
        assert!(result.is_err(), "expected error for missing FileParameters");
        assert!(
            format!("{result:?}").contains("MetadataRequiredMissing"),
            "expected MetadataRequiredMissing, got: {result:?}"
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
    // Strict mode tests (MS-VHDX-`宽松扩展标准` §3)
    // -----------------------------------------------------------------------

    /// strict=false, optional unknown region entry → Ok
    #[test]
    fn test_strict_false_optional_unknown_region_passes() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID (required=0)
        // Region table 1 is at 192KB. Current: 2 entries (header 16 bytes + 2*32 = 80 bytes used).
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        // Update entry count: 2 → 3
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        // Write entry 2: unknown GUID, required=0, valid offset/length
        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        // file_offset: 4MB (aligned)
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        // length: 1MB (aligned)
        let length: u32 = 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        // required: 0 (optional)
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        // Fix CRC for RT1 (zero out checksum field first, then compute)
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // Do the same for RT2 at 256KB
        let rt2_offset = REGION_TABLE2_OFFSET as usize;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let rt2_entry_start = rt2_offset + 16 + 2 * 32;
        buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
        buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        // Extend buffer to cover the new region offset
        let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
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
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        // required: 1 (required)
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&1u32.to_le_bytes());

        let checksum = crc32c(&{
            let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
            slice.copy_from_slice(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // RT2
        let rt2_offset = REGION_TABLE2_OFFSET as usize;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let rt2_entry_start = rt2_offset + 16 + 2 * 32;
        buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
        buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&1u32.to_le_bytes());
        let checksum2 = crc32c(&{
            let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
            slice.copy_from_slice(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=false but required unknown → should still fail
        let validator = SpecValidator::new(&buf, false);
        let result = validator.validate_region_table();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(
            msg.contains("RegionRequiredUnknown"),
            "expected RegionRequiredUnknown, got: {msg}"
        );
    }

    /// strict=true, optional unknown region entry → Err
    #[test]
    fn test_strict_true_optional_unknown_region_fails() {
        let mut buf = build_test_vhdx();

        // Add a third region table entry with an unknown GUID and required=0
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        let checksum = crc32c(&{
            let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
            slice.copy_from_slice(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // RT2
        let rt2_offset = REGION_TABLE2_OFFSET as usize;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let rt2_entry_start = rt2_offset + 16 + 2 * 32;
        buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
        buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&{
            let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
            slice.copy_from_slice(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
            slice[4..8].copy_from_slice(&0u32.to_le_bytes());
            slice
        });
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=true → optional unknown should still fail
        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_region_table();
        assert!(result.is_err());
        let msg = format!("{result:?}");
        assert!(
            msg.contains("RegionOptionalUnknown"),
            "expected RegionOptionalUnknown, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Parent locator / chain validation tests
    // -----------------------------------------------------------------------

    /// Build a VHDX buffer whose parent locator contains the given KV pairs.
    ///
    /// The base VHDX is modified to be a differencing disk (`has_parent=1`) and
    /// the existing empty parent locator metadata entry is replaced with one
    /// pointing to actual locator data at `items_base` + 48.
    fn build_vhdx_with_parent_locator(kvs: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = build_test_vhdx();

        // Locate the metadata region from region table 1 (at 192 KB).
        // RT: 16-byte header + 2×32-byte entries. Entry 1 (metadata) starts at offset 48.
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        let metadata_offset =
            u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
        let items_base = mo + METADATA_TABLE_SIZE as usize; // items start after the 64 KB metadata table

        // Mark the disk as differencing: set FileParameters has_parent bit (bit 1).
        buf[items_base..items_base + 8]
            .view_bits_mut::<Lsb0>()
            .set(1, true);

        // Existing items occupy bytes 0..44 of the items area.
        // Place parent locator data right after, at offset 48 (4-byte aligned).
        let pl_start = items_base + 48;
        let pl_region_off = u32::try_from(pl_start - mo).expect("parent locator offset fits u32"); // offset within metadata region

        // -- Build parent locator data ---------------------------------------
        let loc_hdr_size = 20usize;
        let kv_entry_size = 12usize;
        let num = kvs.len();
        let kv_tab_size = num * kv_entry_size;
        let kv_dat_base = loc_hdr_size + kv_tab_size;

        // Encode keys & values to UTF-16LE.
        let encoded: Vec<Encoded> = kvs
            .iter()
            .map(|(k, v)| Encoded {
                key: k.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                val: v.encode_utf16().flat_map(u16::to_le_bytes).collect(),
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
        pl[18..20].copy_from_slice(
            &u16::try_from(num)
                .expect("entry count fits u16")
                .to_le_bytes(),
        ); // entry count

        // Write KV entries and data.
        let mut kv_off = kv_dat_base;
        for (i, e) in encoded.iter().enumerate() {
            let entry_off = loc_hdr_size + i * kv_entry_size;
            let key_off = u32::try_from(kv_off).expect("key offset fits u32");
            let val_off = u32::try_from(kv_off + e.key.len()).expect("value offset fits u32");
            pl[entry_off..entry_off + 4].copy_from_slice(&key_off.to_le_bytes());
            pl[entry_off + 4..entry_off + 8].copy_from_slice(&val_off.to_le_bytes());
            pl[entry_off + 8..entry_off + 10].copy_from_slice(
                &u16::try_from(e.key.len())
                    .expect("key length fits u16")
                    .to_le_bytes(),
            );
            pl[entry_off + 10..entry_off + 12].copy_from_slice(
                &u16::try_from(e.val.len())
                    .expect("value length fits u16")
                    .to_le_bytes(),
            );

            pl[kv_off..kv_off + e.key.len()].copy_from_slice(&e.key);
            kv_off += e.key.len();
            pl[kv_off..kv_off + e.val.len()].copy_from_slice(&e.val);
            kv_off += e.val.len();
        }

        // Update the ParentLocator metadata table entry (entry index 5).
        let pl_entry = mo + 32 + 5 * 32;
        buf[pl_entry + 16..pl_entry + 20].copy_from_slice(&pl_region_off.to_le_bytes());
        buf[pl_entry + 20..pl_entry + 24].copy_from_slice(
            &u32::try_from(pl_size)
                .expect("parent locator size fits u32")
                .to_le_bytes(),
        );

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
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        let metadata_offset =
            u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
        let kv0_off = mo + METADATA_TABLE_SIZE as usize + 48 + 20;
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
        let validator = SpecValidator::from_file(&file);

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
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

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
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        // Fix CRC for RT1
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        // Fix CRC for RT2
        let rt2_offset = REGION_TABLE2_OFFSET as usize;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let rt2_entry_start = rt2_offset + 16 + 2 * 32;
        buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
        buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        // Extend buffer to cover the new region
        let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
        if buf.len() < needed {
            buf.resize(needed, 0);
        }

        // strict=false → optional unknown passes but should push an issue
        let validator = SpecValidator::new(&buf, false);
        let issues = validator.validate_region_table().unwrap();
        assert!(
            !issues.is_empty(),
            "expected at least one issue for optional unknown region"
        );
        let found = issues.iter().any(|i| i.code() == "REGION_OPTIONAL_UNKNOWN");
        assert!(
            found,
            "expected REGION_OPTIONAL_UNKNOWN issue, got: {:?}",
            issues
                .iter()
                .map(super::ValidationIssue::code)
                .collect::<Vec<_>>()
        );

        // Verify issue fields
        let issue = issues
            .iter()
            .find(|i| i.code() == "REGION_OPTIONAL_UNKNOWN")
            .unwrap();
        assert_eq!(issue.section(), "region_table");
        assert_eq!(issue.spec_ref(), "RELAX");
        assert!(issue.message().contains("tolerated"));
    }

    #[test]
    fn test_optional_unknown_metadata_pushes_issue() {
        let mut buf = build_test_vhdx();

        // Get metadata offset from region table
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        let metadata_offset =
            u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");

        // The metadata table has 6 entries. Change entry count to 7 and add an unknown optional one.
        buf[mo + 10..mo + 12].copy_from_slice(&7u16.to_le_bytes());

        // Write a 7th entry at the next slot (entries start at offset 32, each 32 bytes)
        let entry_off = mo + 32 + 6 * 32;
        let unknown_guid: [u8; 16] = [
            0xDE, 0xAD, 0xBE, 0xEF, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
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
        let found = issues
            .iter()
            .any(|i| i.code() == "METADATA_OPTIONAL_UNKNOWN");
        assert!(
            found,
            "expected METADATA_OPTIONAL_UNKNOWN issue, got: {:?}",
            issues
                .iter()
                .map(super::ValidationIssue::code)
                .collect::<Vec<_>>()
        );

        let issue = issues
            .iter()
            .find(|i| i.code() == "METADATA_OPTIONAL_UNKNOWN")
            .unwrap();
        assert_eq!(issue.section(), "metadata");
        assert_eq!(issue.spec_ref(), "RELAX");
    }

    #[test]
    fn test_strict_true_no_issue_for_optional_unknown_region() {
        // strict=true: optional unknown should Err, not push issue
        let mut buf = build_test_vhdx();

        let rt_offset = REGION_TABLE1_OFFSET as usize;
        buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

        let entry_start = rt_offset + 16 + 2 * 32;
        let unknown_guid: [u8; 16] = [
            0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0xAA, 0xBB,
        ];
        buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
        let offset: u64 = 4 * 1024 * 1024;
        buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        let length: u32 = 1024 * 1024;
        buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum = crc32c(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        let rt2_offset = REGION_TABLE2_OFFSET as usize;
        buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
        let rt2_entry_start = rt2_offset + 16 + 2 * 32;
        buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
        buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
        buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
        buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&0u32.to_le_bytes());
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
        let checksum2 = crc32c(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
        buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

        let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
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
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        let metadata_offset =
            u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");

        // Entry 0 (FileParameters) reserved field is at entry start (offset+32) + 28
        let reserved_off = mo + 32 + 28;
        buf[reserved_off..reserved_off + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_metadata();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            Error::MetadataEntryReservedNonzero { reserved } => {
                assert_eq!(*reserved, 0xDEAD_BEEF);
            }
            other => panic!("expected MetadataEntryReservedNonzero error, got: {other:?}"),
        }
    }

    #[test]
    fn test_metadata_file_parameters_reserved_flags() {
        let mut buf = build_test_vhdx();

        // Get metadata offset from region table
        let rt_offset = REGION_TABLE1_OFFSET as usize;
        let metadata_offset =
            u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
        let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");

        // FileParameters data starts at mo + 64KB (after the 64KB metadata table)
        let fp_data_off = mo + METADATA_TABLE_SIZE as usize;
        // Set bit 2 of BitFields (bit 34 in the 8-byte Lsb0 view), which falls
        // in reserved bits 2-31. Per MS-VHDX §2.6.2.1 these bits MUST be 0;
        // BitFields is the second u32 (bytes 4-7, bits 32-63).
        buf[fp_data_off..fp_data_off + 8]
            .view_bits_mut::<Lsb0>()
            .set(34, true);

        let validator = SpecValidator::new(&buf, true);
        let result = validator.validate_required_metadata_items();
        // Reserved flags in FileParameters is now a blocking error per API.md
        assert!(
            result.is_err(),
            "expected Err for reserved flags, got: {result:?}"
        );
        let err = result.unwrap_err();
        match &err {
            Error::FileParametersReservedFlags { flags } => {
                assert_ne!(*flags, 0, "flags should be non-zero");
            }
            other => panic!("expected FileParametersReservedFlags error, got: {other:?}"),
        }
    }

    #[test]
    fn validate_header_accepts_single_valid_header() {
        // Build VHDX with header1 valid, header2 corrupted
        let mut buf = build_test_vhdx();
        // Corrupt header2 signature at offset 128 KB
        buf[HEADER2_OFFSET as usize] = 0xFF;
        let validator = SpecValidator::new(&buf, true);
        // Should succeed — single valid header is OK per MS-VHDX §2.2.2
        assert!(
            validator.validate_header().is_ok(),
            "validate_header should accept single valid header"
        );
    }
}
