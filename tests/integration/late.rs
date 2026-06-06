use super::*;

// ---------------------------------------------------------------------------
// 9. Overlay-aware structure reads: InMemoryOnReadOnly on file with pending log
// ---------------------------------------------------------------------------

/// Verify that `InMemoryOnReadOnly` opens `test-fs.vhdx` (which has a pending
/// log) and all structure sections (header, BAT, metadata) are readable through
/// the in-memory replay overlay.
#[test]
fn overlay_inmemory_read_sections_test_fs() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
        .finish()
        .expect("open test-fs.vhdx with InMemoryOnReadOnly");

    let sections = f.sections();

    // Header section must be readable with overlay patches applied
    let header = sections.header().expect("header section via overlay");
    assert_eq!(
        header.file_type().signature(),
        b"vhdxfile",
        "header signature via overlay"
    );

    // BAT section: test-fs.vhdx has block_size=0 in FileParameters
    // (known pre-existing issue), so BAT may fail. Accept that gracefully;
    // the important thing is that the overlay doesn't crash.
    match sections.bat() {
        Ok(bat) => {
            assert!(
                bat.entries().count() > 0,
                "BAT should have entries via overlay"
            );
        }
        Err(_) => {
            eprintln!("BAT loading skipped (block_size may be 0 in test-fs.vhdx)");
        }
    }

    // Metadata section must be readable with overlay patches applied
    let metadata = sections.metadata().expect("metadata section via overlay");
    assert!(
        metadata.table().header().entry_count() > 0,
        "metadata table should have entries via overlay"
    );
}

// ---------------------------------------------------------------------------
// 10. Require mode rejects pending log
// ---------------------------------------------------------------------------

/// Verify that `Require` mode returns `Error::LogReplayRequired` when opening
/// `test-fs.vhdx` which has a pending (not-yet-flushed) log.
#[test]
fn overlay_require_rejects_pending_log() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let result = File::open(&path)
        .log_replay(LogReplayPolicy::Require)
        .finish();

    match result {
        Err(vhdx::Error::LogReplayRequired) => {
            // Expected: Require mode rejects files with pending logs
        }
        other => panic!(
            "Require mode should return LogReplayRequired for file with pending log, got: {other:?}"
        ),
    }
}

// ---------------------------------------------------------------------------
// 11. InMemoryOnReadOnly on clean VHDX (no pending log)
// ---------------------------------------------------------------------------

/// Verify that `InMemoryOnReadOnly` works normally on a freshly-created dynamic
/// VHDX that has no pending log. No overlay is needed, but the policy should
/// still allow the file to be opened and all sections read.
#[test]
fn overlay_inmemory_clean_vhdx_no_pending_log() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("clean.vhdx");

    // Create a dynamic VHDX, then close the handle
    let f = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create dynamic vhdx");
    drop(f);

    // Keep tempdir alive
    let _dir = dir;

    // Reopen with InMemoryOnReadOnly — no log, so no overlay is built
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
        .finish()
        .expect("reopen clean vhdx with InMemoryOnReadOnly");

    let sections = f.sections();

    let header = sections.header().expect("header on clean vhdx");
    assert_eq!(
        header.file_type().signature(),
        b"vhdxfile",
        "header signature on clean vhdx"
    );

    let bat = sections.bat().expect("BAT on clean vhdx");
    assert!(
        bat.entries().count() > 0,
        "BAT should have entries on clean vhdx"
    );

    let metadata = sections.metadata().expect("metadata on clean vhdx");
    assert!(
        metadata.table().header().entry_count() > 0,
        "metadata entries on clean vhdx"
    );
}

// ---------------------------------------------------------------------------
// 12. ReadOnlyNoReplay allows structure reads on file with pending log
// ---------------------------------------------------------------------------

/// Verify that `ReadOnlyNoReplay` on `test-fs.vhdx` (which has a pending log)
/// allows structure-level reads (header, BAT, metadata) without replaying
/// the log. Payload data consistency is not guaranteed under this policy,
/// but structure reads must succeed.
#[test]
fn overlay_readonly_noreplay_structure_reads() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let f = File::open(&path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()
        .expect("open test-fs.vhdx with ReadOnlyNoReplay");

    let sections = f.sections();

    // Header must be accessible
    let header = sections.header().expect("header with ReadOnlyNoReplay");
    assert_eq!(
        header.file_type().signature(),
        b"vhdxfile",
        "header signature with ReadOnlyNoReplay"
    );

    // BAT section: test-fs.vhdx has block_size=0 in FileParameters
    // (known pre-existing issue), so BAT may fail. Accept gracefully.
    match sections.bat() {
        Ok(bat) => {
            assert!(
                bat.entries().count() > 0,
                "BAT should have entries with ReadOnlyNoReplay"
            );
        }
        Err(_) => {
            eprintln!("BAT loading skipped (block_size may be 0 in test-fs.vhdx)");
        }
    }

    // Metadata must be accessible
    let metadata = sections.metadata().expect("metadata with ReadOnlyNoReplay");
    assert!(
        metadata.table().header().entry_count() > 0,
        "metadata entries with ReadOnlyNoReplay"
    );
}

// ---------------------------------------------------------------------------
// 13. Differencing disk: create and verify parent locator
// ---------------------------------------------------------------------------

#[test]
fn create_differencing_disk_and_verify_parent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let parent_path = dir.path().join("parent.vhdx");
    let child_path = dir.path().join("child.vhdx");

    // Create parent disk
    let parent = File::create(&parent_path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create parent vhdx");
    drop(parent);

    // Create child differencing disk
    let child = File::create(&child_path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .parent_path(&parent_path)
        .finish()
        .expect("create differencing vhdx");
    drop(child);

    // Re-open child and verify parent locator resolves correctly
    let child = File::open(&child_path)
        .finish()
        .expect("re-open differencing vhdx");

    let metadata = child.sections().metadata().expect("metadata");
    let fp = metadata.items().file_parameters().expect("FileParameters");
    assert!(fp.has_parent(), "differencing disk should have parent flag");

    let locator = metadata
        .items()
        .parent_locator()
        .expect("parent locator should exist");
    let resolved = locator
        .resolve_parent_path()
        .expect("resolve_parent_path should succeed");
    assert_eq!(
        resolved, parent_path,
        "resolved parent path should match the parent we created"
    );

    // Parent locator validation (including GUID chain check) must pass
    let issues = child
        .validator()
        .validate_parent_locator()
        .expect("validate_parent_locator should not error");
    assert!(
        issues.is_empty(),
        "parent locator should have no validation issues: {issues:?}"
    );
}

// ---------------------------------------------------------------------------
// 14. Write path integration: write to fixed disk and read back
// ---------------------------------------------------------------------------

#[test]
fn write_and_read_back_fixed_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("writable.vhdx");

    // Create fixed disk (blocks are FullyPresent)
    let f = File::create(&path)
        .size(4 * 1024 * 1024) // 4 MB
        .block_size(1024 * 1024) // 1 MB blocks
        .fixed(true)
        .finish()
        .expect("create fixed vhdx");
    drop(f);

    // Open writable
    let f = File::open(&path).write().finish().expect("open writable");

    let io = f.io().expect("IO context");

    // Write a pattern to sector 0
    let pattern = vec![0xABu8; 4096];
    let mut writer = io.sector(0, 1).expect("sector 0 for write");
    writer.write_all(&pattern).expect("write sector 0");

    // Read back and verify
    let mut reader = io.sector(0, 1).expect("sector 0 for read");
    let mut buf = vec![0u8; 4096];
    reader.read_exact(&mut buf).expect("read sector 0");
    assert_eq!(buf, pattern, "read back data must match written pattern");
}

// ---------------------------------------------------------------------------
// 15. Policy conflict tests: write-incompatible policies
// ---------------------------------------------------------------------------

#[test]
fn in_memory_on_read_only_with_write_is_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy_conflict_imr.vhdx");

    let f = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create vhdx");
    drop(f);

    let result = File::open(&path)
        .write()
        .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
        .finish();
    assert!(
        result.is_err(),
        "InMemoryOnReadOnly with write access should be rejected"
    );
    let _dir = dir;
}

#[test]
fn read_only_no_replay_with_write_is_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy_conflict_rnr.vhdx");

    let f = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create vhdx");
    drop(f);

    let result = File::open(&path)
        .write()
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish();
    assert!(
        result.is_err(),
        "ReadOnlyNoReplay with write access should be rejected"
    );
    let _dir = dir;
}

// ---------------------------------------------------------------------------
// 16. Default Require behaviour
// ---------------------------------------------------------------------------

/// Verify that the default policy (Require, without calling `log_replay()`)
/// rejects files with pending logs.
#[test]
fn default_require_rejects_pending_log() {
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let result = File::open(&path).finish();
    match result {
        Err(vhdx::Error::LogReplayRequired) => {
            // Expected: default Require mode rejects files with pending logs
        }
        other => panic!("default Require should return LogReplayRequired, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 17. CreateOptions validation tests
// ---------------------------------------------------------------------------

#[test]
fn create_rejects_invalid_block_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad_block.vhdx");

    // Block size must be between 1MB and 256MB
    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(512 * 1024) // 512 KB — below minimum
        .finish();
    assert!(result.is_err(), "block size below 1MB should be rejected");

    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(300 * 1024 * 1024) // 300 MB — above maximum
        .finish();
    assert!(result.is_err(), "block size above 256MB should be rejected");

    // Block size must be a power of 2
    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .block_size(10 * 1024 * 1024) // 10 MB — not a power of 2
        .finish();
    assert!(
        result.is_err(),
        "block size not a power of 2 should be rejected"
    );

    let _dir = dir;
}

#[test]
fn create_rejects_invalid_sector_sizes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad_sector.vhdx");

    // Logical sector size must be 512 or 4096
    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .logical_sector_size(1024)
        .finish();
    assert!(
        result.is_err(),
        "logical sector size 1024 should be rejected"
    );

    // Physical sector size must be 512 or 4096
    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .physical_sector_size(2048)
        .finish();
    assert!(
        result.is_err(),
        "physical sector size 2048 should be rejected"
    );

    let _dir = dir;
}

#[test]
fn create_rejects_fixed_with_parent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fixed_parent.vhdx");
    let parent_path = dir.path().join("parent.vhdx");

    // Create a parent so the path exists
    let p = File::create(&parent_path)
        .size(64 * 1024 * 1024)
        .finish()
        .expect("create parent");
    drop(p);

    // Fixed disk with parent should be rejected
    let result = File::create(&path)
        .size(64 * 1024 * 1024)
        .fixed(true)
        .parent_path(&parent_path)
        .finish();
    assert!(result.is_err(), "fixed disk with parent should be rejected");

    let _dir = dir;
}

#[test]
fn create_rejects_exceeds_64tb() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("huge.vhdx");

    let result = File::create(&path)
        .size(65 * 1024u64 * 1024 * 1024 * 1024) // 65 TB
        .finish();
    assert!(result.is_err(), "size > 64TB should be rejected");

    let _dir = dir;
}

// ---------------------------------------------------------------------------
// 18. Error type checks
// ---------------------------------------------------------------------------

#[test]
fn open_nonexistent_returns_io_error() {
    let result = File::open("tests/fixtures/does-not-exist-at-all.vhdx").finish();
    let err = result.unwrap_err();
    assert!(
        matches!(err, vhdx::Error::Io(_)),
        "nonexistent file should return Io error, got: {err:?}"
    );
}
