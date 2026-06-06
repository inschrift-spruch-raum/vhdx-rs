use super::{
    Error, MIB, Result, SignaturePosition, SpecValidator, ValidationIssue, is_known_region_guid,
};

impl<'a> SpecValidator<'a> {
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
}
