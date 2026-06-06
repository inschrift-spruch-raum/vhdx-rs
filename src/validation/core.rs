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

use crate::error::{Error, Result, SignaturePosition};

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
    pub(super) data: &'a [u8],
    /// Whether strict validation mode is enabled.
    pub(super) strict: bool,
    /// Optional path to the child file (used in parent chain validation).
    pub(super) child_path: Option<std::path::PathBuf>,
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
    pub(super) fn push_issue(issues: &mut Vec<ValidationIssue>, issue: ValidationIssue) {
        issues.push(issue);
    }

    /// Map a header validation [`Error`] to the correct [`ValidationIssue`] and push it.
    ///
    /// Distinguishes `InvalidSignature` (header position) from `InvalidChecksum` and
    /// version errors, producing the appropriate per-header issue code so that each
    /// invalid header gets a *specific* diagnostic rather than the generic
    /// `HEADER_SEQUENCE_NUMBER_INVALID` catch-all.
    pub(super) fn push_header_issue(
        issues: &mut Vec<ValidationIssue>, header_idx: u32, err: &Error,
    ) {
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

    /// Run all structural validations.
    ///
    /// Calls each sub-validation in order. Returns the first error encountered.
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
        if self.has_parent() {
            issues.extend(self.validate_parent_locator()?);
        }
        Ok(issues)
    }
}
