use std::path::Path;

use crate::cli::DiskType;
use crate::utils::{human_readable_size, parse_size};

pub fn cmd_create(
    path: &Path, size: &str, disk_type: &DiskType, block_size: &str, parent: Option<&Path>,
) {
    use vhdx_rs::File;

    // Parse size
    let size_bytes = parse_size(size);
    if size_bytes == 0 {
        eprintln!("Error: Invalid size format: {size}");
        std::process::exit(1);
    }

    // Parse block size
    let block_size_bytes = parse_size(block_size);
    if block_size_bytes == 0 || !block_size_bytes.is_power_of_two() {
        eprintln!("Error: Invalid block size: {block_size}");
        std::process::exit(1);
    }

    let fixed = matches!(disk_type, DiskType::Fixed);
    let has_parent = matches!(disk_type, DiskType::Differencing) || parent.is_some();

    // Validate parent path for differencing disks
    if has_parent && parent.is_none() {
        eprintln!("Error: Differencing disk requires --parent option");
        std::process::exit(1);
    }

    match File::create(path)
        .size(size_bytes)
        .fixed(fixed)
        .has_parent(has_parent)
        .block_size(u32::try_from(block_size_bytes).unwrap_or(0))
        .finish()
    {
        Ok(_) => {
            println!("Created VHDX file: {}", path.display());
            println!("  Virtual Size: {}", human_readable_size(size_bytes));
            println!("  Block Size: {}", human_readable_size(block_size_bytes));
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
