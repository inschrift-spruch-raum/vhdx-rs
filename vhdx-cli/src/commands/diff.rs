//! `diff` 子命令实现
//!
//! 差分磁盘相关操作，包括：
//! - **parent**：查看差分磁盘的父磁盘定位器信息
//! - **chain**：查看磁盘链（从当前磁盘到基础磁盘的层次关系）

use std::path::{Path, PathBuf};

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
                    walk_chain(file);
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening VHDX file: {e}");
            std::process::exit(1);
        }
    }
}

/// 遍历并输出从当前磁盘到基础磁盘的完整链路
///
/// 链路顺序：child -> parent -> ... -> base。
/// 检测缺失父盘与循环引用，遇到问题时输出错误并 exit(1)。
fn walk_chain(start: &Path) {
    use vhdx_rs::File;

    println!("Disk Chain:");
    println!("  -> {}", start.display());

    // 已访问文件的规范化路径集合，用于检测循环引用
    let mut visited: Vec<PathBuf> = Vec::new();
    // 起始文件加入已访问集合
    if let Ok(canonical) = start.canonicalize() {
        visited.push(canonical);
    } else {
        // 无法规范化起始路径，仍加入原始路径以防无限循环
        visited.push(start.to_path_buf());
    }

    let mut current = start.to_path_buf();

    loop {
        let Ok(vhdx) = File::open(&current).finish() else {
            // 无法打开文件（理论上不应发生在首次迭代之后）
            break;
        };

        let has_parent = vhdx
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
            .unwrap_or(false);

        if !has_parent {
            println!("     (base disk)");
            break;
        }

        // 解析父盘路径
        let metadata = match vhdx.sections().metadata() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Error reading metadata from {}: {e}", current.display());
                std::process::exit(1);
            }
        };

        let parent_path = match metadata
            .items()
            .parent_locator()
            .and_then(|loc| loc.resolve_parent_path())
        {
            Some(p) => p,
            None => {
                eprintln!(
                    "Error: Missing parent path in differencing disk: {}",
                    current.display()
                );
                std::process::exit(1);
            }
        };

        // 将相对路径解析为绝对路径（基于当前磁盘所在目录）
        let resolved_parent = if parent_path.is_absolute() {
            parent_path
        } else {
            match current.parent() {
                Some(dir) => dir.join(&parent_path),
                None => parent_path,
            }
        };

        // 检查父盘文件是否存在
        if !resolved_parent.exists() {
            eprintln!(
                "Error: Parent disk not found: {}",
                resolved_parent.display()
            );
            std::process::exit(1);
        }

        // 规范化父盘路径用于循环检测
        let canonical_parent = match resolved_parent.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "Error resolving parent path {}: {e}",
                    resolved_parent.display()
                );
                std::process::exit(1);
            }
        };

        // 检测循环引用
        if visited.contains(&canonical_parent) {
            eprintln!(
                "Error: Circular reference detected: {} -> {}",
                current.display(),
                resolved_parent.display()
            );
            std::process::exit(1);
        }

        visited.push(canonical_parent);
        current = resolved_parent;
        println!("  -> {}", current.display());
    }
}
