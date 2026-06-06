use super::{
    Error, Guid, Header, HeaderStructure, MIB, Result, SignaturePosition, SpecValidator,
    ValidationIssue,
};

impl SpecValidator<'_> {
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
        header: &Header<'_>, issues: &mut Vec<ValidationIssue>,
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

    fn validate_header_pair(header: &Header<'_>, issues: &mut Vec<ValidationIssue>) -> Result<()> {
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
        header: &Header<'_>, issues: &mut Vec<ValidationIssue>, v1: &HeaderStructure<'_>,
        v2: &HeaderStructure<'_>,
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
        header: &Header<'_>, issues: &mut Vec<ValidationIssue>,
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
    fn validate_single_header(result: Result<HeaderStructure<'_>>) -> Result<HeaderStructure<'_>> {
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
}
