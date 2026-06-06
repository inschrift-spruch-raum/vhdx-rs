use std::path::{Path, PathBuf};
use std::process;

use clap::{Args, Parser, Subcommand, ValueEnum};
use vhdx::{File, LogReplayPolicy};

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

    // Build creation options.
    let mut opts = File::create(&args.path).size(size_bytes);

    if matches!(args.disk_type, DiskType::Fixed) {
        opts = opts.fixed(true);
    }

    opts = opts.block_size(args.block_size);

    if let Some(ref parent) = args.parent {
        opts = opts.parent_path(parent);
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
    let file = match File::open(&file)
        .log_replay(log_policy)
        .strict(args.strict)
        .finish()
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file: {e}");
            process::exit(1);
        }
    };

    // Run structural validation.
    let validator = file.validator();
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
    // Use File::open() standard path as per API.md
    let file = File::open(path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;

    let sections = file.sections();
    let header = sections.header()?;
    let ft = header.file_type();
    let current = header.header(0)?;

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

    let creator = decode_utf16le_creator(&ft.creator()[..]);

    let info = InfoOutput {
        creator: &creator,
        hdr: &current,
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
    let file = File::open(&file)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;
    let sections = file.sections();

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

    // Use the library's File API instead of raw bytes + constructors.
    let file = File::open(&file_path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;

    let sections = file.sections();
    let fp = {
        let metadata = sections.metadata()?;
        metadata
            .items()
            .file_parameters()
            .map_err(|_| vhdx::Error::InvalidFile("No FileParameters metadata item found".into()))?
    };

    if !fp.has_parent() {
        return Err(vhdx::Error::InvalidFile(format!(
            "{} is not a differencing disk (has_parent flag is not set)",
            file_path.display()
        )));
    }

    // Dispatch to the requested action.
    match args.action {
        DiffAction::Parent => cmd_diff_parent(&file),
        DiffAction::Chain => cmd_diff_chain(&file, &file_path),
    }
}

/// Show the resolved parent disk path from the parent locator.
fn cmd_diff_parent(file: &File) -> vhdx::Result<()> {
    let metadata = file.sections().metadata()?;
    let locator = metadata
        .items()
        .parent_locator()
        .map_err(|_| vhdx::Error::InvalidFile("No parent locator metadata item found".into()))?;

    let parent_path = locator
        .resolve_parent_path()
        .map_err(|e| vhdx::Error::InvalidFile(format!("Failed to resolve parent path: {e}")))?;

    println!("{}", parent_path.display());
    Ok(())
}

/// Show parent chain information for the differencing disk.
fn cmd_diff_chain(file: &File, file_path: &Path) -> vhdx::Result<()> {
    // 1. 执行 parent locator 校验（含 DataWriteGuid 比较）
    let issues = file.validator().validate_parent_locator()?;

    // 2. 获取父路径用于显示
    let parent_path = {
        let metadata = file.sections().metadata()?;
        let locator = metadata
            .items()
            .parent_locator()
            .map_err(|_| vhdx::Error::InvalidFile("no parent locator".into()))?;
        locator
            .resolve_parent_path()
            .map_err(|_| vhdx::Error::InvalidFile("unresolvable parent path".into()))?
    };

    // 3. 判断 linkage 是否匹配（从校验结果中找 PARENT_LOCATOR_GUID_MISMATCH）
    let linkage_matched = !issues
        .iter()
        .any(|i| i.code() == "PARENT_LOCATOR_GUID_MISMATCH");

    println!("Child path:  {}", file_path.display());
    println!("Parent path: {}", parent_path.display());
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
// Section display functions
// ---------------------------------------------------------------------------

fn show_header(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let header = sections.header()?;

    // -- File Type Identifier --
    let ft = header.file_type();
    println!("=== File Type Identifier ===");
    println!(
        "  Signature: {:?}",
        std::str::from_utf8(ft.signature()).unwrap_or("<binary>")
    );
    println!("  Creator:    {} bytes", ft.creator().len());

    // -- Current Header --
    println!();
    println!("=== Current Header (index 0) ===");
    match header.header(0) {
        Ok(h) => {
            println!(
                "  Signature:       {:?}",
                std::str::from_utf8(h.signature()).unwrap_or("<binary>")
            );
            println!("  Sequence Number: {}", h.sequence_number());
            println!("  CRC-32C:         {}", h.checksum());
            println!("  Version:         {} (expected 1)", h.version());
            println!("  Log Version:     {} (expected 0)", h.log_version());
            println!("  Log Offset:      {}", h.log_offset());
            println!("  Log Length:      {}", h.log_length());
            println!("  File Write GUID: {}", h.file_write_guid());
            println!("  Data Write GUID: {}", h.data_write_guid());
            println!("  Log GUID:        {}", h.log_guid());
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }

    // -- Header 1 --
    println!();
    println!("=== Header 1 ===");
    print_header_summary(&header, 1);

    // -- Header 2 --
    println!();
    println!("=== Header 2 ===");
    print_header_summary(&header, 2);

    // -- Region Table 1 --
    println!();
    println!("=== Region Table 1 ===");
    print_region_table(&header, 1);

    // -- Region Table 2 --
    println!();
    println!("=== Region Table 2 ===");
    print_region_table(&header, 2);

    Ok(())
}

fn print_header_summary(header: &vhdx::section::Header<'_>, index: usize) {
    match header.header(index) {
        Ok(h) => {
            println!(
                "  Signature:       {:?}",
                std::str::from_utf8(h.signature()).unwrap_or("<binary>")
            );
            println!("  Sequence Number: {}", h.sequence_number());
            println!("  CRC-32C:         {}", h.checksum());
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }
}

fn print_region_table(header: &vhdx::section::Header<'_>, index: usize) {
    match header.region_table(index) {
        Ok(rt) => {
            let hdr = rt.header();
            println!(
                "  Signature:    {:?}",
                std::str::from_utf8(hdr.signature()).unwrap_or("<binary>")
            );
            println!("  Entry Count:  {}", hdr.entry_count());
            println!("  CRC-32C:      {}", hdr.checksum());
            for (i, entry) in rt.entries().enumerate() {
                println!("  Entry [{i}]:");
                println!("    GUID:     {}", entry.guid());
                println!("    Offset:   {}", entry.file_offset());
                println!("    Length:   {}", entry.length());
                println!("    Required: {}", entry.required());
            }
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }
}

fn show_bat(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    use vhdx::section::BatState;

    let bat = sections.bat()?;

    println!("=== Block Allocation Table (BAT) ===");

    let mut total = 0u64;
    let mut displayed = 0u64;
    for (i, entry) in bat.entries().enumerate() {
        total = i as u64 + 1;
        if displayed < 20 {
            if displayed == 0 {
                println!();
                println!("  First 20 entries:");
            }
            let state_str = match entry.state()? {
                BatState::Payload(s) => format!("Payload({s:?})"),
                BatState::SectorBitmap(s) => format!("SectorBitmap({s:?})"),
            };
            println!(
                "  [{i:>4}] state={state_str}, offset_mb={}",
                entry.file_offset_mb()
            );
            displayed += 1;
        }
    }

    println!("  Total Entries: {total}");
    if total > 20 {
        println!("  ... ({} entries omitted)", total - 20);
    }

    Ok(())
}

fn show_metadata(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let meta = sections.metadata()?;
    let table = meta.table();
    let items = meta.items();

    println!("=== Metadata ===");
    if table.header().signature() == b"metadata" {
        println!("  Signature: metadata (valid)");
    } else {
        println!(
            "  Signature: [invalid: {:?}]",
            std::str::from_utf8(table.header().signature()).unwrap_or("<binary>")
        );
    }
    println!("  Entry Count: {}", table.header().entry_count());
    println!();

    // Show known metadata items
    println!("  Known Metadata Items:");
    if let Ok(fp) = items.file_parameters() {
        println!("    FileParameters:");
        println!("      Block Size:         {} bytes", fp.block_size());
        println!("      Has Parent:         {}", fp.has_parent());
        println!("      Leave Block Alloc:  {}", fp.leave_block_allocated());
    } else {
        println!("    FileParameters: not found");
    }
    if let Ok(size) = items.virtual_disk_size() {
        println!("    VirtualDiskSize:     {size} bytes");
    } else {
        println!("    VirtualDiskSize: not found");
    }
    if let Ok(id) = items.virtual_disk_id() {
        println!("    VirtualDiskId:       {id}");
    } else {
        println!("    VirtualDiskId: not found");
    }
    if let Ok(lss) = items.logical_sector_size() {
        println!("    LogicalSectorSize:   {lss}");
    } else {
        println!("    LogicalSectorSize: not found");
    }
    if let Ok(pss) = items.physical_sector_size() {
        println!("    PhysicalSectorSize:  {pss}");
    } else {
        println!("    PhysicalSectorSize: not found");
    }
    if let Ok(pl) = items.parent_locator() {
        println!("    ParentLocator:");
        println!("      Key-Value Entries: {}", pl.header().key_value_count());
        for (i, kv) in pl.entries().enumerate() {
            let key = kv
                .key(pl.key_value_data())
                .unwrap_or_else(|_| "<decode error>".into());
            let val = kv
                .value(pl.key_value_data())
                .unwrap_or_else(|_| "<decode error>".into());
            println!("      [{i}] \"{key}\" = \"{val}\"");
        }
    } else {
        println!("    ParentLocator: not found");
    }

    // Show all table entries (including unknown GUIDs)
    println!();
    println!("  Raw Table Entries:");
    for (i, entry) in table.entries().enumerate() {
        println!(
            "    [{i}] GUID={}, offset={}, length={}, flags={:#010x}",
            entry.item_id(),
            entry.offset(),
            entry.length(),
            entry.flags_bits(),
        );
    }

    Ok(())
}

fn show_log(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let log = sections.log()?;

    println!("=== Log ===");
    let entries: Vec<_> = log.entries().collect();
    println!("  Total Entries: {}", entries.len());
    println!();
    let display_count = entries.len().min(10);
    for (i, entry) in entries.iter().take(display_count).enumerate() {
        let hdr = entry.header();
        println!("  Entry [{i}]:");
        println!("    Sequence Number:  {}", hdr.sequence_number());
        println!("    Descriptor Count: {}", hdr.descriptor_count());
        println!("    Entry Length:     {} bytes", hdr.entry_length());
        println!("    Tail:             {}", hdr.tail());
        println!("    CRC-32C:          {}", hdr.checksum());
    }
    if entries.len() > 10 {
        println!("  ... ({} entries omitted)", entries.len() - 10);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Output helpers – text
// ---------------------------------------------------------------------------

mod errors;
mod output;

use errors::report_error;
use output::{InfoOutput, decode_utf16le_creator, print_json, print_text};

#[cfg(test)]
mod tests;
