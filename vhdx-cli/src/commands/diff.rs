//! `diff` 子命令实现
//!
//! 差分磁盘相关操作，包括：
//! - **parent**：查看差分磁盘的父磁盘定位器信息
//! - **chain**：查看磁盘链（从当前磁盘到基础磁盘的层次关系）

use std::path::Path;

use crate::cli::DiffCommand;

/// 执行 `diff` 子命令
///
/// 打开 VHDX 文件并执行指定的差分磁盘操作。
/// 如果文件存在未完成的日志条目，会输出警告信息。
///
/// # 参数
/// - `file`: VHDX 文件路径
/// - `command`: 差分操作类型（查看父磁盘或磁盘链）
pub fn cmd_diff(file: &Path, command: &DiffCommand) {
    use vhdx_rs::File;

    match File::open(file).finish() {
        Ok(vhdx_file) => {
            // 检查是否存在未完成的日志条目
            if vhdx_file
                .sections()
                .log()
                .is_ok_and(|l| l.is_replay_required())
            {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match command {
                // 查看父磁盘定位器信息
                DiffCommand::Parent => {
                    if vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
                        .unwrap_or(false)
                    {
                        if let Ok(metadata) = vhdx_file.sections().metadata()
                            && let Some(locator) = metadata.items().parent_locator()
                        {
                            println!("Parent Locator Entries:");
                            for (i, entry) in locator.entries().iter().enumerate() {
                                if let Some(key) = entry.key(locator.key_value_data())
                                    && let Some(value) = entry.value(locator.key_value_data())
                                {
                                    println!("  [{i}] {key}: {value}");
                                }
                            }
                        }
                    } else {
                        println!("This is not a differencing disk (no parent)");
                    }
                }
                // 查看磁盘链
                DiffCommand::Chain => {
                    println!("Disk Chain:");
                    println!("  -> {}", file.display());
                    if vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
                        .unwrap_or(false)
                    {
                        println!("     (has parent - chain traversal not yet implemented)");
                    } else {
                        println!("     (base disk)");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
