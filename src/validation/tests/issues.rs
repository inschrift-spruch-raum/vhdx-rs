use super::super::{Error, SpecValidator, ValidationIssue};
use super::helpers::build_test_vhdx;
use crate::constants::{
    HEADER2_OFFSET, METADATA_TABLE_SIZE, REGION_TABLE_SIZE, REGION_TABLE1_OFFSET,
    REGION_TABLE2_OFFSET,
};
use bitvec::prelude::*;
use crc32c::crc32c;

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
        0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA,
        0xBB,
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
        issues.iter().map(ValidationIssue::code).collect::<Vec<_>>()
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
        0xDE, 0xAD, 0xBE, 0xEF, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA,
        0xBB,
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
        issues.iter().map(ValidationIssue::code).collect::<Vec<_>>()
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
        0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA,
        0xBB,
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
