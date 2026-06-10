use std::path::{Path, PathBuf};
use std::process;

use clap::{Args, Parser, Subcommand, ValueEnum};
use vhdx::{LogReplayPolicy, Medium};

#[derive(Parser)]
#[command(
    name = "vhdx-tool",
    version,
    about = "VHDX (Virtual Hard Disk v2) tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display VHDX file information
    Info(InfoArgs),
    /// Create a new VHDX file
    Create(CreateArgs),
    /// Check VHDX file integrity
    Check(CheckArgs),
    /// View internal VHDX file sections
    Sections(SectionsArgs),
    /// Differencing disk operations (parent path, chain info)
    Diff(DiffArgs),
}

#[derive(clap::Args)]
struct InfoArgs {
    /// Path to the VHDX file
    file: Option<PathBuf>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    format: String,
}

#[derive(clap::Args)]
struct SectionsArgs {
    /// Path to the VHDX file
    file: Option<PathBuf>,
    #[command(subcommand)]
    command: SectionCommand,
}

#[derive(Subcommand)]
enum SectionCommand {
    /// File Type Identifier, Headers, and Region Tables
    Header,
    /// Block Allocation Table entries
    Bat,
    /// Metadata table and items
    Metadata,
    /// Log entries
    Log,
}

#[derive(Args)]
struct DiffArgs {
    /// Path to the VHDX file
    file: Option<PathBuf>,
    #[command(subcommand)]
    action: DiffAction,
}

#[derive(Subcommand)]
enum DiffAction {
    /// Show the resolved parent disk path from the parent locator
    Parent,
    /// Show parent chain information (child, parent, linkage status)
    Chain,
}

#[derive(clap::Args)]
struct CheckArgs {
    /// Path to the VHDX file
    file: Option<PathBuf>,

    /// Replay pending log entries
    #[arg(long)]
    log_replay: bool,

    /// Enable or disable strict validation (default: true)
    #[arg(long, default_value_t = true, num_args = 0..=1, default_missing_value = "true")]
    strict: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum DiskType {
    /// Dynamic (sparse) disk
    Dynamic,
    /// Fixed-size disk
    Fixed,
    /// Differencing disk (requires --parent)
    Differencing,
}

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Dynamic => write!(f, "dynamic"),
            DiskType::Fixed => write!(f, "fixed"),
            DiskType::Differencing => write!(f, "differencing"),
        }
    }
}

#[derive(clap::Args)]
struct CreateArgs {
    /// Path for the new VHDX file
    path: PathBuf,

    /// Virtual disk size (e.g. "1GB", "100MB", "512KB", "2TB", or plain bytes)
    #[arg(long)]
    size: String,

    /// Disk type: dynamic, fixed, or differencing (default: dynamic)
    #[arg(long = "type", value_name = "TYPE", default_value = "dynamic")]
    disk_type: DiskType,

    /// Block size in bytes (must be a power of 2 between 1MB and 256MB)
    #[arg(long, default_value_t = 33554432)]
    block_size: u32,

    /// Parent path for differencing disks
    #[arg(long)]
    parent: Option<PathBuf>,

    /// Logical sector size in bytes (default: 4096)
    #[arg(long, default_value = "4096")]
    logical_sector_size: u32,

    /// Physical sector size in bytes (default: 4096)
    #[arg(long, default_value = "4096")]
    physical_sector_size: u32,

    /// Overwrite existing file
    #[arg(long)]
    force: bool,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Info(args) => {
            cmd_info(args);
            Ok(())
        }
        Commands::Create(args) => cmd_create(&args),
        Commands::Check(args) => {
            cmd_check(args);
            Ok(())
        }
        Commands::Sections(args) => cmd_sections(args),
        Commands::Diff(args) => cmd_diff(args),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Size parsing
// ---------------------------------------------------------------------------

/// Parse a human-readable size string (e.g. "1GB", "100MB", "512KB", "2TB")
/// or a plain number (bytes). Returns the size in bytes.
fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("size cannot be empty".into());
    }

    // Try suffix-based parsing first.
    let suffixes: &[(&str, u64)] = &[
        ("TB", 1024u64 * 1024 * 1024 * 1024),
        ("GB", 1024u64 * 1024 * 1024),
        ("MB", 1024u64 * 1024),
        ("KB", 1024u64),
    ];

    let lower = s.to_ascii_uppercase();
    for (suffix, multiplier) in suffixes {
        if lower.ends_with(suffix) {
            let num_part = s[..s.len() - suffix.len()].trim();
            let value: u64 = num_part
                .parse()
                .map_err(|_| format!("invalid size number: {num_part}"))?;
            return Ok(value * multiplier);
        }
    }

    // Also accept lowercase suffixes like "gb", "mb", "kb", "tb"
    let lower_suffixes: &[(&str, u64)] = &[
        ("tb", 1024u64 * 1024 * 1024 * 1024),
        ("gb", 1024u64 * 1024 * 1024),
        ("mb", 1024u64 * 1024),
        ("kb", 1024u64),
    ];
    for (suffix, multiplier) in lower_suffixes {
        if lower.ends_with(suffix) {
            let num_part = s[..s.len() - suffix.len()].trim();
            let value: u64 = num_part
                .parse()
                .map_err(|_| format!("invalid size number: {num_part}"))?;
            return Ok(value * multiplier);
        }
    }

    // Plain number = bytes
    s.parse().map_err(|_| {
        format!("invalid size: {s} (expected e.g. \"1GB\", \"100MB\", or plain bytes)")
    })
}

fn open_medium(path: &Path) -> vhdx::Result<vhdx::OpenOptions<std::fs::File>> {
    let inner = std::fs::File::open(path).map_err(vhdx::Error::Io)?;
    Ok(Medium::<std::fs::File>::open(inner))
}

// ---------------------------------------------------------------------------
// Create command implementation
// ---------------------------------------------------------------------------

fn cmd_create(args: &CreateArgs) -> vhdx::Result<()> {
    // Parse size string.
    let size_bytes = parse_size(&args.size).map_err(vhdx::Error::InvalidParameter)?;

    // Validate differencing disk has --parent.
    if matches!(args.disk_type, DiskType::Differencing) && args.parent.is_none() {
        return Err(vhdx::Error::InvalidParameter(
            "--parent is required for differencing disks".into(),
        ));
    }
    if args.parent.is_some() && !matches!(args.disk_type, DiskType::Differencing) {
        return Err(vhdx::Error::InvalidParameter(
            "--parent is only valid for differencing disks".into(),
        ));
    }

    // Check file existence.
    if args.path.exists() {
        if args.force {
            std::fs::remove_file(&args.path).map_err(vhdx::Error::Io)?;
        } else {
            return Err(vhdx::Error::InvalidFile(format!(
                "file already exists: {} (use --force to overwrite)",
                args.path.display()
            )));
        }
    }

    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&args.path)
        .map_err(vhdx::Error::Io)?;

    // Build creation options.
    let mut opts = Medium::create(inner).size(size_bytes);

    if matches!(args.disk_type, DiskType::Fixed) {
        opts = opts.fixed(true);
    }

    opts = opts.block_size(args.block_size);

    if let Some(ref parent) = args.parent {
        let mut parent_medium = open_medium(parent)?
            .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
            .finish()?;
        opts = opts.parent(&mut parent_medium, parent)?;
    }

    opts = opts
        .logical_sector_size(args.logical_sector_size)
        .physical_sector_size(args.physical_sector_size);

    // Create the file.
    opts.finish()?;

    println!(
        "Created {} VHDX: {} ({} bytes)",
        args.disk_type,
        args.path.display(),
        size_bytes,
    );

    Ok(())
}

fn cmd_check(args: CheckArgs) {
    let Some(file) = args.file else {
        eprintln!("Usage: vhdx-tool check [FILE] --log-replay");
        process::exit(1);
    };

    // Determine log replay policy based on flags.
    // --log-replay triggers Auto (log replay).
    let log_policy = if args.log_replay {
        LogReplayPolicy::Auto
    } else {
        LogReplayPolicy::Require
    };

    // Open the file.
    let mut medium = match open_medium(&file)
        .and_then(|opts| opts.log_replay(log_policy).strict(args.strict).finish())
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file: {e}");
            process::exit(1);
        }
    };

    // Run structural validation.
    let validator = match medium.validator() {
        Ok(validator) => validator,
        Err(e) => {
            report_error(&e);
            process::exit(1);
        }
    };
    match validator.validate_file() {
        Ok(issues) => {
            if issues.is_empty() {
                println!("No issues found.");
            } else {
                for issue in &issues {
                    println!(
                        "[{}] {}: {} ({})",
                        issue.section(),
                        issue.code(),
                        issue.message(),
                        issue.spec_ref()
                    );
                }
            }
            // Ok means validation passed; issues are advisory findings, not errors.
            process::exit(0);
        }
        Err(e) => {
            report_error(&e);
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Info command implementation
// ---------------------------------------------------------------------------

fn cmd_info(args: InfoArgs) {
    let Some(file) = args.file else {
        eprintln!("Error: no file specified");
        process::exit(1);
    };
    if let Err(e) = run_info(&file, &args.format) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn run_info(path: &PathBuf, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let medium = open_medium(path)?
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;

    let sections = medium.sections()?;
    let (creator, sequence_number, file_write_guid, data_write_guid, version) = {
        let header = sections.header()?;
        let ft = header.file_type();
        let creator = decode_utf16le_creator(&ft.creator()[..]);
        let current = header.header(0)?;
        (
            creator,
            current.sequence_number(),
            current.file_write_guid(),
            current.data_write_guid(),
            current.version(),
        )
    };

    // Metadata
    let metadata = sections.metadata().ok();
    let virtual_size = metadata
        .as_ref()
        .and_then(|m| m.items().virtual_disk_size().ok())
        .unwrap_or(0);
    let logical_sec = metadata
        .as_ref()
        .and_then(|m| m.items().logical_sector_size().ok())
        .unwrap_or(0);
    let physical_sec = metadata
        .as_ref()
        .and_then(|m| m.items().physical_sector_size().ok())
        .unwrap_or(0);

    let block_size;
    let disk_type;
    if let Some(fp) = metadata
        .as_ref()
        .and_then(|m| m.items().file_parameters().ok())
    {
        block_size = fp.block_size();
        disk_type = if fp.has_parent() {
            "Differencing"
        } else if fp.leave_block_allocated() {
            "Fixed"
        } else {
            "Dynamic"
        };
    } else {
        block_size = 0;
        disk_type = "Unknown";
    }

    let info = InfoOutput {
        creator: &creator,
        sequence_number,
        file_write_guid,
        data_write_guid,
        version,
        block_size,
        logical_sector: logical_sec,
        physical_sector: physical_sec,
        virtual_size,
        disk_type,
    };

    if format == "json" {
        print_json(path, &info);
    } else {
        print_text(path, &info);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Sections command implementation
// ---------------------------------------------------------------------------

fn cmd_sections(args: SectionsArgs) -> vhdx::Result<()> {
    let Some(file) = args.file else {
        eprintln!("Error: no file specified");
        process::exit(1);
    };
    let medium = open_medium(&file)?
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;
    let sections = medium.sections()?;

    match args.command {
        SectionCommand::Header => show_header(&sections),
        SectionCommand::Bat => show_bat(&sections),
        SectionCommand::Metadata => show_metadata(&sections),
        SectionCommand::Log => show_log(&sections),
    }
}

// ---------------------------------------------------------------------------
// Diff command implementation
// ---------------------------------------------------------------------------

fn cmd_diff(args: DiffArgs) -> vhdx::Result<()> {
    let Some(file_path) = args.file else {
        eprintln!("Error: no file specified");
        process::exit(1);
    };

    // Use the library's Medium API instead of raw bytes + constructors.
    let mut medium = open_medium(&file_path)?
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;

    let has_parent = {
        let sections = medium.sections()?;
        let metadata = sections.metadata()?;
        let fp = metadata.items().file_parameters().map_err(|_| {
            vhdx::Error::InvalidFile("No FileParameters metadata item found".into())
        })?;
        fp.has_parent()
    };

    if !has_parent {
        return Err(vhdx::Error::InvalidFile(format!(
            "{} is not a differencing disk (has_parent flag is not set)",
            file_path.display()
        )));
    }

    // Dispatch to the requested action.
    match args.action {
        DiffAction::Parent => cmd_diff_parent(&mut medium),
        DiffAction::Chain => cmd_diff_chain(&mut medium, &file_path),
    }
}

/// Show parent path entries from the parent locator.
fn cmd_diff_parent(medium: &mut Medium) -> vhdx::Result<()> {
    let sections = medium.sections()?;
    let metadata = sections.metadata()?;
    let locator = metadata
        .items()
        .parent_locator()
        .map_err(|_| vhdx::Error::InvalidFile("No parent locator metadata item found".into()))?;

    let mut found_path_entry = false;
    for entry in locator.entries() {
        let data = locator.key_value_data();
        let key = entry.key(data)?;
        if matches!(
            key.as_str(),
            "relative_path" | "volume_path" | "absolute_win32_path"
        ) {
            println!("{key}: {}", entry.value(data)?);
            found_path_entry = true;
        }
    }
    if !found_path_entry {
        return Err(vhdx::Error::InvalidFile(
            "parent locator has no path entries".into(),
        ));
    }
    Ok(())
}

/// Show parent chain information for the differencing disk.
fn cmd_diff_chain(medium: &mut Medium, file_path: &Path) -> vhdx::Result<()> {
    // 1. 执行 parent locator 校验（含 DataWriteGuid 比较）
    let issues = medium.validator()?.validate_parent_locator()?;

    // 2. 获取父定位器路径候选项用于显示；不在 crate 内执行隐式文件系统解析。
    let parent_path_entries = {
        let sections = medium.sections()?;
        let metadata = sections.metadata()?;
        let locator = metadata
            .items()
            .parent_locator()
            .map_err(|_| vhdx::Error::InvalidFile("no parent locator".into()))?;
        let data = locator.key_value_data();
        let entries = locator
            .entries()
            .filter_map(|entry| {
                let key = entry.key(data).ok()?;
                if matches!(
                    key.as_str(),
                    "relative_path" | "volume_path" | "absolute_win32_path"
                ) {
                    Some((key, entry.value(data).ok()?))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return Err(vhdx::Error::InvalidFile(
                "parent locator has no path entries".into(),
            ));
        }
        entries
    };

    // 3. 判断 linkage 是否匹配（从校验结果中找 PARENT_LOCATOR_GUID_MISMATCH）
    let linkage_matched = !issues
        .iter()
        .any(|i| i.code() == "PARENT_LOCATOR_GUID_MISMATCH");

    println!("Child path:  {}", file_path.display());
    for (key, value) in parent_path_entries {
        println!("Parent {key}: {value}");
    }
    println!("Linkage matched: {linkage_matched}");

    // 4. 如果有其他 issues，也打印出来
    for issue in &issues {
        println!(
            "  [{}] {}: {}",
            issue.section(),
            issue.code(),
            issue.message()
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Output helpers – text
// ---------------------------------------------------------------------------

mod errors;
mod output;
mod sections;

use errors::report_error;
use output::{InfoOutput, decode_utf16le_creator, print_json, print_text};
use sections::{show_bat, show_header, show_log, show_metadata};

#[cfg(test)]
mod tests;
