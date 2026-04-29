//! `create` 子命令实现
//!
//! 创建新的 VHDX 虚拟磁盘文件。支持三种磁盘类型：
//! - 动态分配（Dynamic）：按需分配磁盘空间
//! - 固定大小（Fixed）：预分配全部磁盘空间
//! - 差分磁盘（Differencing）：基于父磁盘的增量磁盘

use std::path::Path;

use byte_unit::{Byte, UnitType};

use crate::cli::DiskType;

/// 执行 `create` 子命令
///
/// 根据指定的参数创建新的 VHDX 虚拟磁盘文件。
/// 如果创建的是差分磁盘，必须指定 `--parent` 参数。
///
/// # 参数
/// - `path`: 新 VHDX 文件的保存路径
/// - `size_bytes`: 虚拟磁盘大小（字节）
/// - `disk_type`: 已解析的磁盘类型（动态/固定/差分），由调用方按 `--type` 优先、`--disk-type` 兼容回退的规则合并
/// - `block_size_bytes`: 块大小（字节）
/// - `parent`: 可选的父磁盘路径（差分磁盘必须指定）
/// - `force`: 是否在目标文件已存在时强制覆盖
pub fn cmd_create(
    path: &Path, size_bytes: u64, disk_type: &DiskType, block_size_bytes: u32,
    parent: Option<&Path>, force: bool,
) {
    use vhdx_rs::File;

    // 判断磁盘类型：是否为固定分配
    let fixed = matches!(disk_type, DiskType::Fixed);
    // 判断是否为差分磁盘：类型指定为 Differencing 或提供了父磁盘路径
    let has_parent = matches!(disk_type, DiskType::Differencing) || parent.is_some();

    // 差分磁盘必须指定父磁盘路径
    if has_parent && parent.is_none() {
        eprintln!("Error: Differencing disk requires --parent option");
        std::process::exit(1);
    }

    // --force 处理：仅在目标文件已存在时删除，其他错误路径不受影响
    if path.exists()
        && force
        && let Err(e) = std::fs::remove_file(path)
    {
        eprintln!("Error removing existing file: {e}");
        std::process::exit(1);
    }
    // 未指定 --force 时由库返回 "File already exists" 错误

    let mut builder = File::create(path)
        .size(size_bytes)
        .fixed(fixed)
        .block_size(block_size_bytes);
    if let Some(parent_path) = parent {
        builder = builder.parent_path(parent_path);
    }

    match builder.finish() {
        Ok(_) => {
            println!("Created VHDX file: {}", path.display());
            println!(
                "  Virtual Size: {:.2}",
                Byte::from_u64(size_bytes).get_appropriate_unit(UnitType::Binary)
            );
            println!(
                "  Block Size: {:.2}",
                Byte::from_u64(u64::from(block_size_bytes)).get_appropriate_unit(UnitType::Binary)
            );
            // 显示实际的磁盘类型
            println!(
                "  Type: {}",
                if fixed {
                    "Fixed"
                } else if has_parent {
                    "Differencing"
                } else {
                    "Dynamic"
                }
            );
            // 差分磁盘显示父磁盘路径
            if let Some(parent_path) = parent {
                println!("  Parent: {}", parent_path.display());
            }
        }
        Err(e) => {
            eprintln!("Error creating VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
