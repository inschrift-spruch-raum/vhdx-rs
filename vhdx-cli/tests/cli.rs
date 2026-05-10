use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

/// Compile-time path to test VHDX files in workspace root.
const TEST_VOID: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../misc/test-void.vhdx");
const TEST_FS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../misc/test-fs.vhdx");

/// Assert the test files exist (fail early with a clear message).
fn init() {
    assert!(
        Path::new(TEST_VOID).exists(),
        "test file not found: {TEST_VOID}"
    );
    assert!(
        Path::new(TEST_FS).exists(),
        "test file not found: {TEST_FS}"
    );
}

// ---------------------------------------------------------------------------
// 1. Info command on test files
// ---------------------------------------------------------------------------

#[test]
fn info_command_on_test_void() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("info").arg(TEST_VOID);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("vhdxfile"));
}

#[test]
fn info_command_on_test_fs() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("info").arg(TEST_FS);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("vhdxfile"));
}

#[test]
fn info_command_json_format() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("info").arg(TEST_VOID).arg("--format").arg("json");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"signature\": \"vhdxfile\""));
}

// ---------------------------------------------------------------------------
// 2. Check command on valid files
// ---------------------------------------------------------------------------

#[test]
fn check_command_on_temp_valid_vhdx() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg(path.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

/// test-fs.vhdx has a known BAT entry alignment issue (offset 4 MB not aligned
/// to 32 MB block size). The `check` command now correctly detects this and exits
/// with a non-zero exit code.
#[test]
fn check_command_on_test_fs_detects_alignment_issue() {
    init();
    // test-fs.vhdx has pending log entries; use --log-replay to replay them.
    // The file has a BAT offset alignment violation that causes a non-zero exit code.
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg(TEST_FS).arg("--log-replay");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("BAT_FILE_OFFSET_UNALIGNED"));
}

/// check --log-replay on a valid file (policy should be Auto, still succeeds).
#[test]
fn check_command_with_log_replay_on_temp_vhdx() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check")
        .arg(path.to_str().unwrap())
        .arg("--log-replay");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

// ---------------------------------------------------------------------------
// 3. Error cases
// ---------------------------------------------------------------------------

#[test]
fn info_command_missing_file_returns_nonzero() {
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("info").arg("__nonexistent_file__.vhdx");
    cmd.assert().failure();
}

#[test]
fn check_command_missing_file_returns_nonzero() {
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg("__nonexistent_file__.vhdx");
    cmd.assert().failure();
}

#[test]
fn check_command_invalid_file_returns_nonzero() {
    // Create a file that is too small to have a valid VHDX signature.
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad.vhdx");
    std::fs::write(&bad, b"not a valid VHDX").expect("write");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg(bad.to_str().unwrap());
    cmd.assert().failure();
    // TempDir is cleaned up on drop when dir goes out of scope
    let _dir = dir;
}

#[test]
fn info_command_invalid_file_returns_nonzero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad2.vhdx");
    std::fs::write(&bad, b"garbage data not vhdx").expect("write");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("info").arg(bad.to_str().unwrap());
    cmd.assert().failure();
    // TempDir is cleaned up on drop when dir goes out of scope
    let _dir = dir;
}

// ---------------------------------------------------------------------------
// 4. Sections command
// ---------------------------------------------------------------------------

#[test]
fn sections_header_command_on_test_void() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("sections").arg(TEST_VOID).arg("header");
    cmd.assert().success().stdout(
        predicate::str::contains("vhdx").and(predicate::str::contains("File Type Identifier")),
    );
}

#[test]
fn sections_bat_command_on_test_void() {
    init();
    // test-void.vhdx has block_size=32MB, so BAT can be displayed.
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("sections").arg(TEST_VOID).arg("bat");
    cmd.assert().success().stdout(
        predicate::str::contains("Block Allocation Table")
            .and(predicate::str::contains("Total Entries")),
    );
}

#[test]
fn sections_metadata_command_on_test_void() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("sections").arg(TEST_VOID).arg("metadata");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Metadata"));
}

#[test]
fn sections_log_command_on_test_void() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("sections").arg(TEST_VOID).arg("log");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Log"));
}

// ---------------------------------------------------------------------------
// 5. Diff command error handling (non-differencing disk)
// ---------------------------------------------------------------------------

#[test]
fn diff_parent_on_non_differencing_disk_is_error() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("diff").arg(TEST_VOID).arg("parent");
    cmd.assert().failure();
}

#[test]
fn diff_chain_on_non_differencing_disk_is_error() {
    init();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("diff").arg(TEST_VOID).arg("chain");
    cmd.assert().failure();
}

// ---------------------------------------------------------------------------
// 6. CLI parse tests (structural)
// ---------------------------------------------------------------------------

#[test]
fn cli_no_subcommand_shows_error() {
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.assert().failure();
}

// ---------------------------------------------------------------------------
// 7. --strict flag variants (uses a temp-created valid VHDX)
// ---------------------------------------------------------------------------

/// Create a temporary valid VHDX and return its path (caller keeps TempDir).
fn create_temp_valid_vhdx() -> (std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("valid.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("64MB")
        .assert()
        .success();

    (path, dir)
}

#[test]
fn check_strict_plain_flag() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg(path.to_str().unwrap()).arg("--strict");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

#[test]
fn check_strict_explicit_true() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check")
        .arg(path.to_str().unwrap())
        .arg("--strict")
        .arg("true");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

#[test]
fn check_strict_false_flag() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check")
        .arg(path.to_str().unwrap())
        .arg("--strict")
        .arg("false");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

#[test]
fn check_default_no_strict_flag() {
    let (path, _dir) = create_temp_valid_vhdx();
    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("check").arg(path.to_str().unwrap());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}

// ---------------------------------------------------------------------------
// 7. Create command
// ---------------------------------------------------------------------------

#[test]
fn create_command_dynamic_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("created.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("64MB");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    // Verify the file can be inspected
    let mut info = Command::cargo_bin("vhdx-tool").unwrap();
    info.arg("info").arg(path.to_str().unwrap());
    info.assert()
        .success()
        .stdout(predicate::str::contains("vhdxfile"));

    // Verify check passes
    let mut check = Command::cargo_bin("vhdx-tool").unwrap();
    check.arg("check").arg(path.to_str().unwrap());
    check
        .assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));

    let _dir = dir;
}

#[test]
fn create_command_fixed_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fixed_cli.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("64MB")
        .arg("--type")
        .arg("fixed");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fixed"));

    let _dir = dir;
}

#[test]
fn create_command_with_custom_block_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("custom_block.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("128MB")
        .arg("--block-size")
        .arg("16777216"); // 16 MB
    cmd.assert().success();

    let _dir = dir;
}

#[test]
fn create_command_rejects_zero_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("zero.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("0");
    cmd.assert().failure();

    let _dir = dir;
}

#[test]
fn create_command_rejects_invalid_block_size() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad_block.vhdx");

    let mut cmd = Command::cargo_bin("vhdx-tool").unwrap();
    cmd.arg("create")
        .arg(path.to_str().unwrap())
        .arg("--size")
        .arg("64MB")
        .arg("--block-size")
        .arg("500000"); // below 1MB
    cmd.assert().failure();

    let _dir = dir;
}
