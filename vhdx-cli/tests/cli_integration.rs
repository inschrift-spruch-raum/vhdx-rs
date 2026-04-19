//! CLI 工具集成测试 — 验证命令行界面的各项功能

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

/// 构造一个指向 vhdx-tool 可执行文件的 Command 实例。
fn vhdx_tool() -> Command {
    Command::cargo_bin("vhdx-tool").unwrap()
}

/// 创建一个包含 1 MiB 固定类型 VHDX 文件的临时目录，用于后续测试。
fn create_fixed_vhdx() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.vhdx");
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success();
    dir
}

/// 创建一个包含 1 MiB 动态类型 VHDX 文件的临时目录，用于后续测试。
fn create_dynamic_vhdx() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhdx");
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();
    dir
}

/// 获取样例文件路径，若文件不存在则返回 None（测试将跳过）。
fn fixture_path(relative: &str) -> Option<String> {
    let p = Path::new(relative);
    if p.exists() {
        Some(relative.to_string())
    } else {
        None
    }
}

/// 测试通过 CLI 创建固定磁盘：验证输出包含创建成功信息和 "Fixed" 类型标识。
#[test]
fn create_fixed_disk_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixed.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Fixed"));
}

/// 测试通过 CLI 创建动态磁盘：验证输出包含创建成功信息和 "Dynamic" 类型标识。
#[test]
fn create_dynamic_disk_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Dynamic"));
}

/// 测试通过 --type 主路径创建固定磁盘：验证输出包含创建成功信息和 "Fixed" 类型标识。
#[test]
fn create_fixed_disk_via_type_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixed.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--type",
            "fixed",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Fixed"));
}

/// 测试通过 --type 主路径创建动态磁盘：验证输出包含创建成功信息和 "Dynamic" 类型标识。
#[test]
fn create_dynamic_disk_via_type_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Dynamic"));
}

/// 测试 --disk-type 兼容路径仍然有效：验证输出包含创建成功信息和 "Fixed" 类型标识。
#[test]
fn create_fixed_disk_via_compat_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("compat.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Fixed"));
}

/// 测试同时传入 --type 与 --disk-type 时 --type 优先：验证输出显示 --type 指定的类型。
#[test]
fn create_both_flags_type_takes_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("precedence.vhdx");

    // --type fixed --disk-type dynamic → 应使用 fixed（--type 优先）
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--type",
            "fixed",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Fixed"));
}

/// 测试仅传入 --type 省略磁盘类型时默认为 dynamic：验证输出显示 "Dynamic"。
#[test]
fn create_default_disk_type_is_dynamic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.vhdx");

    vhdx_tool()
        .args(["create", path.to_str().unwrap(), "--size", "1MiB"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("Dynamic"));
}

/// 测试创建时指定自定义块大小：验证输出中显示指定的块大小。
#[test]
fn create_with_explicit_block_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocked.vhdx");

    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--block-size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"))
        .stdout(predicate::str::contains("1.00 MiB"));
}

/// 测试使用无效的大小参数创建应失败：验证错误提示包含 "Invalid size"。
#[test]
fn create_invalid_size_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.vhdx");

    vhdx_tool()
        .args(["create", path.to_str().unwrap(), "--size", "abc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid size"));
}

/// 测试缺少 --size 参数创建应失败：验证错误提示包含 "--size"。
#[test]
fn create_missing_size_argument_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.vhdx");

    vhdx_tool()
        .args(["create", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--size"));
}

/// 测试在已存在文件上重复创建应失败：验证第二次创建返回错误。
#[test]
fn create_file_already_exists_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exists.vhdx");

    // 首次创建应成功
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success();

    // 第二次创建同一路径应失败
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error creating VHDX file"));
}

/// 测试 --force 标志允许覆盖已存在的文件：验证第二次创建成功。
#[test]
fn create_force_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("force.vhdx");

    // 首次创建应成功
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
        ])
        .assert()
        .success();

    // 使用 --force 覆盖应成功
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "fixed",
            "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created VHDX file"));
}

/// 测试 --force 不绕过差分磁盘的父磁盘校验：无 parent 时仍应失败。
#[test]
fn create_force_does_not_bypass_parent_validation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("diff_no_parent.vhdx");

    // --force + --disk-type differencing 但无 --parent，仍应失败
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Differencing disk requires --parent",
        ));
}

/// 测试 --force 不绕过父磁盘路径不存在的校验：指定不存在的 parent 仍应失败。
#[test]
fn create_force_does_not_bypass_missing_parent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("diff_bad_parent.vhdx");
    let fake_parent = dir.path().join("nonexistent.vhdx");

    // --force + --parent 指向不存在的文件，仍应失败
    vhdx_tool()
        .args([
            "create",
            path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            fake_parent.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error creating VHDX file"));
}

/// 测试 info 命令以文本格式输出：验证包含 Virtual Size、Block Size 和 Disk Type 字段。
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

/// 测试 info 命令以 JSON 格式输出：验证包含所有核心字段的键名。
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

/// 测试 info 命令对不存在的文件应报错。
#[test]
fn info_nonexistent_file_fails() {
    vhdx_tool()
        .args(["info", "/nonexistent/path/test.vhdx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

/// 测试 info 命令对固定磁盘显示 "Fixed" 类型。
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

/// 测试 info 命令对动态磁盘显示 "Dynamic" 类型。
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

/// 测试 check 命令对有效文件应返回成功并显示检查完成信息。
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

/// 测试 check 命令对不存在的文件应报错。
#[test]
fn check_nonexistent_file_fails() {
    vhdx_tool()
        .args(["check", "/nonexistent/path/test.vhdx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error checking VHDX file"));
}

/// 测试 check 命令带 --log-replay 标志：验证输出包含日志重放相关信息。
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

/// 测试 sections 命令显示头部区域内容。
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

/// 测试 sections 命令显示 BAT 区域内容及总条目数。
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

/// 测试 sections 命令显示元数据区域内容。
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

/// 测试 sections 命令显示日志区域内容。
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

/// 测试 sections 命令对不存在的文件应报错。
#[test]
fn sections_nonexistent_file_fails() {
    vhdx_tool()
        .args(["sections", "/nonexistent/path/test.vhdx", "header"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

/// 测试 diff parent 子命令对非差分磁盘应提示非差分类型。
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

/// 测试 diff chain 子命令对非差分磁盘应显示为基础磁盘。
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

/// 测试 diff 命令对不存在的文件应报错。
#[test]
fn diff_nonexistent_file_fails() {
    vhdx_tool()
        .args(["diff", "/nonexistent/path/test.vhdx", "chain"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error opening VHDX file"));
}

/// 测试 repair 命令的 --dry-run 模式：验证输出包含 "Dry run" 标识。
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

/// 测试 repair 命令对不存在的文件应报错。
#[test]
fn repair_nonexistent_file_fails() {
    vhdx_tool()
        .args(["repair", "/nonexistent/path/test.vhdx", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}

/// 测试 --help 标志：验证输出包含 "VHDX" 关键字。
#[test]
fn help_flag_shows_vhdx() {
    vhdx_tool()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("VHDX"));
}

/// 测试 --version 标志：验证命令执行成功。
#[test]
fn version_flag() {
    vhdx_tool().arg("--version").assert().success();
}

/// 测试 info 命令对 misc/test-void.vhdx 样本文件的处理：验证输出包含虚拟大小信息。
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

/// 测试 check 命令对 misc/test-void.vhdx 样本文件的处理：验证检查成功。
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
