//! `check` 子命令实现
//!
//! 检查 VHDX 文件的完整性。通过 [`SpecValidator`] 执行规范级别的
//! 结构校验，包括文件头、区域表、元数据、BAT 和日志区域。
//!
//! 标志语义：
//! - **默认**：以只读不回放策略打开，检查原始文件状态
//! - **`--log-replay`**：以内存回放策略打开，数据读取反映回放后状态
//! - **`--repair`**：检测修复需求并给出明确指引，不执行实际修复

use std::path::Path;

/// 记录单次校验结果
struct CheckResult {
    /// 检查项名称
    name: &'static str,
    /// 校验结果：`Ok(())` 通过，`Err(e)` 失败
    result: vhdx_rs::Result<()>,
}

/// 执行 `check` 子命令
///
/// 打开 VHDX 文件并依次执行以下规范校验：
/// 1. Header（文件头签名、版本、校验和）
/// 2. Region Table（区域表签名、条目完整性）
/// 3. Metadata（元数据可读性）
/// 4. Required Metadata Items（必需元数据项存在性）
/// 5. BAT（块分配表条目状态一致性）
/// 6. Log（日志条目结构与序列一致性）
///
/// 校验通过时输出通过计数；校验失败时输出失败计数和错误摘要，
/// 并以非零退出码退出。
///
/// # 参数
/// - `file`: 要检查的 VHDX 文件路径
/// - `repair`: 是否检测修复需求（仅输出指引，不实际修复）
/// - `log_replay`: 是否以内存回放方式检查
pub fn cmd_check(file: &Path, repair: bool, log_replay: bool) {
    use vhdx_rs::{File, LogReplayPolicy};

    println!("Checking VHDX file: {}", file.display());

    // 根据标志选择日志回放策略：
    // - 默认使用 ReadOnlyNoReplay 以检查原始文件状态
    // - --log-replay 使用 InMemoryOnReadOnly 以在内存中应用回放
    let policy = if log_replay {
        LogReplayPolicy::InMemoryOnReadOnly
    } else {
        LogReplayPolicy::ReadOnlyNoReplay
    };

    match File::open(file).log_replay(policy).finish() {
        Ok(vhdx_file) => {
            // 直接检查原始日志结构是否存在待回放条目
            let has_pending_log = vhdx_file
                .sections()
                .log()
                .is_ok_and(|l| l.is_replay_required());

            // 非回放模式下报告待处理日志警告
            if has_pending_log && !log_replay {
                println!("⚠ File has pending log entries from an interrupted write.");
                println!("  Use --log-replay to check with replay applied, or");
                println!("  run 'vhdx-tool repair <file>' to fix.");
                println!();
            }

            // 依次执行规范校验项
            let validator = vhdx_file.validator();
            let mut results = vec![
                CheckResult {
                    name: "Header",
                    result: validator.validate_header(),
                },
                CheckResult {
                    name: "Region Table",
                    result: validator.validate_region_table(),
                },
                CheckResult {
                    name: "Metadata",
                    result: validator.validate_metadata(),
                },
                CheckResult {
                    name: "Required Metadata Items",
                    result: validator.validate_required_metadata_items(),
                },
                CheckResult {
                    name: "BAT",
                    result: validator.validate_bat(),
                },
                CheckResult {
                    name: "Log",
                    result: validator.validate_log(),
                },
            ];

            // 差分盘额外校验 Parent Locator；非差分盘不计入检查项。
            let is_diff = vhdx_file.sections().metadata().is_ok_and(|m| {
                m.items().file_parameters().is_some_and(|fp| fp.has_parent())
            });
            if is_diff {
                results.push(CheckResult {
                    name: "Parent Locator",
                    result: validator.validate_parent_locator(),
                });
            }

            let mut passed = 0u32;
            let mut failed = 0u32;
            let mut first_error: Option<String> = None;

            for item in &results {
                match &item.result {
                    Ok(()) => {
                        println!("✓ {}", item.name);
                        passed += 1;
                    }
                    Err(e) => {
                        println!("✗ {}: {e}", item.name);
                        failed += 1;
                        if first_error.is_none() {
                            first_error = Some(format!("{}: {e}", item.name));
                        }
                    }
                }
            }

            // --log-replay: 报告日志回放状态
            if log_replay {
                if has_pending_log {
                    println!(
                        "\n⚠ Log replay applied in memory; pending entries detected in raw log"
                    );
                } else {
                    println!("\n✓ No pending log entries");
                }
            }

            // --repair: 检测修复需求并给出指引（不执行写修复）
            if repair {
                if has_pending_log {
                    println!("\n⚠ File has pending log entries requiring repair");
                    println!("  To fix, run: vhdx-tool repair {}", file.display());
                } else {
                    println!("\n✓ No repair needed");
                }
            }

            // 输出结构化校验结果摘要
            println!("\nCheck summary: {passed} passed, {failed} failed");

            if failed > 0 {
                // 输出首个关键错误摘要
                if let Some(err) = first_error {
                    println!("First error: {err}");
                }
                std::process::exit(1);
            }

            // --repair 且存在待修复项：以非零退出码退出
            if repair && has_pending_log {
                std::process::exit(1);
            }

            println!("File check completed successfully");
        }
        Err(e) => {
            eprintln!("✗ Error checking VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
