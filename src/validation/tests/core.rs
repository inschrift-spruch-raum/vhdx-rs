use super::super::{SpecValidator, ValidationIssue};
use super::helpers::build_test_vhdx;
use crate::constants::{HEADER1_OFFSET, HEADER2_OFFSET, REGION_TABLE_SIZE, REGION_TABLE1_OFFSET};
use crc32c::crc32c;

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
