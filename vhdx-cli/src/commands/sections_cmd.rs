use std::path::Path;

use crate::cli::SectionCommand;

pub fn cmd_sections(file: &Path, section: &SectionCommand) {
    use vhdx_rs::File;

    match File::open(file).finish() {
        Ok(vhdx_file) => {
            // Show warning if there are pending log entries
            if vhdx_file.has_pending_logs() {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match section {
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
                SectionCommand::Bat => {
                    println!("BAT Section");
                    println!("===========");
                    let bat_entries = vhdx_rs::Bat::calculate_total_entries(
                        vhdx_file.virtual_disk_size(),
                        vhdx_file.block_size(),
                        vhdx_file.logical_sector_size(),
                    );
                    println!("Total BAT Entries: {bat_entries}");
                    println!("\nNote: Full BAT listing not yet implemented");
                }
                SectionCommand::Metadata => {
                    println!("Metadata Section");
                    println!("================");
                    if let Ok(metadata) = vhdx_file.sections().metadata() {
                        let items = metadata.items();
                        if let Some(fp) = items.file_parameters() {
                            println!("Block Size: {} bytes", fp.block_size());
                            println!("Leave Block Allocated: {}", fp.leave_block_allocated());
                            println!("Has Parent: {}", fp.has_parent());
                        }
                        if let Some(size) = items.virtual_disk_size() {
                            println!("Virtual Disk Size: {size} bytes");
                        }
                        if let Some(id) = items.virtual_disk_id() {
                            println!("Virtual Disk ID: {id}");
                        }
                        if let Some(sector_size) = items.logical_sector_size() {
                            println!("Logical Sector Size: {sector_size} bytes");
                        }
                        if let Some(phys_size) = items.physical_sector_size() {
                            println!("Physical Sector Size: {phys_size} bytes");
                        }
                    }
                }
                SectionCommand::Log => {
                    println!("Log Section");
                    println!("===========");
                    println!("Note: Log viewing not yet implemented");
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
