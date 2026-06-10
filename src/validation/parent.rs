use super::{
    BAT_REGION_GUID, Error, Guid, Header, METADATA_REGION_GUID, Result, SpecValidator,
    ValidationIssue,
};

impl SpecValidator {
    /// Validate the parent locator for differencing disks.
    ///
    /// # Errors
    ///
    /// Returns an error when parent locator keys, paths, or linkage are invalid.
    pub fn validate_parent_locator(&self) -> Result<Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let Some(meta_data) = self.metadata_region() else {
            return Ok(issues);
        };

        let meta = crate::metadata::Metadata::new(meta_data)?;
        let Ok(locator) = meta.items().parent_locator() else {
            return Ok(issues);
        };
        Self::validate_parent_locator_keys(&locator, &mut issues)?;

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
        if parent_linkage_guid.is_none() {
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

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Parse the header section from the data buffer.
    pub(super) fn parse_header(&self) -> Result<Header<'_>> {
        Header::new(&self.data)
    }

    /// Resolve the log region slice from the data buffer.
    pub(super) fn log_region(&self) -> Option<&[u8]> {
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
    pub(super) fn region_for_guid(&self, guid: &Guid) -> Option<&[u8]> {
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
    pub(super) fn bat_region(&self) -> Option<&[u8]> {
        self.region_for_guid(&BAT_REGION_GUID)
    }

    /// Resolve the metadata region data.
    pub(super) fn metadata_region(&self) -> Option<&[u8]> {
        self.region_for_guid(&METADATA_REGION_GUID)
    }

    /// Determine whether this is a differencing disk (`has_parent` flag).
    pub(super) fn has_parent(&self) -> bool {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(fp) = meta.items().file_parameters()
        {
            return fp.has_parent();
        }
        false
    }

    /// Extract the current header's `LogGuid`.
    pub(super) fn current_log_guid(header: &Header<'_>) -> Result<Guid> {
        let current = header.header(0)?;
        Ok(current.log_guid())
    }

    /// Compute the chunk ratio for BAT interpretation.
    pub(super) fn chunk_ratio(&self) -> u64 {
        let block_size = u64::from(self.block_size());
        let logical_sector_size = u64::from(self.logical_sector_size());
        if block_size == 0 || logical_sector_size == 0 {
            return 0;
        }
        crate::common::compute_chunk_ratio(block_size, logical_sector_size)
    }

    /// Get block size from metadata.
    pub(super) fn block_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(fp) = meta.items().file_parameters()
        {
            return fp.block_size();
        }
        0
    }

    /// Get logical sector size from metadata.
    pub(super) fn logical_sector_size(&self) -> u32 {
        if let Some(meta_data) = self.metadata_region()
            && let Ok(meta) = crate::metadata::Metadata::new(meta_data)
            && let Ok(lss) = meta.items().logical_sector_size()
        {
            return lss;
        }
        0
    }

    /// Get virtual disk size from metadata.
    pub(super) fn virtual_disk_size(&self) -> u64 {
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
