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
            if vhdx_file.has_pending_logs() {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match format {
                // 文本格式：以可读的方式展示文件信息
                OutputFormat::Text => {
                    println!("VHDX File Information");
                    println!("=====================");
                    println!("Path: {}", file.display());
                    println!("Virtual Size: {} bytes", vhdx_file.virtual_disk_size());
                    println!(
                        "Virtual Size (human): {:.2}",
                        Byte::from_u64(vhdx_file.virtual_disk_size())
                            .get_appropriate_unit(UnitType::Binary)
                    );
                    println!("Block Size: {} bytes", vhdx_file.block_size());
                    println!(
                        "Logical Sector Size: {} bytes",
                        vhdx_file.logical_sector_size()
                    );
                    // 根据是否固定大小判断磁盘类型
                    println!(
                        "Disk Type: {}",
                        if vhdx_file.is_fixed() {
                            "Fixed"
                        } else {
                            "Dynamic"
                        }
                    );
                    // 如果是差分磁盘，显示父磁盘信息
                    if vhdx_file.has_parent() {
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
                    println!("{{");
                    println!("  \"path\": \"{}\",", file.display());
                    println!("  \"virtual_size\": {},", vhdx_file.virtual_disk_size());
                    println!("  \"block_size\": {},", vhdx_file.block_size());
                    println!(
                        "  \"logical_sector_size\": {},",
                        vhdx_file.logical_sector_size()
                    );
                    println!("  \"is_fixed\": {},", vhdx_file.is_fixed());
                    println!("  \"has_parent\": {}", vhdx_file.has_parent());
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
