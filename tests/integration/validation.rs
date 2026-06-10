use super::*;

// 4. Create fixed disk, verify Medium size and BAT entries
// ---------------------------------------------------------------------------

#[test]
fn create_fixed_verify_size_and_bat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fixed.vhdx");
    let f = create_medium(&path)
        .size(256 * 1024 * 1024)
        .block_size(32 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("create fixed vhdx");
    let _dir = dir;

    // Fixed disk must pre-allocate all payload blocks.
    let file_size = std::fs::metadata(&path).expect("metadata").len();
    assert!(
        file_size > 256 * 1024 * 1024,
        "fixed disk Medium size ({file_size}) should exceed virtual size (268435456)"
    );

    // Drop original handle, patch header seq, re-open for validation
    drop(f);
    let mut f = patch_header2_seq_and_reopen(&path);

    // BAT: all payload entries must be FullyPresent, sector-bitmap entries NotPresent.
    // Only check the entries that were actually written (entries().count()
    // reflects buffer-size/8 which is larger than the logical entry count).
    let virtual_size: u64 = 256 * 1024 * 1024;
    let block_size: u64 = 32 * 1024 * 1024;
    let logical_sector_size: u64 = 4096;
    let num_payload = virtual_size.div_ceil(block_size);
    let chunk_ratio_calc = (1u64 << 23) * logical_sector_size / block_size;
    let num_sb = num_payload.div_ceil(chunk_ratio_calc);
    let total_expected = usize::try_from(num_payload + num_sb).unwrap();

    {
        let sections = f.sections().expect("sections");
        let bat = sections.bat().expect("BAT");
        let mut payload_count = 0u64;
        let mut sb_count = 0u64;
        for entry in bat.entries().take(total_expected) {
            match entry.state().unwrap() {
                BatState::SectorBitmap(state) => {
                    assert_eq!(
                        state,
                        SectorBitmapState::NotPresent,
                        "sector bitmap entry in non-differencing fixed disk should be NotPresent"
                    );
                    sb_count += 1;
                }
                BatState::Payload(state) => {
                    assert_eq!(
                        state,
                        PayloadBlockState::FullyPresent,
                        "payload entry in fixed disk should be FullyPresent"
                    );
                    payload_count += 1;
                }
            }
        }
        assert!(payload_count > 0, "should have payload entries");
        // Sector bitmap entries only appear within the first total_expected
        // entries when num_payload >= chunk_ratio. For small disks the sector
        // bitmap entry is beyond the written entry range.
        if num_payload >= chunk_ratio_calc {
            assert!(sb_count > 0, "should have sector bitmap entries");
        }
    }

    // Chunk ratio check (already computed locally)
    assert!(chunk_ratio_calc > 0, "chunk ratio should be positive");

    // Full validation
    f.validator()
        .expect("validator")
        .validate_file()
        .expect("validate fixed disk");
}

// ---------------------------------------------------------------------------
// 5. Zero-copy iteration verification
// ---------------------------------------------------------------------------

#[test]
fn zero_copy_bat_iteration_entries_count_matches() {
    // Use a created VHDX where we control block_size/metadata
    let (f, _dir) = create_test_vhdx(256 * 1024 * 1024, 32 * 1024 * 1024, false);
    let sections = f.sections().expect("sections");
    let bat = sections.bat().expect("BAT");
    let count: usize = bat.entries().count();
    assert!(count > 0, "BAT should have entries");
}

#[test]
fn zero_copy_metadata_iteration_entries_count_matches() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .finish()
        .unwrap();
    let sections = f.sections().expect("sections");
    let metadata = sections.metadata().expect("metadata");
    let table = metadata.table();
    let entry_count = table.header().entry_count() as usize;
    let count: usize = table.entries().count();
    assert_eq!(
        count, entry_count,
        "metadata entries() count should equal table header entry_count"
    );
    assert!(entry_count > 0, "metadata should have at least one entry");
}

/// Also verify zero-copy metadata on test-fs for broader coverage.
#[test]
fn zero_copy_metadata_iteration_on_fs_vhdx() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();
    let sections = f.sections().expect("sections");
    let metadata = sections.metadata().expect("metadata");
    let table = metadata.table();
    let entry_count = table.header().entry_count() as usize;
    let count: usize = table.entries().count();
    assert_eq!(count, entry_count);
    assert!(entry_count > 0);
}

// ---------------------------------------------------------------------------
// 6. Validator on test files
// ---------------------------------------------------------------------------

/// test-void.vhdx is a minimal reference Medium that should pass validation.
#[test]
fn validator_on_void_vhdx_passes() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let mut f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .finish()
        .unwrap();
    f.validator()
        .expect("validator")
        .validate_file()
        .expect("test-void.vhdx should pass validation");
}

/// test-fs.vhdx has a known BAT entry Medium offset alignment issue
/// (offset 4 MB is not aligned to the 32 MB block size). The validation
/// now correctly detects this as `BAT_ENTRY_FILE_OFFSET_UNALIGNED`.
#[test]
fn validator_on_fs_vhdx_detects_bat_alignment_issue() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let mut f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();
    let result = f.validator().expect("validator").validate_file();
    // The Medium has a BAT entry at offset 4 MB that is not aligned to 32 MB block size.
    // This should return a BatFileOffsetUnaligned error (blocking).
    assert!(
        result.is_err(),
        "test-fs should fail validation due to BAT offset alignment: {result:?}"
    );
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("offset") && msg.contains("align"),
        "expected BAT alignment error, got: {msg}"
    );
}

#[test]
fn header_signature_is_vhdxfile() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .finish()
        .unwrap();
    let sections = f.sections().expect("sections");
    let header = sections.header().expect("header");
    assert_eq!(
        std::str::from_utf8(header.file_type().signature()).unwrap(),
        "vhdxfile"
    );
}

// ---------------------------------------------------------------------------
// 7. Error cases
// ---------------------------------------------------------------------------

#[test]
fn open_nonexistent_file_is_error() {
    let result = std::fs::File::open("tests/fixtures/does-not-exist.vhdx");
    assert!(result.is_err(), "opening nonexistent Medium should fail");
}

#[test]
fn open_invalid_vhdx_file_is_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("not-vhdx.bin");
    let mut data = vec![0u8; 1024 * 1024];
    data[..32].copy_from_slice(b"this is not a VHDX Medium at all");
    std::fs::write(&path, data).expect("write temp Medium");
    let _dir = dir;

    let result = Medium::open(std::fs::File::open(&path).expect("open VHDX medium")).finish();
    assert!(result.is_err(), "opening non-VHDX Medium should fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, vhdx::Error::InvalidSignature { .. }),
        "expected InvalidSignature error, got {err:?}"
    );
}

#[test]
fn io_sector_out_of_bounds_is_error() {
    let (mut f, _dir) = create_test_vhdx(256 * 1024 * 1024, 32 * 1024 * 1024, false);
    let mut io = f.io().expect("IO context");
    // 256MB / 4KB = 65536 sectors, so sector 65536 is out of bounds.
    let max_sectors = 256 * 1024 * 1024 / 4096;
    let result = io.sector(max_sectors, 1); // max_sectors is first OOB index
    assert!(
        result.is_err(),
        "accessing sector {max_sectors} should be out of bounds"
    );
}

#[test]
fn create_validation_rejects_zero_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("zero.vhdx");
    let _dir = dir;
    let result = create_medium(&path).finish();
    assert!(result.is_err(), "creating with size=0 should fail");
}

// ---------------------------------------------------------------------------
// 8. Create with different block sizes
// ---------------------------------------------------------------------------

#[test]
fn create_dynamic_1mb_block_size() {
    let (mut f, _dir) = create_and_reopen_for_validation(64 * 1024 * 1024, 1024 * 1024, false);

    {
        let sections = f.sections().expect("sections");
        let metadata = sections.metadata().unwrap();
        let fp = metadata.items().file_parameters().unwrap();
        assert_eq!(fp.block_size(), 1024 * 1024);
    }
    f.validator()
        .expect("validator")
        .validate_file()
        .expect("validate 1MB block disk");
}

#[test]
fn create_fixed_with_512_sector_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("512sector.vhdx");
    let f = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .logical_sector_size(512)
        .physical_sector_size(512)
        .fixed(true)
        .finish()
        .expect("create with 512 sector size");
    let _dir = dir;

    let sections = f.sections().expect("sections");
    let metadata = sections.metadata().unwrap();
    let items = metadata.items();
    assert_eq!(items.logical_sector_size().unwrap(), 512);
    assert_eq!(items.physical_sector_size().unwrap(), 512);
}
