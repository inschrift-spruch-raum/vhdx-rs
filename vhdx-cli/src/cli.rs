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
    Info {
        file: PathBuf,
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
    Create {
        path: PathBuf,
        #[arg(short, long, value_parser = parse_size)]
        size: u64,
        #[arg(short, long, value_enum, default_value = "dynamic")]
        disk_type: DiskType,
        #[arg(short, long, default_value = "32MiB", value_parser = parse_block_size)]
        block_size: u32,
        #[arg(short, long)]
        parent: Option<PathBuf>,
    },
    Check {
        file: PathBuf,
        #[arg(short, long)]
        repair: bool,
        #[arg(short, long)]
        log_replay: bool,
    },
    Repair {
        file: PathBuf,
        #[arg(short, long)]
        dry_run: bool,
    },
    Sections {
        file: PathBuf,
        #[command(subcommand)]
        section: SectionCommand,
    },
    Diff {
        file: PathBuf,
        #[command(subcommand)]
        command: DiffCommand,
    },
}

#[derive(Subcommand)]
pub enum SectionCommand {
    Header,
    Bat,
    Metadata,
    Log,
}

#[derive(Subcommand)]
pub enum DiffCommand {
    Parent,
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
