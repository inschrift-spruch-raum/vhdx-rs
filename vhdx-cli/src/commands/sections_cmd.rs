//! `sections` 子命令实现
//!
//! 查看 VHDX 文件各区域（Section）的详细信息。
//! 支持查看以下区域：
//! - **Header**：文件头信息（序列号、版本、日志偏移等）
//! - **BAT**：块分配表信息
//! - **Metadata**：元数据（磁盘大小、扇区大小、磁盘 ID 等）
//! - **Log**：日志区域（尚未实现）

use std::path::Path;

use crate::cli::SectionCommand;

/// 执行 `sections` 子命令
///
/// 打开 VHDX 文件并根据指定的区域类型显示对应信息。
/// 如果文件存在未完成的日志条目，会输出警告信息。
///
/// # 参数
/// - `file`: VHDX 文件路径
/// - `section`: 要查看的区域类型
pub fn cmd_sections(file: &Path, section: &SectionCommand) {
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

            match section {
                // 查看文件头区域
                SectionCommand::Header => {
                    println!("Header Section");
                    println!("==============");
                    if let Ok(header) = vhdx_file.sections().header()
                        && let Some(hdr) = header.header(0)
                    {
                        println!("Sequence Number: {}", hdr.sequence_number());
                        println!("Version: {}", hdr.version());
                        println!("Log Version: {}", hdr.log_version());
                        println!("Log Length: {}", hdr.log_length());
                        println!("Log Offset: {}", hdr.log_offset());
                        println!("File Write GUID: {}", hdr.file_write_guid());
                        println!("Data Write GUID: {}", hdr.data_write_guid());
                        println!("Log GUID: {}", hdr.log_guid());
                    }
                }
                // 查看块分配表区域
                SectionCommand::Bat => {
                    println!("BAT Section");
                    println!("===========");
                    // 获取 BAT 总条目数
                    let bat_entries = match vhdx_file.sections().bat() {
                        Ok(bat) => bat.len() as u64,
                        Err(_) => 0,
                    };
                    println!("Total BAT Entries: {bat_entries}");
                    println!("\nNote: Full BAT listing not yet implemented");
                }
                // 查看元数据区域
                SectionCommand::Metadata => {
                    println!("Metadata Section");
                    println!("================");
                    if let Ok(metadata) = vhdx_file.sections().metadata() {
                        let items = metadata.items();
                        // 文件参数：块大小、是否保留已分配块、是否有父磁盘
                        if let Some(fp) = items.file_parameters() {
                            println!("Block Size: {} bytes", fp.block_size());
                            println!("Leave Block Allocated: {}", fp.leave_block_allocated());
                            println!("Has Parent: {}", fp.has_parent());
                        }
                        // 虚拟磁盘大小
                        if let Some(size) = items.virtual_disk_size() {
                            println!("Virtual Disk Size: {size} bytes");
                        }
                        // 虚拟磁盘唯一标识符
                        if let Some(id) = items.virtual_disk_id() {
                            println!("Virtual Disk ID: {id}");
                        }
                        // 逻辑扇区大小
                        if let Some(sector_size) = items.logical_sector_size() {
                            println!("Logical Sector Size: {sector_size} bytes");
                        }
                        // 物理扇区大小
                        if let Some(phys_size) = items.physical_sector_size() {
                            println!("Physical Sector Size: {phys_size} bytes");
                        }
                    }
                }
                // 查看日志区域
                SectionCommand::Log => {
                    println!("Log Section");
                    println!("===========");
                    match vhdx_file.sections().log() {
                        Ok(log) => {
                            let entries = log.entries();
                            let total = entries.len();
                            println!("Total Log Entries: {total}");

                            if total == 0 {
                                println!("\nNo log entries found. File is clean.");
                            }

                            for (i, entry) in entries.iter().enumerate() {
                                let header = entry.header();
                                // 将签名字节转为可读字符串
                                let sig = String::from_utf8_lossy(header.signature());
                                println!("\nEntry {i}:");
                                println!("  Signature: {sig}");
                                println!("  Sequence Number: {}", header.sequence_number());
                                println!("  Entry Length: {} bytes", header.entry_length());
                                println!("  Descriptor Count: {}", header.descriptor_count());
                                println!("  Checksum: 0x{:08X}", header.checksum());
                                println!("  Log GUID: {}", header.log_guid());
                                println!("  Flushed File Offset: {}", header.flushed_file_offset());
                                println!("  Last File Offset: {}", header.last_file_offset());

                                // 描述符概要
                                let descriptors = entry.descriptors();
                                let data_count = descriptors
                                    .iter()
                                    .filter(|d| matches!(d, vhdx_rs::section::Descriptor::Data(_)))
                                    .count();
                                let zero_count = descriptors
                                    .iter()
                                    .filter(|d| matches!(d, vhdx_rs::section::Descriptor::Zero(_)))
                                    .count();
                                println!("  Data Descriptors: {data_count}");
                                println!("  Zero Descriptors: {zero_count}");
                            }
                        }
                        Err(e) => {
                            eprintln!("Error parsing log section: {e}");
                            std::process::exit(1);
                        }
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
