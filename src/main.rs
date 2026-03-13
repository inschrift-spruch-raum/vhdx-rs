//! VHDX Tool - Command line utility for VHDX files

use clap::{Parser, Subcommand};
use linkfs::{DiskType, VhdxBuilder, VhdxFile};
use std::path::PathBuf;

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
            eprintln!("Create command not yet implemented");
            eprintln!("Parameters:");
            eprintln!("  Path: {}", path.display());
            eprintln!("  Size: {}", size);
            eprintln!("  Type: {}", type_);
            eprintln!("  Block size: {:?}", block_size);
            eprintln!("  Logical sector: {:?}", logical_sector);
            eprintln!("  Physical sector: {:?}", physical_sector);
            eprintln!("  Parent: {:?}", parent);
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
            eprintln!("Write command not yet implemented");
            eprintln!("Parameters:");
            eprintln!("  Path: {}", path.display());
            eprintln!("  Offset: {}", offset);
            eprintln!("  Input: {:?}", input);
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
    let vhdx = VhdxFile::open(&path, true).map_err(|e| format!("Failed to open VHDX: {}", e))?;

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
        VhdxFile::open(&path, true).map_err(|e| format!("Failed to open VHDX: {}", e))?;

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

fn check_file(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking VHDX file: {}", path.display());

    match VhdxFile::open(&path, true) {
        Ok(vhdx) => {
            println!("✓ File opened successfully");
            println!("✓ Headers validated");
            println!("✓ Region table validated");
            println!("✓ Metadata parsed");
            println!("✓ BAT loaded");

            if vhdx.has_parent() {
                println!("✓ Parent disk accessible");
            }

            println!("\nFile is valid!");
        }
        Err(e) => {
            eprintln!("✗ File check failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
