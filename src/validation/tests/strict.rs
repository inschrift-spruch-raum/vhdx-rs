use super::super::SpecValidator;
use super::helpers::build_test_vhdx;
use crate::constants::{REGION_TABLE_SIZE, REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET};
use crc32c::crc32c;

/// strict=false, optional unknown region entry → Ok
#[test]
fn test_strict_false_optional_unknown_region_passes() {
    let mut buf = build_test_vhdx();

    // Add a third region table entry with an unknown GUID (required=0)
    // Region table 1 is at 192KB. Current: 2 entries (header 16 bytes + 2*32 = 80 bytes used).
    let rt_offset = REGION_TABLE1_OFFSET as usize;
    // Update entry count: 2 → 3
    buf[rt_offset + 8..rt_offset + 12].copy_from_slice(&3u32.to_le_bytes());

    // Write entry 2: unknown GUID, required=0, valid offset/length
    let entry_start = rt_offset + 16 + 2 * 32;
    let unknown_guid: [u8; 16] = [
        0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xAA,
        0xBB,
    ];
    buf[entry_start..entry_start + 16].copy_from_slice(&unknown_guid);
    // file_offset: 4MB (aligned)
    let offset: u64 = 4 * 1024 * 1024;
    buf[entry_start + 16..entry_start + 24].copy_from_slice(&offset.to_le_bytes());
    // length: 1MB (aligned)
    let length: u32 = 1024 * 1024;
    buf[entry_start + 24..entry_start + 28].copy_from_slice(&length.to_le_bytes());
    // required: 0 (optional)
    buf[entry_start + 28..entry_start + 32].copy_from_slice(&0u32.to_le_bytes());

    // Fix CRC for RT1 (zero out checksum field first, then compute)
    buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&0u32.to_le_bytes());
    let checksum = crc32c(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
    buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

    // Do the same for RT2 at 256KB
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

    // Extend buffer to cover the new region offset
    let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
    if buf.len() < needed {
        buf.resize(needed, 0);
    }

    // strict=false → optional unknown should pass
    let validator = SpecValidator::new(&buf, false);
    assert!(validator.validate_region_table().is_ok());
}

/// strict=false, required unknown region entry → Err
#[test]
fn test_strict_false_required_unknown_region_fails() {
    let mut buf = build_test_vhdx();

    // Add a third region table entry with an unknown GUID and required=1
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
    // required: 1 (required)
    buf[entry_start + 28..entry_start + 32].copy_from_slice(&1u32.to_le_bytes());

    let checksum = crc32c(&{
        let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
        slice.copy_from_slice(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice
    });
    buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

    // RT2
    let rt2_offset = REGION_TABLE2_OFFSET as usize;
    buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
    let rt2_entry_start = rt2_offset + 16 + 2 * 32;
    buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
    buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
    buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
    buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&1u32.to_le_bytes());
    let checksum2 = crc32c(&{
        let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
        slice.copy_from_slice(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice
    });
    buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

    let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
    if buf.len() < needed {
        buf.resize(needed, 0);
    }

    // strict=false but required unknown → should still fail
    let validator = SpecValidator::new(&buf, false);
    let result = validator.validate_region_table();
    assert!(result.is_err());
    let msg = format!("{result:?}");
    assert!(
        msg.contains("RegionRequiredUnknown"),
        "expected RegionRequiredUnknown, got: {msg}"
    );
}

/// strict=true, optional unknown region entry → Err
#[test]
fn test_strict_true_optional_unknown_region_fails() {
    let mut buf = build_test_vhdx();

    // Add a third region table entry with an unknown GUID and required=0
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

    let checksum = crc32c(&{
        let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
        slice.copy_from_slice(&buf[rt_offset..][..REGION_TABLE_SIZE as usize]);
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice
    });
    buf[rt_offset + 4..rt_offset + 8].copy_from_slice(&checksum.to_le_bytes());

    // RT2
    let rt2_offset = REGION_TABLE2_OFFSET as usize;
    buf[rt2_offset + 8..rt2_offset + 12].copy_from_slice(&3u32.to_le_bytes());
    let rt2_entry_start = rt2_offset + 16 + 2 * 32;
    buf[rt2_entry_start..rt2_entry_start + 16].copy_from_slice(&unknown_guid);
    buf[rt2_entry_start + 16..rt2_entry_start + 24].copy_from_slice(&offset.to_le_bytes());
    buf[rt2_entry_start + 24..rt2_entry_start + 28].copy_from_slice(&length.to_le_bytes());
    buf[rt2_entry_start + 28..rt2_entry_start + 32].copy_from_slice(&0u32.to_le_bytes());
    let checksum2 = crc32c(&{
        let mut slice = vec![0u8; REGION_TABLE_SIZE as usize];
        slice.copy_from_slice(&buf[rt2_offset..][..REGION_TABLE_SIZE as usize]);
        slice[4..8].copy_from_slice(&0u32.to_le_bytes());
        slice
    });
    buf[rt2_offset + 4..rt2_offset + 8].copy_from_slice(&checksum2.to_le_bytes());

    let needed = usize::try_from(offset + u64::from(length)).expect("needed size fits usize");
    if buf.len() < needed {
        buf.resize(needed, 0);
    }

    // strict=true → optional unknown should still fail
    let validator = SpecValidator::new(&buf, true);
    let result = validator.validate_region_table();
    assert!(result.is_err());
    let msg = format!("{result:?}");
    assert!(
        msg.contains("RegionOptionalUnknown"),
        "expected RegionOptionalUnknown, got: {msg}"
    );
}
