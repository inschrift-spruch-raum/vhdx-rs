//! `repair` 子命令实现
//!
//! 修复 VHDX 文件，主要通过重放日志来恢复因中断写入
//! 而导致的不一致状态。支持 `--dry-run` 模式以仅检查
//! 而不实际修改文件。
use std::path::Path;

/// 执行 `repair` 子命令
///
/// 对 VHDX 文件进行修复操作。在 dry-run 模式下只检查
/// 是否需要修复而不实际修改文件。正常模式下会以读写方式
/// 打开文件并重放日志条目。
///
/// # 参数
/// - `file`: 要修复的 VHDX 文件路径
/// - `dry_run`: 为 true 时仅检查，不实际修复
pub fn cmd_repair(file: &Path, dry_run: bool) {
    use vhdx_rs::{Error, File, LogReplayPolicy};

    println!("Repairing VHDX file: {}", file.display());

    // dry-run 模式：只检查不修改，使用只读内存回放确保不落盘
    if dry_run {
        println!("Dry run mode - no changes will be made");
        match File::open(file)
            .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
            .finish()
        {
            Ok(vhdx_file) => {
                if vhdx_file
                    .sections()
                    .log()
                    .is_ok_and(|l| l.is_replay_required())
                {
                    println!("\u{2713} File has pending log entries that would be replayed");
                } else {
                    println!("\u{2713} File does not require repair");
                }
                return;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    // 正常修复模式：以读写方式打开，显式使用 Require 策略
    match File::open(file)
        .write()
        .log_replay(LogReplayPolicy::Require)
        .finish()
    {
        Ok(_) => {
            println!("\u{2713} File repaired successfully");
            println!("\u{2713} Log entries replayed");
        }
        // 日志重放失败的特定错误
        Err(Error::LogReplayRequired) => {
            eprintln!("Error: Unable to replay log entries");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error repairing VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
