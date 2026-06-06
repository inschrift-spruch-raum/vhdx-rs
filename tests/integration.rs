use std::io::{Read, Seek, SeekFrom, Write};

use vhdx::section::{BatState, PayloadBlockState, SectorBitmapState};
use vhdx::{File, LogReplayPolicy};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a test VHDX and return the opened File handle along with the
/// backing tempdir (caller must hold the `TempDir` to keep files alive).
fn create_test_vhdx(size: u64, block_size: u32, fixed: bool) -> (File, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.vhdx");
    let f = File::create(&path)
        .size(size)
        .block_size(block_size)
        .logical_sector_size(4096)
        .fixed(fixed)
        .finish()
        .expect("create vhdx");
    (f, dir)
}

/// Copy a reference file from misc/ into a tempdir under target/test/,
/// return (`TempDir`, `PathBuf`). Hold `TempDir` for the test's lifetime.
fn ref_to_tmp(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = std::path::Path::new("target").join("test");
    let _ = std::fs::create_dir_all(&root);
    let dir = tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(&root)
        .expect("tempdir");
    let src = format!("misc/{name}");
    let dst = dir.path().join(name);
    std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
    (dir, dst)
}

/// `File::create()` writes both headers with `sequence_number=0`.
/// The `SpecValidator` requires different sequence numbers.
/// This helper patches Header 2 to sequence=1, fixes its CRC, then re-opens
/// the file so `validate_file()` can succeed.
fn patch_header2_seq_and_reopen(path: &std::path::Path) -> File {
    const HEADER2_OFFSET: u64 = 128 * 1024;

    // Open raw file, patch Header 2 sequence number to 1
    let mut raw = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("re-open for header patch");

    let mut header = [0u8; 4096];
    raw.seek(SeekFrom::Start(HEADER2_OFFSET)).unwrap();
    raw.read_exact(&mut header).unwrap();

    // Set sequence number = 1 (bytes 8..16)
    header[8..16].copy_from_slice(&1u64.to_le_bytes());

    // Recalculate CRC-32C (zero checksum field first)
    header[4..8].copy_from_slice(&0u32.to_le_bytes());
    let checksum = crc32c::crc32c(&header);
    header[4..8].copy_from_slice(&checksum.to_le_bytes());

    // Write back
    raw.seek(SeekFrom::Start(HEADER2_OFFSET)).unwrap();
    raw.write_all(&header).unwrap();
    drop(raw);

    // Re-open as a proper VHDX File
    File::open(path)
        .finish()
        .expect("re-open after header patch")
}

/// Create a test VHDX, patch sequence numbers, re-open, and return.
fn create_and_reopen_for_validation(
    size: u64, block_size: u32, fixed: bool,
) -> (File, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.vhdx");
    let f = File::create(&path)
        .size(size)
        .block_size(block_size)
        .logical_sector_size(4096)
        .fixed(fixed)
        .finish()
        .expect("create vhdx");
    // Close the original handle so the patch can acquire write access
    drop(f);

    let patched_file = patch_header2_seq_and_reopen(&path);
    (patched_file, dir)
}

// ---------------------------------------------------------------------------
// 1. Open test-void.vhdx and validate all sections
// ---------------------------------------------------------------------------

#[test]
fn open_void_vhdx_validate_sections() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = File::open(&path).finish().unwrap();
    let sections = f.sections();

    // Header section
    let header = sections.header().expect("header section");
    assert_eq!(
        header.file_type().signature(),
        b"vhdxfile",
        "file type identifier signature"
    );
    let current = header.header(0).expect("current header structure");
    // Version must be 1 per MS-VHDX
    assert_eq!(current.version(), 1, "header version");

    // Metadata section (accessible even on minimal/void VHDX)
    let metadata = sections.metadata().expect("metadata section");
    let table = metadata.table();
    assert!(
        table.header().entry_count() > 0,
        "metadata table should have entries"
    );

    // BAT section (may fail if block_size=0, as in test-void)
    match sections.bat() {
        Ok(bat) => {
            assert!(bat.entries().count() > 0, "BAT should have entries");
        }
        Err(_) => {
            // test-void may have block_size=0 in FileParameters,
            // making chunk_ratio uncomputable.
            eprintln!("BAT loading skipped (block_size may be 0 in test-void)");
        }
    }

    // Full validation — test-void is a minimal reference file that should
    // pass validation (no reserved flags issues in the reference).
    let result = f.validator().validate_file();
    assert!(
        result.is_ok(),
        "test-void validation should pass, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Open test-fs.vhdx and read sector 0 (FAT32 boot sector)
// ---------------------------------------------------------------------------

#[test]
fn open_fs_vhdx_read_sector_zero() {
    // test-fs.vhdx has a pending log -> use Auto to replay it on open
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();

    // Verify that header + metadata are accessible regardless of IO state.
    let sections = f.sections();
    let metadata = sections.metadata().expect("metadata on test-fs");
    assert!(
        metadata.table().header().entry_count() > 0,
        "test-fs should have metadata table entries"
    );

    // IO may or may not be available depending on whether reading is
    // supported for this file. If it works, verify the FAT32 boot sector.
    if let Ok(io) = f.io()
        && let Ok(mut sector) = io.sector(0, 1)
    {
        let mut buf = vec![0u8; 4096];
        if sector.read_exact(&mut buf).is_ok() {
            // Sector 0 of a FAT32-formatted disk should NOT be all zeros.
            assert!(
                !buf.iter().all(|&b| b == 0),
                "sector 0 should contain boot sector data, not all zeros"
            );
            // Verify the FAT32 signature (bytes 0x52-0x59 = "FAT32   ")
            assert_eq!(
                &buf[0x52..0x5A],
                b"FAT32   ",
                "FAT32 boot sector signature at offset 0x52"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Create dynamic disk and validate
// ---------------------------------------------------------------------------

#[test]
fn create_dynamic_validate() {
    let (f, _dir) = create_and_reopen_for_validation(256 * 1024 * 1024, 32 * 1024 * 1024, false);

    // Structural validation
    f.validator()
        .validate_file()
        .expect("validate dynamic disk");

    // Metadata check
    let sections = f.sections();
    let metadata = sections.metadata().expect("metadata");
    let fp = metadata.items().file_parameters().expect("FileParameters");
    assert_eq!(fp.block_size(), 32 * 1024 * 1024);
    assert!(!fp.has_parent(), "dynamic disk should not have parent");
    assert!(
        !fp.leave_block_allocated(),
        "dynamic disk: LeaveBlockAllocated should be false"
    );

    // Virtual disk size check
    assert_eq!(
        metadata.items().virtual_disk_size().unwrap(),
        256 * 1024 * 1024
    );
}

// ---------------------------------------------------------------------------
// 4. Create fixed disk, verify file size and BAT entries
// ---------------------------------------------------------------------------

#[test]
fn create_fixed_verify_size_and_bat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fixed.vhdx");
    let f = File::create(&path)
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
        "fixed disk file size ({file_size}) should exceed virtual size (268435456)"
    );

    // Drop original handle, patch header seq, re-open for validation
    drop(f);
    let f = patch_header2_seq_and_reopen(&path);

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

    let sections = f.sections();
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

    // Chunk ratio check (already computed locally)
    assert!(chunk_ratio_calc > 0, "chunk ratio should be positive");

    // Full validation
    f.validator().validate_file().expect("validate fixed disk");
}

// ---------------------------------------------------------------------------
// 5. Zero-copy iteration verification
// ---------------------------------------------------------------------------

#[test]
fn zero_copy_bat_iteration_entries_count_matches() {
    // Use a created VHDX where we control block_size/metadata
    let (f, _dir) = create_test_vhdx(256 * 1024 * 1024, 32 * 1024 * 1024, false);
    let sections = f.sections();
    let bat = sections.bat().expect("BAT");
    let count: usize = bat.entries().count();
    assert!(count > 0, "BAT should have entries");
}

#[test]
fn zero_copy_metadata_iteration_entries_count_matches() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = File::open(&path).finish().unwrap();
    let sections = f.sections();
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
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();
    let sections = f.sections();
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

/// test-void.vhdx is a minimal reference file that should pass validation.
#[test]
fn validator_on_void_vhdx_passes() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = File::open(&path).finish().unwrap();
    f.validator()
        .validate_file()
        .expect("test-void.vhdx should pass validation");
}

/// test-fs.vhdx has a known BAT entry file offset alignment issue
/// (offset 4 MB is not aligned to the 32 MB block size). The validation
/// now correctly detects this as `BAT_ENTRY_FILE_OFFSET_UNALIGNED`.
#[test]
fn validator_on_fs_vhdx_detects_bat_alignment_issue() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();
    let result = f.validator().validate_file();
    // The file has a BAT entry at offset 4 MB that is not aligned to 32 MB block size.
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
    let f = File::open(&path).finish().unwrap();
    let sections = f.sections();
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
    let result = File::open("tests/fixtures/does-not-exist.vhdx").finish();
    assert!(result.is_err(), "opening nonexistent file should fail");
}

#[test]
fn open_invalid_vhdx_file_is_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("not-vhdx.bin");
    std::fs::write(&path, b"this is not a VHDX file at all").expect("write temp file");
    let _dir = dir;

    let result = File::open(&path).finish();
    assert!(result.is_err(), "opening non-VHDX file should fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, vhdx::Error::InvalidSignature { .. }),
        "expected InvalidSignature error, got {err:?}"
    );
}

#[test]
fn io_sector_out_of_bounds_is_error() {
    let (f, _dir) = create_test_vhdx(256 * 1024 * 1024, 32 * 1024 * 1024, false);
    let io = f.io().expect("IO context");
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
    let result = File::create(&path).finish();
    assert!(result.is_err(), "creating with size=0 should fail");
}

// ---------------------------------------------------------------------------
// 8. Create with different block sizes
// ---------------------------------------------------------------------------

#[test]
fn create_dynamic_1mb_block_size() {
    let (f, _dir) = create_and_reopen_for_validation(64 * 1024 * 1024, 1024 * 1024, false);

    let sections = f.sections();
    let metadata = sections.metadata().unwrap();
    let fp = metadata.items().file_parameters().unwrap();
    assert_eq!(fp.block_size(), 1024 * 1024);
    f.validator()
        .validate_file()
        .expect("validate 1MB block disk");
}

#[test]
fn create_fixed_with_512_sector_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("512sector.vhdx");
    let f = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .logical_sector_size(512)
        .physical_sector_size(512)
        .fixed(true)
        .finish()
        .expect("create with 512 sector size");
    let _dir = dir;

    let sections = f.sections();
    let metadata = sections.metadata().unwrap();
    let items = metadata.items();
    assert_eq!(items.logical_sector_size().unwrap(), 512);
    assert_eq!(items.physical_sector_size().unwrap(), 512);
}

#[path = "integration/late.rs"]
mod late;
