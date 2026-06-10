use super::{Error, Guid, Header, Result, SignaturePosition, SpecValidator, ValidationIssue};

impl SpecValidator {
    /// Validate the log section.
    ///
    /// # Errors
    ///
    /// Returns an error when log entry integrity or sequencing checks fail.
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

    fn read_current_header_log_guid(header: &Header<'_>) -> Result<Guid> {
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
}
