use super::{Error, PayloadBlockState, Result, SectorBitmapState, SpecValidator, ValidationIssue};

impl SpecValidator {
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
}
