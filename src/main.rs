//! VHDX Tool - Command line utility for VHDX files

use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use vhdx_rs::header::{read_headers, read_region_tables, FileTypeIdentifier};
use vhdx_rs::metadata::MetadataRegion;
use vhdx_rs::{DiskType, Builder, File};

#[derive(Parser)]
#[command(name = "vhdx-tool")]
#[command(about = "VHDX (Virtual Hard Disk v2) command line tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display information about a VHDX file
    Info {
        /// Path to VHDX file
        path: PathBuf,
    },
    /// Create a new VHDX file
    Create {
        /// Path to new VHDX file
        path: PathBuf,
        /// Virtual disk size (e.g., 10G, 100M)
        #[arg(short, long)]
        size: String,
        /// Disk type: fixed, dynamic, differencing
        #[arg(short, long, default_value = "dynamic")]
        type_: String,
        /// Block size (e.g., 1M, 32M)
        #[arg(short, long)]
        block_size: Option<String>,
        /// Logical sector size (512 or 4096)
        #[arg(long)]
        logical_sector: Option<u32>,
        /// Physical sector size (512 or 4096)
        #[arg(long)]
        physical_sector: Option<u32>,
        /// Parent disk path (for differencing)
        #[arg(short, long)]
        parent: Option<PathBuf>,
    },
    /// Read data from VHDX
    Read {
        /// Path to VHDX file
        path: PathBuf,
        /// Virtual offset to read from
        #[arg(short, long)]
        offset: u64,
        /// Number of bytes to read
        #[arg(short, long)]
        length: usize,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Write data to VHDX
    Write {
        /// Path to VHDX file
        path: PathBuf,
        /// Virtual offset to write to
        #[arg(short, long)]
        offset: u64,
        /// Input file (default: stdin)
        #[arg(short, long)]
        input: Option<PathBuf>,
    },
    /// Check VHDX file integrity
    Check {
        /// Path to VHDX file
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Info { path } => {
            if let Err(e) = show_info(path) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Create {
            path,
            size,
            type_,
            block_size,
            logical_sector,
            physical_sector,
            parent,
        } => {
            if let Err(e) = create_vhdx(
                path,
                size,
                type_,
                block_size,
                logical_sector,
                physical_sector,
                parent,
            ) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Read {
            path,
            offset,
            length,
            output,
        } => {
            if let Err(e) = read_data(path, offset, length, output) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Write {
            path,
            offset,
            input,
        } => {
            if let Err(e) = write_data(path.clone(), offset, input.clone()) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Check { path } => {
            if let Err(e) = check_file(path) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn show_info(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let vhdx = File::open(&path, true).map_err(|e| format!("Failed to open VHDX: {}", e))?;

    println!("VHDX File: {}", path.display());
    println!("============================");
    println!(
        "Virtual Disk Size: {} bytes ({:.2} GB)",
        vhdx.virtual_disk_size(),
        vhdx.virtual_disk_size() as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!(
        "Block Size: {} bytes ({:.2} MB)",
        vhdx.block_size(),
        vhdx.block_size() as f64 / (1024.0 * 1024.0)
    );
    println!("Logical Sector Size: {} bytes", vhdx.logical_sector_size());
    println!(
        "Physical Sector Size: {} bytes",
        vhdx.physical_sector_size()
    );
    println!("Disk Type: {:?}", vhdx.disk_type());
    println!("Virtual Disk ID: {}", vhdx.virtual_disk_id());

    if let Some(creator) = vhdx.creator() {
        println!("Creator: {}", creator);
    }

    if vhdx.has_parent() {
        println!("Has Parent: Yes");
    }

    Ok(())
}

fn read_data(
    path: PathBuf,
    offset: u64,
    length: usize,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx =
        File::open(&path, true).map_err(|e| format!("Failed to open VHDX: {}", e))?;

    let mut buffer = vec![0u8; length];
    let bytes_read = vhdx
        .read(offset, &mut buffer)
        .map_err(|e| format!("Failed to read: {}", e))?;

    buffer.truncate(bytes_read);

    if let Some(output_path) = output {
        std::fs::write(&output_path, &buffer)?;
        println!("Read {} bytes to {}", bytes_read, output_path.display());
    } else {
        // Write to stdout
        use std::io::Write;
        std::io::stdout().write_all(&buffer)?;
    }

    Ok(())
}

fn write_data(
    path: PathBuf,
    offset: u64,
    input: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx =
        File::open(&path, false).map_err(|e| format!("Failed to open VHDX: {}", e))?;

    let buffer = if let Some(input_path) = input {
        std::fs::read(&input_path)?
    } else {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        buf
    };

    let bytes_written = vhdx
        .write(offset, &buffer)
        .map_err(|e| format!("Failed to write: {}", e))?;

    println!("Wrote {} bytes to {}", bytes_written, path.display());
    Ok(())
}

fn check_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking VHDX file: {}\n", path.display());

    // Step 1: Open file
    let mut file = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
    println!("[1/6] ✓ File opened");

    // Step 2: File type identifier
    let mut ft_data = vec![0u8; FileTypeIdentifier::SIZE];
    file.read_exact(&mut ft_data)
        .map_err(|e| format!("Failed to read file type identifier: {}", e))?;
    let file_type = FileTypeIdentifier::from_bytes(&ft_data)
        .map_err(|e| format!("Invalid file type identifier: {}", e))?;
    let creator_info = file_type
        .creator_string()
        .map(|c| format!(" (creator: {})", c))
        .unwrap_or_default();
    println!("[2/6] ✓ File type: vhdxfile{}", creator_info);

    // Step 3: Headers
    let (idx, header, _) =
        read_headers(&mut file).map_err(|e| format!("Failed to read headers: {}", e))?;
    header
        .check_version()
        .map_err(|e| format!("Header version check failed: {}", e))?;
    println!(
        "[3/6] ✓ Headers validated (#{}, seq={})",
        idx + 1,
        header.sequence_number
    );

    // Step 4: Region Table
    let (region_table, is_copy1) = read_region_tables(&mut file)
        .map_err(|e| format!("Failed to read region tables: {}", e))?;
    let table_source = if is_copy1 { "copy 1" } else { "copy 2" };
    let entry_count = region_table.header.entry_count;
    println!(
        "[4/6] ✓ Region table validated ({} - {} entries)",
        table_source, entry_count
    );

    // Step 5: Metadata
    let metadata_entry = region_table
        .find_metadata()
        .ok_or("Metadata region not found in region table")?;
    let mut metadata_data = vec![0u8; metadata_entry.length as usize];
    file.seek(SeekFrom::Start(metadata_entry.file_offset))
        .map_err(|e| format!("Failed to seek to metadata region: {}", e))?;
    file.read_exact(&mut metadata_data)
        .map_err(|e| format!("Failed to read metadata region: {}", e))?;
    let metadata = MetadataRegion::from_bytes(&metadata_data)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    // Parse metadata values for display
    let virtual_disk_size = metadata
        .virtual_disk_size()
        .map_err(|e| format!("Failed to get virtual disk size: {}", e))?;
    let _logical_sector_size = metadata
        .logical_sector_size()
        .map_err(|e| format!("Failed to get logical sector size: {}", e))?;
    let file_params = metadata
        .file_parameters()
        .map_err(|e| format!("Failed to get file parameters: {}", e))?;

    println!(
        "[5/6] ✓ Metadata parsed (size: {} bytes, block size: {} bytes)",
        virtual_disk_size.size, file_params.block_size
    );

    // Step 6: BAT and parent disk check
    let bat_entry = region_table
        .find_bat()
        .ok_or("BAT region not found in region table")?;

    // Check if differencing disk and verify parent if needed
    let mut parent_info = String::new();
    if file_params.has_parent {
        // Try to access parent locator to verify parent is accessible
        if let Ok(locator) = metadata.parent_locator() {
            if locator.parent_path().is_some() {
                parent_info = " (differencing disk)".to_string();
            } else {
                parent_info = " (differencing disk, parent path not set)".to_string();
            }
        } else {
            parent_info = " (differencing disk, parent locator missing)".to_string();
        }
    }

    println!(
        "[6/6] ✓ BAT region found (offset: {}, length: {}){}",
        bat_entry.file_offset, bat_entry.length, parent_info
    );

    println!("\n✓ File is valid!");
    Ok(())
}

fn parse_size(size_str: &str) -> Result<u64, String> {
    let size_str = size_str.trim().to_uppercase();

    // Parse number and unit
    let (num_str, multiplier) = if size_str.ends_with("TB") || size_str.ends_with("T") {
        (
            &size_str[..size_str.len() - if size_str.ends_with("TB") { 2 } else { 1 }],
            1024u64.pow(4),
        )
    } else if size_str.ends_with("GB") || size_str.ends_with("G") {
        (
            &size_str[..size_str.len() - if size_str.ends_with("GB") { 2 } else { 1 }],
            1024u64.pow(3),
        )
    } else if size_str.ends_with("MB") || size_str.ends_with("M") {
        (
            &size_str[..size_str.len() - if size_str.ends_with("MB") { 2 } else { 1 }],
            1024u64.pow(2),
        )
    } else if size_str.ends_with("KB") || size_str.ends_with("K") {
        (
            &size_str[..size_str.len() - if size_str.ends_with("KB") { 2 } else { 1 }],
            1024u64,
        )
    } else if size_str.ends_with("B") {
        (&size_str[..size_str.len() - 1], 1)
    } else {
        // Just a number, assume bytes
        (&size_str[..], 1)
    };

    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("Invalid size number: {}", num_str))?;

    Ok(num * multiplier)
}

fn create_vhdx(
    path: PathBuf,
    size: String,
    type_: String,
    block_size: Option<String>,
    logical_sector: Option<u32>,
    physical_sector: Option<u32>,
    parent: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse disk size
    let virtual_disk_size = parse_size(&size)?;

    // Parse disk type
    let disk_type = match type_.to_lowercase().as_str() {
        "fixed" => DiskType::Fixed,
        "dynamic" => DiskType::Dynamic,
        "differencing" => DiskType::Differencing,
        _ => {
            return Err(format!(
                "Invalid disk type: {}. Use 'fixed', 'dynamic', or 'differencing'",
                type_
            )
            .into())
        }
    };

    // Check for parent requirement
    if disk_type == DiskType::Differencing && parent.is_none() {
        return Err("Differencing disk requires a parent disk. Use --parent <path>".into());
    }

    // Parse block size (default: 32MB)
    let block_size_bytes = block_size
        .map(|s| parse_size(&s))
        .transpose()?
        .unwrap_or(32 * 1024 * 1024);

    // Validate block size (1MB to 256MB, 1MB aligned)
    if !(1024 * 1024..=256 * 1024 * 1024).contains(&block_size_bytes) {
        return Err("Block size must be between 1MB and 256MB"
            .to_string()
            .into());
    }
    if block_size_bytes % (1024 * 1024) != 0 {
        return Err("Block size must be 1MB aligned".to_string().into());
    }

    // Set sector sizes (default: 512 logical, 4096 physical)
    let logical_sector_size = logical_sector.unwrap_or(512);
    let physical_sector_size = physical_sector.unwrap_or(4096);

    // Validate sector sizes
    if logical_sector_size != 512 && logical_sector_size != 4096 {
        return Err("Logical sector size must be 512 or 4096".to_string().into());
    }
    if physical_sector_size != 512 && physical_sector_size != 4096 {
        return Err("Physical sector size must be 512 or 4096"
            .to_string()
            .into());
    }

    // Create the VHDX file
    let mut builder = Builder::new(virtual_disk_size)
        .disk_type(disk_type)
        .block_size(block_size_bytes as u32)
        .sector_sizes(logical_sector_size, physical_sector_size);

    // Handle parent disk for differencing disks
    if let Some(parent_path) = parent {
        // Validate parent exists
        if !parent_path.exists() {
            return Err(format!("Parent disk not found: {}", parent_path.display()).into());
        }

        // Set parent path on builder
        let parent_str = parent_path.to_string_lossy().to_string();
        builder = builder.parent_path(parent_str);
    }

    builder.create(&path)?;

    println!("Successfully created VHDX file: {}", path.display());
    println!(
        "  Size: {} bytes ({:.2} GB)",
        virtual_disk_size,
        virtual_disk_size as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    println!("  Type: {:?}", disk_type);
    println!(
        "  Block size: {} bytes ({:.2} MB)",
        block_size_bytes,
        block_size_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("  Logical sector: {} bytes", logical_sector_size);
    println!("  Physical sector: {} bytes", physical_sector_size);

    Ok(())
}
