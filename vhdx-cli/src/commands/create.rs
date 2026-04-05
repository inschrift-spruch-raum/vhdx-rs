use std::path::Path;

use byte_unit::{Byte, UnitType};

use crate::cli::DiskType;

pub fn cmd_create(
    path: &Path, size_bytes: u64, disk_type: &DiskType, block_size_bytes: u32,
    parent: Option<&Path>,
) {
    use vhdx_rs::File;

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
        .block_size(block_size_bytes)
        .finish()
    {
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
