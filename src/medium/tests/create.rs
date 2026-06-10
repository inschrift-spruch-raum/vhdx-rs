use bitvec::prelude::*;
use crc32c::crc32c;
use std::io::Read;

use crate::constants::{
    HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, METADATA_REGION_SIZE, METADATA_TABLE_SIZE,
    REGION_TABLE_SIZE, REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET, TABLE_ENTRY_SIZE,
    TABLE_HEADER_SIZE,
};
use crate::metadata::EntryFlags;
use crate::types::{self, Guid};
use crate::{CreateOptions, Medium, Result};

fn open_test_medium(path: impl AsRef<std::path::Path>) -> Result<Medium> {
    let inner = std::fs::File::open(path)?;
    Medium::open(inner).finish()
}

fn create_test_medium(path: impl AsRef<std::path::Path>) -> CreateOptions<std::fs::File> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("prepare caller-owned create medium");
    Medium::create(inner)
}

// -- Create tests -------------------------------------------------------

/// Helper: create a VHDX in `target/test-output/` and return the bytes.
fn create_test_vhdx(size: u64) -> Vec<u8> {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_output = project_root.join("target").join("test-output");
    std::fs::create_dir_all(&test_output).expect("create test-output dir");
    let test_id: u64 = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    )
    .unwrap();
    let test_dir = test_output.join(format!("vhdx-test-{test_id}"));
    std::fs::create_dir_all(&test_dir).expect("create test dir");
    let path = test_dir.join("test.vhdx");
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("prepare caller-owned create medium");
    Medium::create(inner)
        .size(size)
        .finish()
        .expect("create vhdx");
    let mut buf = Vec::new();
    let mut f = std::fs::File::open(&path).expect("reopen");
    f.read_to_end(&mut buf).expect("read");
    // Clean up the test artifacts.
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&test_dir);
    buf
}

#[test]
fn create_1gb_default() {
    let data = create_test_vhdx(1024 * 1024 * 1024);
    assert!(data.len() > 1024 * 1024, "file must extend past 1 MB");

    // File type identifier signature == "vhdxfile"
    assert_eq!(&data[0..8], b"vhdxfile");

    // Header 1 signature == "head"
    assert_eq!(
        &data[usize::try_from(HEADER1_OFFSET).unwrap()..][..4],
        b"head"
    );
    // Header 2 signature == "head"
    assert_eq!(
        &data[usize::try_from(HEADER2_OFFSET).unwrap()..][..4],
        b"head"
    );

    // Region table 1 signature == "regi"
    assert_eq!(
        &data[usize::try_from(REGION_TABLE1_OFFSET).unwrap()..][..4],
        b"regi"
    );
    // Region table 2 signature == "regi"
    assert_eq!(
        &data[usize::try_from(REGION_TABLE2_OFFSET).unwrap()..][..4],
        b"regi"
    );

    validate_header_crc(&data, usize::try_from(HEADER1_OFFSET).unwrap());
    validate_header_crc(&data, usize::try_from(HEADER2_OFFSET).unwrap());

    validate_region_crc(&data, usize::try_from(REGION_TABLE1_OFFSET).unwrap());
    validate_region_crc(&data, usize::try_from(REGION_TABLE2_OFFSET).unwrap());
}

#[test]
fn sequence_number_is_zero() {
    let data = create_test_vhdx(1024 * 1024 * 1024);
    let seq = u64::from_le_bytes(
        data[usize::try_from(HEADER1_OFFSET).unwrap() + 8..][..8]
            .try_into()
            .unwrap(),
    );
    assert_eq!(seq, 0);
}

#[test]
fn version_is_one() {
    let data = create_test_vhdx(1024 * 1024 * 1024);
    let version_offset = usize::try_from(HEADER1_OFFSET).unwrap() + 4 + 4 + 8 + 16 + 16 + 16 + 2;
    let version = u16::from_le_bytes(data[version_offset..][..2].try_into().unwrap());
    assert_eq!(version, 1);
}

#[test]
fn region_table_has_two_entries() {
    let data = create_test_vhdx(1024 * 1024 * 1024);
    let count = u32::from_le_bytes(
        data[usize::try_from(REGION_TABLE1_OFFSET).unwrap() + 8..][..4]
            .try_into()
            .unwrap(),
    );
    assert_eq!(count, 2);
}

#[test]
fn validation_rejects_zero_size() {
    let tf = tempfile::NamedTempFile::new().unwrap();
    let result = create_test_medium(tf.path()).finish();
    assert!(result.is_err());
}

#[test]
fn validation_rejects_invalid_block_size() {
    let tf = tempfile::NamedTempFile::new().unwrap();
    let result = create_test_medium(tf.path())
            .size(1024 * 1024 * 1024)
            .block_size(3 * 1024 * 1024) // not power of 2
            .finish();
    assert!(result.is_err());
}

#[test]
fn validation_rejects_invalid_sector_size() {
    let tf = tempfile::NamedTempFile::new().unwrap();
    let result = create_test_medium(tf.path())
        .size(1024 * 1024 * 1024)
        .logical_sector_size(1024)
        .finish();
    assert!(result.is_err());
}

#[test]
fn validation_allows_physical_lt_logical() {
    let tf = tempfile::NamedTempFile::new().unwrap();
    let result = create_test_medium(tf.path())
        .size(1024 * 1024 * 1024)
        .logical_sector_size(4096)
        .physical_sector_size(512)
        .finish();
    assert!(
        result.is_ok(),
        "physical(512) < logical(4096) should be valid per VHDX spec"
    );
}

// -- Metadata verification tests ----------------------------------------

/// Helper: create a VHDX in `target/test-output/` and return bytes + path.
fn create_test_vhdx_detailed(
    size: u64, block_size: u32, fixed: bool, parent_path: Option<&std::path::Path>,
) -> (Vec<u8>, std::path::PathBuf) {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let test_output = project_root.join("target").join("test-output");
    std::fs::create_dir_all(&test_output).expect("create test-output dir");
    let test_id: u64 = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    )
    .unwrap();
    let test_dir = test_output.join(format!("vhdx-test-{test_id}"));
    std::fs::create_dir_all(&test_dir).expect("create test dir");

    // If a parent path is specified, create a minimal parent VHDX first
    let actual_parent_path = if parent_path.is_some() {
        let parent_path_buf = test_dir.join("parent.vhdx");
        create_test_medium(&parent_path_buf)
                .size(10 * 1024 * 1024 * 1024u64) // 10 GB
                .block_size(block_size)
                .fixed(false)
                .finish()
                .expect("create parent vhdx");
        Some(parent_path_buf)
    } else {
        None
    };

    let path = test_dir.join("test.vhdx");
    let mut opts = create_test_medium(&path)
        .size(size)
        .block_size(block_size)
        .fixed(fixed);
    if let Some(ref p) = actual_parent_path {
        let mut parent = open_test_medium(p).expect("open caller-owned parent");
        opts = opts
            .parent(&mut parent, p)
            .expect("set caller-owned parent");
    }
    opts.finish().expect("create vhdx");
    let mut buf = Vec::new();
    let mut f = std::fs::File::open(&path).expect("reopen");
    f.read_to_end(&mut buf).expect("read");
    // Clean up the test artifacts.
    let _ = std::fs::remove_file(&path);
    if let Some(ref p) = actual_parent_path {
        let _ = std::fs::remove_file(p);
    }
    let _ = std::fs::remove_dir(&test_dir);
    (buf, path)
}

/// Read metadata table `entry_count` from raw bytes at `metadata_offset`.
fn read_entry_count(data: &[u8], metadata_offset: u64) -> u16 {
    let off = usize::try_from(metadata_offset).unwrap();
    u16::from_le_bytes(data[off + 10..off + 12].try_into().unwrap())
}

/// Read a metadata table entry's GUID and offset from the raw buffer.
fn read_entry(data: &[u8], metadata_offset: u64, entry_idx: usize) -> (Guid, u32, u32, u32) {
    let base = usize::try_from(metadata_offset).unwrap()
        + TABLE_HEADER_SIZE as usize
        + entry_idx * TABLE_ENTRY_SIZE as usize;
    let guid_bytes: [u8; 16] = data[base..base + 16].try_into().unwrap();
    let guid = Guid::from_bytes(guid_bytes);
    let offset = u32::from_le_bytes(data[base + 16..base + 20].try_into().unwrap());
    let length = u32::from_le_bytes(data[base + 20..base + 24].try_into().unwrap());
    let flags = u32::from_le_bytes(data[base + 24..base + 28].try_into().unwrap());
    (guid, offset, length, flags)
}

#[test]
fn create_dynamic_metadata_table() {
    let (data, _path) =
        create_test_vhdx_detailed(1024 * 1024 * 1024, 32 * 1024 * 1024, false, None);

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(
        1024 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    );
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

    // Signature "metadata"
    let meta_off = usize::try_from(metadata_offset).unwrap();
    assert_eq!(&data[meta_off..][..8], b"metadata");

    // Entry count = 5 (dynamic, no parent)
    let count = read_entry_count(&data, metadata_offset);
    assert_eq!(count, 5, "dynamic disk should have 5 metadata entries");

    // Collect entries
    let mut found = std::collections::HashSet::new();
    for i in 0..5 {
        let (guid, offset, length, flags) =
            read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
        found.insert(format!("{guid}"));
        // Item offsets must be >= 64KB (METADATA_TABLE_SIZE)
        assert!(
            offset >= METADATA_TABLE_SIZE,
            "entry {i} offset {offset} < 64KB"
        );
        // Flags: IsRequired bit (2 per MS-VHDX 搂2.6.1.2) must be set
        let flags_bytes = flags.to_le_bytes();
        let ef = EntryFlags::new(&flags_bytes);
        assert!(
            ef.is_required(),
            "entry {i} flags {flags:#x} missing IsRequired"
        );
        // Validate known GUIDs
        match i {
            0 => {
                assert_eq!(guid, types::StandardItems::FILE_PARAMETERS);
                assert_eq!(length, 8);
                assert!(
                    !ef.is_virtual_disk(),
                    "FileParameters should not be IsVirtualDisk"
                );
            }
            1 => {
                assert_eq!(guid, types::StandardItems::VIRTUAL_DISK_SIZE);
                assert_eq!(length, 8);
                assert!(
                    ef.is_virtual_disk(),
                    "VirtualDiskSize should be IsVirtualDisk"
                );
            }
            2 => {
                assert_eq!(guid, types::StandardItems::VIRTUAL_DISK_ID);
                assert_eq!(length, 16);
            }
            3 => {
                assert_eq!(guid, types::StandardItems::LOGICAL_SECTOR_SIZE);
                assert_eq!(length, 4);
            }
            4 => {
                assert_eq!(guid, types::StandardItems::PHYSICAL_SECTOR_SIZE);
                assert_eq!(length, 4);
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(found.len(), 5);
}

#[test]
fn create_dynamic_metadata_items_values() {
    let (data, _path) =
        create_test_vhdx_detailed(10 * 1024 * 1024 * 1024u64, 32 * 1024 * 1024, false, None);

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(
        10 * 1024 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    );
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

    // Find each item by GUID and verify its value
    for i in 0..5 {
        let (guid, offset, length, _flags) =
            read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
        let meta_off = usize::try_from(metadata_offset).unwrap();
        let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
            [..usize::try_from(length).unwrap()];

        if guid == types::StandardItems::FILE_PARAMETERS {
            // MS-VHDX 搂2.6.2.1: block_size (u32) + bit_fields (u32)
            let fp_block = u32::from_le_bytes(item_data[..4].try_into().unwrap());
            assert_eq!(fp_block, 32 * 1024 * 1024, "block size mismatch");
            let fp = item_data[4..8].view_bits::<Lsb0>();
            assert!(!fp[0], "LeaveBlockAllocated should be 0 for dynamic");
            assert!(!fp[1], "HasParent should be 0 for non-differencing");
        } else if guid == types::StandardItems::VIRTUAL_DISK_SIZE {
            let vs = u64::from_le_bytes(item_data[..8].try_into().unwrap());
            assert_eq!(vs, 10 * 1024 * 1024 * 1024, "virtual disk size mismatch");
        } else if guid == types::StandardItems::VIRTUAL_DISK_ID {
            assert_eq!(length, 16);
            // Should be non-zero (random GUID)
            let all_zero = item_data.iter().all(|&b| b == 0);
            assert!(!all_zero, "VirtualDiskId should not be zero");
        } else if guid == types::StandardItems::LOGICAL_SECTOR_SIZE
            || guid == types::StandardItems::PHYSICAL_SECTOR_SIZE
        {
            let sector_size = u32::from_le_bytes(item_data[..4].try_into().unwrap());
            assert_eq!(sector_size, 4096);
        }
    }
}

#[test]
fn create_dynamic_bat_entries_not_present() {
    let size = 1024 * 1024 * 1024u64;
    let block_size = 32 * 1024 * 1024u32;
    let (data, _path) = create_test_vhdx_detailed(size, block_size, false, None);

    let bat_offset = 2 * 1024 * 1024; // BAT_REGION_OFFSET
    let (_num_payload, _num_sb, total_entries, _cr) =
        CreateOptions::<std::fs::File>::compute_bat_entry_counts(size, block_size, 4096);

    for i in 0..usize::try_from(total_entries).unwrap() {
        let entry_bytes: [u8; 8] = data[bat_offset + i * 8..][..8].try_into().unwrap();
        let raw = u64::from_le_bytes(entry_bytes);
        assert_eq!(
            raw, 0,
            "BAT entry {i} should be 0 (PAYLOAD_BLOCK_NOT_PRESENT) for dynamic disk"
        );
    }
}

#[test]
fn create_fixed_bat_entries_fully_present() {
    let size = 128 * 1024 * 1024u64; // 128 MB
    let block_size = 32 * 1024 * 1024u32;
    let (data, _path) = create_test_vhdx_detailed(size, block_size, true, None);

    let bat_offset = 2 * 1024 * 1024;
    let (num_payload, num_sb, total_entries, chunk_ratio) =
        CreateOptions::<std::fs::File>::compute_bat_entry_counts(size, block_size, 4096);

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(size, block_size, 4096);
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);
    let raw_first_mb = (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(1024 * 1024);
    let payload_align = u64::from(block_size) / (1024 * 1024);
    let first_payload_offset_mb = raw_first_mb.div_ceil(payload_align) * payload_align;

    let mut sb_seen: u64 = 0;
    let mut payload_idx: u64 = 0;

    for i in 0..total_entries {
        let entry_bytes: [u8; 8] = data[bat_offset + usize::try_from(i).unwrap() * 8..][..8]
            .try_into()
            .unwrap();
        let raw = u64::from_le_bytes(entry_bytes);

        let payloads_before = i - sb_seen;
        let is_sb =
            payloads_before > 0 && payloads_before.is_multiple_of(chunk_ratio) && sb_seen < num_sb;
        if is_sb {
            // Sector bitmap entry: NotPresent
            assert_eq!(
                raw, 0,
                "BAT sector bitmap entry {i} should be 0 (NotPresent) for fixed disk"
            );
            sb_seen += 1;
        } else {
            // Payload entry: FullyPresent with sequential offset
            let raw_bytes = raw.to_le_bytes();
            let raw_bits = raw_bytes.view_bits::<Lsb0>();
            let state: u8 = raw_bits[0..3].load::<u8>();
            assert_eq!(
                state, 6,
                "BAT payload entry {i} should be FullyPresent (6) for fixed disk"
            );

            let file_offset_mb: u64 = raw_bits[20..64].load::<u64>();
            let expected_mb =
                first_payload_offset_mb + payload_idx * u64::from(block_size) / (1024 * 1024);
            assert_eq!(
                file_offset_mb, expected_mb,
                "BAT entry {i} (payload_idx={payload_idx}) offset mismatch"
            );
            payload_idx += 1;
        }
    }

    // Verify total payload count
    // When num_payload < chunk_ratio, the interleaving formula classifies
    // all entries as payload (the SB only appears at index == chunk_ratio).
    // This keeps the writer consistent with the bat.rs reader.
    let expected_payload_count = if num_payload < chunk_ratio {
        total_entries
    } else {
        num_payload
    };
    assert_eq!(
        payload_idx, expected_payload_count,
        "should have {expected_payload_count} payload entries (num_payload={num_payload}, chunk_ratio={chunk_ratio})"
    );
}

#[test]
fn create_fixed_file_size_includes_payloads() {
    let size = 128 * 1024 * 1024u64; // 128 MB
    let block_size = 32 * 1024 * 1024u32;
    let (data, _path) = create_test_vhdx_detailed(size, block_size, true, None);

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(size, block_size, 4096);
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);
    let raw_first_mb = (metadata_offset + u64::from(METADATA_REGION_SIZE)).div_ceil(1024 * 1024);
    let payload_align = u64::from(block_size) / (1024 * 1024);
    let first_payload_mb = raw_first_mb.div_ceil(payload_align) * payload_align;
    let first_payload = first_payload_mb * (1024 * 1024);

    let (num_payload, _num_sb, _total, _cr) =
        CreateOptions::<std::fs::File>::compute_bat_entry_counts(size, block_size, 4096);
    let expected_end = first_payload + num_payload * u64::from(block_size);

    assert_eq!(
        data.len() as u64,
        expected_end,
        "fixed disk file should extend to cover all payload blocks"
    );

    // Verify payload blocks are zero-filled (spot-check first and last)
    let first_block_start = usize::try_from(first_payload).unwrap();
    let last_block_start = usize::try_from(first_payload).unwrap()
        + (usize::try_from(num_payload).unwrap() - 1) * usize::try_from(block_size).unwrap();
    assert!(
        data[first_block_start..first_block_start + 1024]
            .iter()
            .all(|&b| b == 0),
        "first payload block should be zero-filled"
    );
    assert!(
        data[last_block_start..last_block_start + 1024]
            .iter()
            .all(|&b| b == 0),
        "last payload block should be zero-filled"
    );
}

#[test]
fn create_differencing_has_parent_locator() {
    let (data, _path) = create_test_vhdx_detailed(
        10 * 1024 * 1024 * 1024,
        32 * 1024 * 1024,
        false,
        Some(std::path::Path::new("parent.vhdx")),
    );

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(
        10 * 1024 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    );
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

    // Should have 6 entries (including ParentLocator)
    let count = read_entry_count(&data, metadata_offset);
    assert_eq!(count, 6, "differencing disk should have 6 metadata entries");

    // Find the ParentLocator entry
    let mut found_pl = false;
    for i in 0..6 {
        let (guid, offset, length, _flags) =
            read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
        if guid == types::StandardItems::PARENT_LOCATOR {
            found_pl = true;
            assert!(length > 0, "ParentLocator should have non-zero length");
            assert!(
                offset >= METADATA_TABLE_SIZE,
                "parent locator offset < 64KB"
            );

            // Verify the parent locator data
            let meta_off = usize::try_from(metadata_offset).unwrap();
            let pl_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                [..usize::try_from(length).unwrap()];

            // Locator header: locator_type GUID (16 bytes) + reserved (2) + kv_count (2)
            let locator_type_bytes: [u8; 16] = pl_data[..16].try_into().unwrap();
            assert_eq!(
                Guid::from_bytes(locator_type_bytes),
                types::StandardItems::LOCATOR_TYPE_VHDX,
                "locator type should be VHDX"
            );
            let kv_count = u16::from_le_bytes(pl_data[18..20].try_into().unwrap());
            assert_eq!(
                kv_count, 2,
                "should have 2 KV entries (parent_linkage, relative_path)"
            );
        }
    }
    assert!(found_pl, "ParentLocator entry not found");
}

#[test]
fn create_differencing_file_parameters_has_parent_flag() {
    let (data, _path) = create_test_vhdx_detailed(
        1024 * 1024 * 1024,
        32 * 1024 * 1024,
        false,
        Some(std::path::Path::new("parent.vhdx")),
    );

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(
        1024 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    );
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

    for i in 0..6 {
        let (guid, offset, length, _flags) =
            read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
        if guid == types::StandardItems::FILE_PARAMETERS {
            let meta_off = usize::try_from(metadata_offset).unwrap();
            let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                [..usize::try_from(length).unwrap()];
            let fp = item_data[4..8].view_bits::<Lsb0>();
            assert!(fp[1], "HasParent flag should be set for differencing disk");
            assert!(
                !fp[0],
                "LeaveBlockAllocated should be 0 for dynamic differencing"
            );
            return;
        }
    }
    panic!("FileParameters not found in differencing disk metadata");
}

#[test]
fn create_fixed_file_parameters_leave_block_allocated() {
    let (data, _path) = create_test_vhdx_detailed(128 * 1024 * 1024, 32 * 1024 * 1024, true, None);

    let bat_size = CreateOptions::<std::fs::File>::calculate_bat_size(
        128 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    );
    let metadata_offset = 2 * 1024 * 1024 + u64::from(bat_size);

    for i in 0..5 {
        let (guid, offset, length, _flags) =
            read_entry(&data, metadata_offset, usize::try_from(i).unwrap());
        if guid == types::StandardItems::FILE_PARAMETERS {
            let meta_off = usize::try_from(metadata_offset).unwrap();
            let item_data = &data[meta_off + usize::try_from(offset).unwrap()..]
                [..usize::try_from(length).unwrap()];
            let fp = item_data[4..8].view_bits::<Lsb0>();
            assert!(
                fp[0],
                "LeaveBlockAllocated flag should be set for fixed disk"
            );
            return;
        }
    }
    panic!("FileParameters not found in fixed disk metadata");
}

// -- helpers --

fn validate_header_crc(data: &[u8], offset: usize) {
    let mut slice = data[offset..][..HEADER_SIZE as usize].to_vec();
    let stored = u32::from_le_bytes(slice[4..8].try_into().unwrap());
    slice[4..8].copy_from_slice(&0u32.to_le_bytes());
    let computed = crc32c(&slice);
    assert_eq!(stored, computed, "header CRC mismatch at offset {offset}");
}

fn validate_region_crc(data: &[u8], offset: usize) {
    let mut slice = data[offset..][..REGION_TABLE_SIZE as usize].to_vec();
    let stored = u32::from_le_bytes(slice[4..8].try_into().unwrap());
    slice[4..8].copy_from_slice(&0u32.to_le_bytes());
    let computed = crc32c(&slice);
    assert_eq!(
        stored, computed,
        "region table CRC mismatch at offset {offset}"
    );
}
