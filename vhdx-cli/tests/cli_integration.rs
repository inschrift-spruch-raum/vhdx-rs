//! CLI 工具集成测试 — 验证命令行界面的各项功能

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

/// 最小 CRC-32C（Castagnoli）实现，用于测试中计算校验和。
fn crc32c(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0x82F6_3B78
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

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

/// 创建一组基础盘 + 差分盘（base.vhdx -> child.vhdx），返回临时目录和子盘路径。
fn create_differencing_vhdx_pair() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.vhdx");

    vhdx_tool()
        .args([
            "create",
            base_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();

    let child_path = dir.path().join("child.vhdx");
    vhdx_tool()
        .args([
            "create",
            child_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            base_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    (dir, child_path)
}

/// 将差分盘 Parent Locator 篡改为无条目内容（测试专用）。
fn inject_invalid_parent_locator_for_cli(path: &std::path::Path) {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};

    // 当前创建布局中 metadata 起始偏移固定为 2 * 1MiB。
    const METADATA_OFFSET: u64 = 2 * 1024 * 1024;
    // 第 6 个 metadata entry 为 Parent Locator（索引 5）。
    const PARENT_LOCATOR_ENTRY_OFFSET: u64 = METADATA_OFFSET + 32 + 5 * 32;
    const PARENT_LOCATOR_LENGTH_FIELD_OFFSET: u64 = PARENT_LOCATOR_ENTRY_OFFSET + 20;
    const PARENT_LOCATOR_DATA_OFFSET: u64 = METADATA_OFFSET + 65_576;

    // 仅写入 20 字节 locator header，entry_count=0，触发 parent_linkage 缺失。
    let invalid_locator = vec![0u8; 20];

    let mut raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("Failed to open child file for parent locator injection");

    raw.seek(SeekFrom::Start(PARENT_LOCATOR_LENGTH_FIELD_OFFSET))
        .expect("Failed to seek parent locator length field");
    raw.write_all(
        &u32::try_from(invalid_locator.len())
            .expect("parent locator size overflow")
            .to_le_bytes(),
    )
    .expect("Failed to write parent locator length");

    raw.seek(SeekFrom::Start(PARENT_LOCATOR_DATA_OFFSET))
        .expect("Failed to seek parent locator data offset");
    raw.write_all(&invalid_locator)
        .expect("Failed to write parent locator data");
    raw.flush()
        .expect("Failed to flush injected parent locator");
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

/// 测试差分盘 check 输出应包含 Parent Locator 检查项。
#[test]
fn check_differencing_disk_includes_parent_locator_item() {
    let (_dir, child_path) = create_differencing_vhdx_pair();

    vhdx_tool()
        .args(["check", child_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ Parent Locator"))
        .stdout(predicate::str::contains("0 failed"));
}

/// 测试 CLI check 在差分盘上输出 Parent Locator 检查项。
#[test]
fn cli_check_differencing_parent_locator_output() {
    let (_dir, child_path) = create_differencing_vhdx_pair();

    vhdx_tool()
        .args(["check", child_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Parent Locator"))
        .stdout(predicate::str::contains("0 failed"));
}

/// 测试差分盘 Parent Locator 无效时，check 应报告 Parent Locator 失败。
///
/// inject_invalid_parent_locator_for_cli 将 locator_type 清零，触发 locator_type mismatch。
#[test]
fn check_invalid_parent_locator_reports_failure() {
    let (_dir, child_path) = create_differencing_vhdx_pair();
    inject_invalid_parent_locator_for_cli(&child_path);

    vhdx_tool()
        .args(["check", child_path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("✗ Parent Locator"))
        .stdout(predicate::str::contains("locator_type"));
}

/// 测试 CLI check 在 Parent Locator 无效时返回失败并报告 locator_type 错误。
#[test]
fn cli_check_invalid_parent_locator_fails() {
    let (_dir, child_path) = create_differencing_vhdx_pair();
    inject_invalid_parent_locator_for_cli(&child_path);

    vhdx_tool()
        .args(["check", child_path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Parent Locator"))
        .stdout(predicate::str::contains("locator_type"));
}

/// 测试非差分盘 check 不应误报 Parent Locator 失败。
#[test]
fn check_non_differencing_disk_no_parent_locator_false_failure() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 failed"))
        .stdout(predicate::str::contains("✗ Parent Locator").not());
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

/// 测试 check 命令带 --log-replay 标志：验证输出报告日志回放状态。
#[test]
fn check_log_replay_flag() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    // --log-replay 以 InMemoryOnReadOnly 策略打开文件，
    // 干净文件应输出 "No pending log entries"
    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--log-replay"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No pending log entries"));
}

/// 测试 check 命令带 --repair 标志对干净文件：验证输出 "No repair needed"。
#[test]
fn check_repair_flag_on_clean_file() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    // --repair 不执行写修复，仅检测并报告修复需求
    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--repair"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No repair needed"));
}

/// 测试 check 命令同时使用 --log-replay 和 --repair 标志：验证两个标志共存时行为正确。
#[test]
fn check_log_replay_with_repair_flag() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    // 同时启用两个标志：内存回放 + 修复检测
    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--log-replay", "--repair"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No pending log entries"))
        .stdout(predicate::str::contains("No repair needed"));
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

/// 测试 sections 命令显示日志区域内容：验证包含标题和条目总数。
#[test]
fn sections_log_shows_log_section() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Log Section"))
        .stdout(predicate::str::contains("Total Log Entries"));
}

/// 测试 sections log 对干净文件（无日志条目）输出友好提示。
#[test]
fn sections_log_clean_file_shows_no_entries() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Total Log Entries: 0"))
        .stdout(predicate::str::contains(
            "No log entries found. File is clean.",
        ));
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
        .stdout(predicate::str::contains("Disk Chain"))
        .stdout(predicate::str::contains("base disk"));
}

/// 测试 diff chain 子命令对差分磁盘链路的 happy path 遍历。
///
/// 创建 base -> child 两层链路，验证输出包含两个文件路径。
#[test]
fn diff_chain_happy_path() {
    // 步骤 1：创建基础磁盘（dynamic）
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.vhdx");

    vhdx_tool()
        .args([
            "create",
            base_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();

    // 步骤 2：基于基础磁盘创建差分磁盘
    let child_path = dir.path().join("child.vhdx");
    vhdx_tool()
        .args([
            "create",
            child_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            base_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // 步骤 3：对差分磁盘执行 chain 命令
    let assert_result = vhdx_tool()
        .args(["diff", child_path.to_str().unwrap(), "chain"])
        .assert()
        .success();

    // 验证输出包含链路标题、子盘路径和基础盘路径
    assert_result
        .stdout(predicate::str::contains("Disk Chain"))
        .stdout(predicate::str::contains("child.vhdx"))
        .stdout(predicate::str::contains("base.vhdx"))
        .stdout(predicate::str::contains("base disk"));
}

/// 测试 diff chain 子命令检测缺失父盘并返回非零退出码。
///
/// 创建差分磁盘后删除父盘，验证 chain 命令输出错误并 exit(1)。
#[test]
fn diff_chain_missing_parent_fails() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.vhdx");

    // 创建基础磁盘
    vhdx_tool()
        .args([
            "create",
            base_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();

    // 创建差分磁盘
    let child_path = dir.path().join("child.vhdx");
    vhdx_tool()
        .args([
            "create",
            child_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            base_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // 删除父盘以模拟缺失场景
    std::fs::remove_file(&base_path).unwrap();

    // 执行 chain 命令应失败并报告父盘缺失
    vhdx_tool()
        .args(["diff", child_path.to_str().unwrap(), "chain"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Parent disk not found"));
}

/// 测试 diff chain 子命令对三层链路（grandchild -> child -> base）的完整遍历。
#[test]
fn diff_chain_three_level_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.vhdx");

    // 创建基础磁盘
    vhdx_tool()
        .args([
            "create",
            base_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();

    // 创建第一层差分磁盘
    let child_path = dir.path().join("child.vhdx");
    vhdx_tool()
        .args([
            "create",
            child_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            base_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // 创建第二层差分磁盘
    let grandchild_path = dir.path().join("grandchild.vhdx");
    vhdx_tool()
        .args([
            "create",
            grandchild_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            child_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // 验证三层链路输出
    let assert_result = vhdx_tool()
        .args(["diff", grandchild_path.to_str().unwrap(), "chain"])
        .assert()
        .success();

    assert_result
        .stdout(predicate::str::contains("Disk Chain"))
        .stdout(predicate::str::contains("grandchild.vhdx"))
        .stdout(predicate::str::contains("child.vhdx"))
        .stdout(predicate::str::contains("base.vhdx"))
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

/// 测试 check 命令对有效文件输出结构化校验摘要：验证包含通过计数和校验项。
#[test]
fn check_valid_file_shows_summary() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    vhdx_tool()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"))
        .stdout(predicate::str::contains("0 failed"))
        .stdout(predicate::str::contains("✓ Header"))
        .stdout(predicate::str::contains("✓ BAT"));
}

/// 测试 check 命令对损坏文件（非 VHDX 格式）应返回失败退出码和校验错误摘要。
#[test]
fn check_corrupted_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupted.vhdx");

    // 写入无效数据模拟损坏文件
    std::fs::write(&path, b"NOT_A_VHDX_FILE_CORRUPT_DATA").unwrap();

    // 损坏文件无法打开，应报告打开错误并以非零退出码退出
    vhdx_tool()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error checking VHDX file"));
}

/// 辅助函数：向已有 VHDX 文件注入 pending log entry 并设置 log_guid。
/// 使用 vhdx_rs 库 API 读取元数据，再通过原始 IO 写入日志条目和更新的 header。
fn inject_pending_log_for_cli(path: &std::path::Path) {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};

    const HEADER_SECTION_SIZE: u64 = 1024 * 1024;
    const LOG_ENTRY_HEADER_SIZE: usize = 64;
    const DESCRIPTOR_SIZE: usize = 32;
    const DATA_SECTOR_SIZE: usize = 4096;

    // 读取 header 以获取 log_offset 和 log_length
    let file = vhdx_rs::File::open(path)
        .finish()
        .expect("Failed to open file for log injection metadata");
    let header_ref = file
        .sections()
        .header()
        .expect("Failed to read header for log injection");
    let header = header_ref
        .header(0)
        .expect("No active header for log injection");

    let log_offset = header.log_offset();
    let log_length = usize::try_from(header.log_length()).expect("log_length overflow");

    // 构造最小可回放日志条目
    let target_file_offset = HEADER_SECTION_SIZE + 512;
    let entry_len = LOG_ENTRY_HEADER_SIZE + DESCRIPTOR_SIZE + DATA_SECTOR_SIZE;
    let mut entry = vec![0u8; entry_len];
    entry[0..4].copy_from_slice(b"loge");
    entry[8..12].copy_from_slice(&(u32::try_from(entry_len).unwrap()).to_le_bytes());
    entry[24..28].copy_from_slice(&1u32.to_le_bytes()); // descriptor_count = 1

    // Task 5: 条目 log_guid 必须与 active header.log_guid 一致。
    let log_guid = vhdx_rs::Guid::from_bytes([
        0xA1, 0xB2, 0xC3, 0xD4, 0x11, 0x22, 0x33, 0x44, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33,
        0x22,
    ]);
    entry[32..48].copy_from_slice(log_guid.as_bytes());

    let desc_off = LOG_ENTRY_HEADER_SIZE;
    entry[desc_off..desc_off + 4].copy_from_slice(b"desc");
    entry[desc_off + 16..desc_off + 24].copy_from_slice(&target_file_offset.to_le_bytes());
    entry[desc_off + 24..desc_off + 32].copy_from_slice(&0u64.to_le_bytes()); // sequence = 0

    let sector_off = LOG_ENTRY_HEADER_SIZE + DESCRIPTOR_SIZE;
    entry[sector_off..sector_off + 4].copy_from_slice(b"data");
    entry[sector_off + 4..sector_off + 8].copy_from_slice(&0u32.to_le_bytes()); // sequence_high = 0
    entry[sector_off + 8..sector_off + 8 + 13].copy_from_slice(b"CLI_LOG_ENTRY");
    entry[sector_off + 4092..sector_off + 4096].copy_from_slice(&0u32.to_le_bytes()); // sequence_low = 0

    // 生成合法 checksum，满足 Task 4 replay precheck。
    entry[4..8].fill(0);
    let checksum = crc32c(&entry);
    entry[4..8].copy_from_slice(&checksum.to_le_bytes());

    // 写入日志条目
    let mut raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("Failed to open file for log injection write");

    raw.seek(SeekFrom::Start(log_offset))
        .expect("Failed to seek log offset");
    raw.write_all(&entry).expect("Failed to write log entry");

    let remaining = log_length.saturating_sub(entry.len());
    if remaining > 0 {
        raw.write_all(&vec![0u8; remaining])
            .expect("Failed to clear log tail");
    }

    // 设置 header log_guid 为非空（表示有待处理日志）
    let updated_header = vhdx_rs::section::HeaderStructure::create(
        header.sequence_number(),
        header.file_write_guid(),
        header.data_write_guid(),
        log_guid,
        header.log_length(),
        header.log_offset(),
    );

    raw.seek(SeekFrom::Start(64 * 1024))
        .expect("Failed to seek header1");
    raw.write_all(&updated_header)
        .expect("Failed to write header1");
    raw.seek(SeekFrom::Start(128 * 1024))
        .expect("Failed to seek header2");
    raw.write_all(&updated_header)
        .expect("Failed to write header2");
    raw.flush().expect("Failed to flush injected log");
}

// ── T9: check --log-replay / sections log / diff chain 回归测试 ──

/// 测试 check --log-replay 对含 pending log 文件应报告 pending entries。
#[test]
fn check_log_replay_with_pending_log_reports_entries() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    // 注入 pending log entry
    inject_pending_log_for_cli(&path);

    // check --log-replay 应输出 pending entries 提示
    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--log-replay"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending"));
}

/// 测试 check（不带 --log-replay）对含 pending log 文件应输出修复指引。
#[test]
fn check_without_replay_shows_repair_guidance_on_pending_log() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    inject_pending_log_for_cli(&path);

    vhdx_tool()
        .args(["check", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending log entries"))
        .stdout(predicate::str::contains("vhdx-tool repair"));
}

/// 测试 check --repair 对含 pending log 文件应报告需要修复并以非零退出码退出。
#[test]
fn check_repair_on_pending_log_file_exits_nonzero() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    inject_pending_log_for_cli(&path);

    vhdx_tool()
        .args(["check", path.to_str().unwrap(), "--repair"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "pending log entries requiring repair",
        ));
}

/// 测试 sections log 对含 pending log 的文件应显示日志条目详情。
#[test]
fn sections_log_shows_pending_entry_details() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    inject_pending_log_for_cli(&path);

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Log Section"))
        .stdout(predicate::str::contains("Total Log Entries: 1"))
        .stdout(predicate::str::contains("Entry 0:"))
        .stdout(predicate::str::contains("Signature: loge"))
        .stdout(predicate::str::contains("Descriptor Count: 1"))
        .stdout(predicate::str::contains("Data Descriptors: 1"));
}

/// 测试 diff parent 对差分磁盘应显示父磁盘定位器条目。
#[test]
fn diff_parent_on_differencing_disk_shows_locator_entries() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.vhdx");

    // 创建基础磁盘
    vhdx_tool()
        .args([
            "create",
            base_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "dynamic",
        ])
        .assert()
        .success();

    // 创建差分磁盘
    let child_path = dir.path().join("child.vhdx");
    vhdx_tool()
        .args([
            "create",
            child_path.to_str().unwrap(),
            "--size",
            "1MiB",
            "--disk-type",
            "differencing",
            "--parent",
            base_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // diff parent 应显示 Parent Locator Entries
    let assert_result = vhdx_tool()
        .args(["diff", child_path.to_str().unwrap(), "parent"])
        .assert()
        .success();

    assert_result
        .stdout(predicate::str::contains("Parent Locator Entries:"))
        .stdout(predicate::str::contains("parent_linkage"))
        .stdout(predicate::str::contains("relative_path"));
}

/// 测试 sections log 对含 pending log 文件应显示 stderr 上的 pending log 警告。
#[test]
fn sections_log_warns_about_pending_log_on_stderr() {
    let dir = create_fixed_vhdx();
    let path = dir.path().join("test.vhdx");

    inject_pending_log_for_cli(&path);

    vhdx_tool()
        .args(["sections", path.to_str().unwrap(), "header"])
        .assert()
        .success()
        .stderr(predicate::str::contains("pending log entries"))
        .stderr(predicate::str::contains("vhdx-tool repair"));
}
