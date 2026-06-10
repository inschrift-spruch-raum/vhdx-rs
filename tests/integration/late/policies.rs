use super::*;

// 15. Policy conflict tests: write-incompatible policies
// ---------------------------------------------------------------------------

#[test]
fn in_memory_on_read_only_with_write_is_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy_conflict_imr.vhdx");

    let f = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create vhdx");
    drop(f);

    let result = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
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

    let f = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(8 * 1024 * 1024)
        .finish()
        .expect("create vhdx");
    drop(f);

    let result = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
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
    let result = Medium::open(std::fs::File::open(&path).expect("open VHDX medium")).finish();
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
    let result = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(512 * 1024) // 512 KB, below minimum
        .finish();
    assert!(result.is_err(), "block size below 1MB should be rejected");

    let result = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(300 * 1024 * 1024) // 300 MB, above maximum
        .finish();
    assert!(result.is_err(), "block size above 256MB should be rejected");

    // Block size must be a power of 2
    let result = create_medium(&path)
        .size(64 * 1024 * 1024)
        .block_size(10 * 1024 * 1024) // 10 MB, not a power of 2
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
    let result = create_medium(&path)
        .size(64 * 1024 * 1024)
        .logical_sector_size(1024)
        .finish();
    assert!(
        result.is_err(),
        "logical sector size 1024 should be rejected"
    );

    // Physical sector size must be 512 or 4096
    let result = create_medium(&path)
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
    let p = create_medium(&parent_path)
        .size(64 * 1024 * 1024)
        .finish()
        .expect("create parent");
    drop(p);

    // Fixed disk with parent should be rejected
    let mut parent = open_medium(&parent_path);
    let result = create_medium(&path)
        .size(64 * 1024 * 1024)
        .fixed(true)
        .parent(&mut parent, &parent_path)
        .expect("configure caller-owned parent medium")
        .finish();
    assert!(result.is_err(), "fixed disk with parent should be rejected");

    let _dir = dir;
}

#[test]
fn create_rejects_exceeds_64tb() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("huge.vhdx");

    let result = create_medium(&path)
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
    let err = std::fs::File::open("tests/fixtures/does-not-exist-at-all.vhdx")
        .map(Medium::open)
        .and_then(|open| open.finish().map_err(std::io::Error::from))
        .unwrap_err();
    assert!(
        err.kind() == std::io::ErrorKind::NotFound,
        "nonexistent Medium should return Io error, got: {err:?}"
    );
}
