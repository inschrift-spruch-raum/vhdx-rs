use super::{
    BAT_REGION_GUID, Error, Guid, Header, METADATA_REGION_GUID, Result, SpecValidator,
    ValidationIssue,
};

impl SpecValidator<'_> {
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
    pub(super) fn parse_header(&self) -> Result<Header<'_>> {
        Header::new(self.data)
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
