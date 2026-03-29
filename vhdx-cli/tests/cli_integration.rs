use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get a `Command` that invokes the `vhdx-tool` binary built by cargo.
fn vhdx_tool() -> Command {
    Command::cargo_bin("vhdx-tool").unwrap()
}

/// Create a temp directory containing a small fixed VHDX file and return the
/// temp dir (dropping it would delete the directory & file).
fn create_fixed_vhdx() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.vhdx");
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success();
    dir
}

/// Create a temp directory containing a small dynamic VHDX file.
fn create_dynamic_vhdx() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhdx");
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();
    dir
}

/// Resolve the path to a test fixture shipped in `misc/`.
/// Returns `None` when the fixture does not exist.
fn fixture_path(relative: &str) -> Option<String> {
    // When running via `cargo test`, CWD is the package root (vhdx-cli/).
    let p = Path::new(relative);
    if p.exists() {
        Some(relative.to_string())
    } else {
        None
    }
}

// ===========================================================================
// CREATE tests
// ===========================================================================

#[test]
fn create_fixed_disk_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixed.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Fixed"));
}

#[test]
fn create_dynamic_disk_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Dynamic"));
}

#[test]
fn create_with_explicit_block_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocked.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--block-size",
            "1M",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("1.00 MB"));
}

#[test]
fn create_invalid_size_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.vhdx");

    vhdx_tool()
        .args(["create", path.to_str().unwrap(), "--size", "abc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid size format"));
}

#[test]
fn create_missing_size_argument_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.vhdx");

    // clap should reject the invocation because --size is required
    vhdx_tool()
        .args(["create", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--size"));
}

#[test]
fn create_file_already_exists_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exists.vhdx");

    // First creation succeeds
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success();

    // Second creation at same path should fail
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1M",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error creating VHDX file"));
}

// ===========================================================================
// INFO tests
// ===========================================================================

#[test]
fn info_text_format_shows_fields() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Virtual Size"))
        .stdout(predicate::str::contains("Block Size"))
        .stdout(predicate::str::contains("Disk Type"));
}

#[test]
fn info_json_format() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["info", path.to_str().unwrap(), "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"virtual_size\""))
        .stdout(predicate::str::contains("\"block_size\""))
        .stdout(predicate::str::contains("\"is_fixed\""))
        .stdout(predicate::str::contains("\"has_parent\""));
}

#[test]
fn info_nonexistent_file_fails() {
    vhdx_tool()
        .args(["info", "/nonexistent/path/test.vhdx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

#[test]
fn info_shows_fixed_type() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Disk Type: Fixed"));
}

#[test]
fn info_shows_dynamic_type() {
    let dir = create_dynamic_vhdx();
    let path = dir.path().join("dynamic.vhdx");

    vhdx_tool()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Disk Type: Dynamic"));
}

// ===========================================================================
// CHECK tests
// ===========================================================================

#[test]
fn check_valid_file_success() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "File check completed successfully",
        ));
}

#[test]
fn check_nonexistent_file_fails() {
    vhdx_tool()
        .args(["check", "/nonexistent/path/test.vhdx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error checking VHDX file"));
}

#[test]
fn check_log_replay_flag() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--log-replay"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Log replay requested"));
}

// ===========================================================================
// SECTIONS tests
// ===========================================================================

#[test]
fn sections_header_shows_header_section() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "header"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Header Section"));
}

#[test]
fn sections_bat_shows_bat_section() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "bat"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BAT Section"))
        .stdout(predicate::str::contains("Total BAT Entries"));
}

#[test]
fn sections_metadata_shows_metadata_section() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "metadata"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Metadata Section"));
}

#[test]
fn sections_log_shows_log_section() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Log Section"));
}

#[test]
fn sections_nonexistent_file_fails() {
    vhdx_tool()
        .args(["sections", "/nonexistent/path/test.vhdx", "header"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

// ===========================================================================
// DIFF tests
// ===========================================================================

#[test]
fn diff_parent_on_non_differencing_disk() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["diff", path.to_str().unwrap(), "parent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not a differencing disk"));
}

#[test]
fn diff_chain_on_non_differencing_disk() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["diff", path.to_str().unwrap(), "chain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("base disk"));
}

#[test]
fn diff_nonexistent_file_fails() {
    vhdx_tool()
        .args(["diff", "/nonexistent/path/test.vhdx", "chain"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

// ===========================================================================
// REPAIR tests
// ===========================================================================

#[test]
fn repair_dry_run_on_valid_file() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["repair", path.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"));
}

#[test]
fn repair_nonexistent_file_fails() {
    vhdx_tool()
        .args(["repair", "/nonexistent/path/test.vhdx", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

// ===========================================================================
// HELP / VERSION tests
// ===========================================================================

#[test]
fn help_flag_shows_vhdx() {
    vhdx_tool()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("VHDX"));
}

#[test]
fn version_flag() {
    vhdx_tool().arg("--version").assert().success();
}

// ===========================================================================
// Fixture tests (guarded — only run when the fixture files exist)
// ===========================================================================

#[test]
fn info_on_test_void_vhdx() {
    let Some(p) = fixture_path("../../misc/test-void.vhdx") else {
        return;
    };

    vhdx_tool()
        .args(["info", &p])
        .assert()
        .success()
        .stdout(predicate::str::contains("Virtual Size"));
}

#[test]
fn check_on_test_void_vhdx() {
    let Some(p) = fixture_path("../../misc/test-void.vhdx") else {
        return;
    };

    vhdx_tool()
        .args(["check", &p])
        .assert()
        .success()
        .stdout(predicate::str::contains("File check completed"));
}
