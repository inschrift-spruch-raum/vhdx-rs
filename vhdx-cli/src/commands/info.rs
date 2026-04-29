//! `info` 子命令实现
//!
//! 显示 VHDX 文件的详细信息，包括虚拟磁盘大小、块大小、
//! 扇区大小、磁盘类型以及文件参数等。支持文本和 JSON 两种输出格式。

use std::path::Path;

use byte_unit::{Byte, UnitType};

use crate::cli::OutputFormat;

/// 执行 `info` 子命令
///
/// 打开指定的 VHDX 文件，读取并显示其基本信息。
/// 如果文件存在未完成的日志条目，会输出警告信息。
///
/// # 参数
/// - `file`: VHDX 文件路径
/// - `format`: 输出格式（文本或 JSON）
pub fn cmd_info(file: &Path, format: &OutputFormat) {
    use vhdx_rs::File;

    match File::open(file).finish() {
        Ok(vhdx_file) => {
            // 检查是否存在未完成写入留下的日志条目
            if vhdx_file
                .sections()
                .log()
                .is_ok_and(|l| l.is_replay_required())
            {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match format {
                // 文本格式：以可读的方式展示文件信息
                OutputFormat::Text => {
                    println!("VHDX File Information");
                    println!("=====================");
                    let virtual_size = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().virtual_disk_size())
                        .unwrap_or(0);
                    let block_sz = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.block_size()))
                        .unwrap_or(0);
                    let logical_sector_sz = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().logical_sector_size())
                        .unwrap_or(0);
                    let is_fixed = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| {
                            m.items()
                                .file_parameters()
                                .map(|fp| fp.leave_block_allocated())
                        })
                        .unwrap_or(false);
                    let has_parent = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
                        .unwrap_or(false);
                    println!("Path: {}", file.display());
                    println!("Virtual Size: {virtual_size} bytes");
                    println!(
                        "Virtual Size (human): {:.2}",
                        Byte::from_u64(virtual_size).get_appropriate_unit(UnitType::Binary)
                    );
                    println!("Block Size: {block_sz} bytes");
                    println!("Logical Sector Size: {logical_sector_sz} bytes");
                    // 根据是否固定大小判断磁盘类型
                    println!("Disk Type: {}", if is_fixed { "Fixed" } else { "Dynamic" });
                    // 如果是差分磁盘，显示父磁盘信息
                    if has_parent {
                        println!("Type: Differencing (has parent)");
                    }

                    // 尝试读取并显示文件参数和虚拟磁盘 ID
                    if let Ok(metadata) = vhdx_file.sections().metadata() {
                        let items = metadata.items();
                        if let Some(fp) = items.file_parameters() {
                            println!("\nFile Parameters:");
                            println!("  Leave Block Allocated: {}", fp.leave_block_allocated());
                            println!("  Has Parent: {}", fp.has_parent());
                        }
                        if let Some(disk_id) = items.virtual_disk_id() {
                            println!("\nVirtual Disk ID: {disk_id}");
                        }
                    }
                }
                // JSON 格式：以结构化 JSON 输出
                OutputFormat::Json => {
                    let virtual_size = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().virtual_disk_size())
                        .unwrap_or(0);
                    let block_sz = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.block_size()))
                        .unwrap_or(0);
                    let logical_sector_sz = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().logical_sector_size())
                        .unwrap_or(0);
                    let is_fixed = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| {
                            m.items()
                                .file_parameters()
                                .map(|fp| fp.leave_block_allocated())
                        })
                        .unwrap_or(false);
                    let has_parent = vhdx_file
                        .sections()
                        .metadata()
                        .ok()
                        .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
                        .unwrap_or(false);
                    println!("{{");
                    println!("  \"path\": \"{}\",", file.display());
                    println!("  \"virtual_size\": {virtual_size},");
                    println!("  \"block_size\": {block_sz},");
                    println!("  \"logical_sector_size\": {logical_sector_sz},");
                    println!("  \"is_fixed\": {is_fixed},");
                    println!("  \"has_parent\": {has_parent}");
                    println!("}}");
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
