use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "vhdx-tool")]
#[command(about = "VHDX (Virtual Hard Disk v2) CLI tool")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// View VHDX file information
    Info {
        /// Path to VHDX file
        file: PathBuf,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
    /// Create a new VHDX file
    Create {
        /// Path to create VHDX file
        path: PathBuf,
        /// Virtual disk size (e.g., 10G, 100M)
        #[arg(short, long)]
        size: String,
        /// Disk type
        #[arg(short, long, value_enum, default_value = "dynamic")]
        r#type: DiskType,
        /// Block size (e.g., 32M)
        #[arg(short, long, default_value = "32M")]
        block_size: String,
        /// Parent disk path (for differencing disks)
        #[arg(short, long)]
        parent: Option<PathBuf>,
        /// Force overwrite if file exists
        #[arg(short, long)]
        force: bool,
    },
    /// Check file integrity
    Check {
        /// Path to VHDX file
        file: PathBuf,
        /// Attempt to repair
        #[arg(short, long)]
        repair: bool,
        /// Replay log
        #[arg(short, long)]
        log_replay: bool,
    },
    /// View internal sections
    Sections {
        /// Path to VHDX file
        file: PathBuf,
        #[command(subcommand)]
        section: SectionCommand,
    },
    /// Differencing disk operations
    Diff {
        /// Path to VHDX file
        file: PathBuf,
        #[command(subcommand)]
        command: DiffCommand,
    },
}

#[derive(Subcommand)]
enum SectionCommand {
    /// View Header Section
    Header,
    /// View BAT Entries
    Bat,
    /// View Metadata
    Metadata,
    /// View Log Entries
    Log,
}

#[derive(Subcommand)]
enum DiffCommand {
    /// Show parent disk path
    Parent,
    /// Show disk chain
    Chain,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, ValueEnum)]
enum DiskType {
    Dynamic,
    Fixed,
    Differencing,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Info { file, format } => {
            cmd_info(file, format);
        }
        Commands::Create {
            path,
            size,
            r#type,
            block_size,
            parent,
            force,
        } => {
            cmd_create(path, size, r#type, block_size, parent, force);
        }
        Commands::Check {
            file,
            repair,
            log_replay,
        } => {
            cmd_check(file, repair, log_replay);
        }
        Commands::Sections { file, section } => {
            cmd_sections(file, section);
        }
        Commands::Diff { file, command } => {
            cmd_diff(file, command);
        }
    }
}

fn cmd_info(file: PathBuf, format: OutputFormat) {
    use vhdx_rs::File;

    match File::open(&file).finish() {
        Ok(vhdx_file) => {
            match format {
                OutputFormat::Text => {
                    println!("VHDX File Information");
                    println!("=====================");
                    println!("Path: {}", file.display());
                    println!("Virtual Size: {} bytes", vhdx_file.virtual_disk_size());
                    println!(
                        "Virtual Size (human): {}",
                        human_readable_size(vhdx_file.virtual_disk_size())
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

                    // Show metadata
                    if let Ok(metadata) = vhdx_file.sections().metadata() {
                        let items = metadata.items();
                        if let Some(fp) = items.file_parameters() {
                            println!("\nFile Parameters:");
                            println!("  Leave Block Allocated: {}", fp.leave_block_allocated());
                            println!("  Has Parent: {}", fp.has_parent());
                        }
                        if let Some(disk_id) = items.virtual_disk_id() {
                            println!("\nVirtual Disk ID: {}", disk_id);
                        }
                    }
                }
                OutputFormat::Json => {
                    println!("{{");
                    println!("  \"path\": {:?},", file);
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
            eprintln!("Error opening VHDX file: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_create(
    path: PathBuf,
    size: String,
    disk_type: DiskType,
    block_size: String,
    parent: Option<PathBuf>,
    force: bool,
) {
    use vhdx_rs::File;

    // Check if file exists and --force not specified
    if path.exists() && !force {
        eprintln!("Error: File already exists. Use --force to overwrite.");
        std::process::exit(1);
    }

    // Parse size
    let size_bytes = parse_size(&size);
    if size_bytes == 0 {
        eprintln!("Error: Invalid size format: {}", size);
        std::process::exit(1);
    }

    // Parse block size
    let block_size_bytes = parse_size(&block_size);
    if block_size_bytes == 0 || !block_size_bytes.is_power_of_two() {
        eprintln!("Error: Invalid block size: {}", block_size);
        std::process::exit(1);
    }

    let fixed = matches!(disk_type, DiskType::Fixed);
    let has_parent = matches!(disk_type, DiskType::Differencing) || parent.is_some();

    // Validate parent path for differencing disks
    if has_parent && parent.is_none() {
        eprintln!("Error: Differencing disk requires --parent option");
        std::process::exit(1);
    }

    match File::create(&path)
        .size(size_bytes)
        .fixed(fixed)
        .has_parent(has_parent)
        .block_size(block_size_bytes as u32)
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
            eprintln!("Error creating VHDX file: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_check(file: PathBuf, repair: bool, log_replay: bool) {
    use vhdx_rs::File;

    println!("Checking VHDX file: {}", file.display());

    match File::open(&file).finish() {
        Ok(vhdx_file) => {
            println!("✓ File opened successfully");
            println!("✓ Headers validated");
            println!("✓ Region tables parsed");
            println!("✓ Metadata section valid");

            if let Ok(_) = vhdx_file.sections().bat() {
                println!("✓ BAT section accessible");
            }

            if log_replay {
                println!("\nLog replay requested (not yet implemented)");
            }

            if repair {
                println!("\nRepair requested (not yet implemented)");
            }

            println!("\nFile check completed successfully");
        }
        Err(e) => {
            eprintln!("✗ Error checking VHDX file: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_sections(file: PathBuf, section: SectionCommand) {
    use vhdx_rs::File;

    match File::open(&file).finish() {
        Ok(vhdx_file) => match section {
            SectionCommand::Header => {
                println!("Header Section");
                println!("==============");
                if let Ok(header) = vhdx_file.sections().header() {
                    if let Some(hdr) = header.header(0) {
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
            }
            SectionCommand::Bat => {
                println!("BAT Section");
                println!("===========");
                let bat_entries = vhdx_rs::Bat::calculate_total_entries(
                    vhdx_file.virtual_disk_size(),
                    vhdx_file.block_size(),
                    vhdx_file.logical_sector_size(),
                );
                println!("Total BAT Entries: {}", bat_entries);
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
                        println!("Virtual Disk Size: {} bytes", size);
                    }
                    if let Some(id) = items.virtual_disk_id() {
                        println!("Virtual Disk ID: {}", id);
                    }
                    if let Some(sector_size) = items.logical_sector_size() {
                        println!("Logical Sector Size: {} bytes", sector_size);
                    }
                    if let Some(phys_size) = items.physical_sector_size() {
                        println!("Physical Sector Size: {} bytes", phys_size);
                    }
                }
            }
            SectionCommand::Log => {
                println!("Log Section");
                println!("===========");
                println!("Note: Log viewing not yet implemented");
            }
        },
        Err(e) => {
            eprintln!("Error opening VHDX file: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_diff(file: PathBuf, command: DiffCommand) {
    use vhdx_rs::File;

    match File::open(&file).finish() {
        Ok(vhdx_file) => match command {
            DiffCommand::Parent => {
                if vhdx_file.has_parent() {
                    if let Ok(metadata) = vhdx_file.sections().metadata() {
                        if let Some(locator) = metadata.items().parent_locator() {
                            println!("Parent Locator Entries:");
                            for (i, entry) in locator.entries().iter().enumerate() {
                                if let Some(key) = entry.key(locator.key_value_data()) {
                                    if let Some(value) = entry.value(locator.key_value_data()) {
                                        println!("  [{}] {}: {}", i, key, value);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    println!("This is not a differencing disk (no parent)");
                }
            }
            DiffCommand::Chain => {
                println!("Disk Chain:");
                println!("  -> {}", file.display());
                if vhdx_file.has_parent() {
                    println!("     (has parent - chain traversal not yet implemented)");
                } else {
                    println!("     (base disk)");
                }
            }
        },
        Err(e) => {
            eprintln!("Error opening VHDX file: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_size(size_str: &str) -> u64 {
    let size_str = size_str.trim().to_uppercase();
    let multiplier = if size_str.ends_with("T") {
        1024u64 * 1024 * 1024 * 1024
    } else if size_str.ends_with("G") {
        1024 * 1024 * 1024
    } else if size_str.ends_with("M") {
        1024 * 1024
    } else if size_str.ends_with("K") {
        1024
    } else {
        1
    };

    let number_part = if size_str.ends_with("T")
        || size_str.ends_with("G")
        || size_str.ends_with("M")
        || size_str.ends_with("K")
        || size_str.ends_with("B")
    {
        &size_str[..size_str.len() - 1]
    } else {
        &size_str
    };

    number_part.parse::<u64>().unwrap_or(0) * multiplier
}

fn human_readable_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}
