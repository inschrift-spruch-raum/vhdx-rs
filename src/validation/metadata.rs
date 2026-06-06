use super::{
    Error, Guid, Result, SpecValidator, StandardItems, ValidationIssue, is_known_metadata_guid,
};

impl SpecValidator<'_> {
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
}
