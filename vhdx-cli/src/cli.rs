use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::utils::{parse_block_size, parse_size};

#[derive(Parser)]
#[command(name = "vhdx-tool")]
#[command(about = "VHDX (Virtual Hard Disk v2) CLI tool")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
        /// Virtual disk size (e.g., 10GiB, 100MiB, 1GiB)
        #[arg(short, long, value_parser = parse_size)]
        size: u64,
        /// Disk type
        #[arg(short, long, value_enum, default_value = "dynamic")]
        disk_type: DiskType,
        /// Block size (e.g., 32MiB, 1MiB)
        #[arg(short, long, default_value = "32MiB", value_parser = parse_block_size)]
        block_size: u32,
        /// Parent disk path (for differencing disks)
        #[arg(short, long)]
        parent: Option<PathBuf>,
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
    /// Repair VHDX file with pending log entries
    Repair {
        /// Path to VHDX file
        file: PathBuf,
        /// Perform a dry run without making changes
        #[arg(short, long)]
        dry_run: bool,
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
pub enum SectionCommand {
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
pub enum DiffCommand {
    /// Show parent disk path
    Parent,
    /// Show disk chain
    Chain,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, ValueEnum)]
pub enum DiskType {
    Dynamic,
    Fixed,
    Differencing,
}
