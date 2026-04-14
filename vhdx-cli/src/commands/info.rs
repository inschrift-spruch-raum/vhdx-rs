use std::path::Path;

use byte_unit::{Byte, UnitType};

use crate::cli::OutputFormat;

pub fn cmd_info(file: &Path, format: &OutputFormat) {
    use vhdx_rs::File;

    match File::open(file).finish() {
        Ok(vhdx_file) => {
            if vhdx_file.has_pending_logs() {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match format {
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
                    println!(
                        "Disk Type: {}",
                        if vhdx_file.is_fixed() {
                            "Fixed"
                        } else {
                            "Dynamic"
                        }
                    );
                    if vhdx_file.has_parent() {
                        println!("Type: Differencing (has parent)");
                    }

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
