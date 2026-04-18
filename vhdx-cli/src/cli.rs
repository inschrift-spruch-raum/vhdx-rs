//! 命令行界面定义模块
//!
//! 使用 clap derive 宏定义 VHDX 工具的所有命令行参数和子命令。
//! 包含主命令结构体 `Cli`、子命令枚举 `Commands`、以及相关的枚举类型。

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::utils::{parse_block_size, parse_size};

/// VHDX (虚拟硬盘 v2) 命令行工具
#[derive(Parser)]
#[command(name = "vhdx-tool")]
#[command(about = "VHDX (Virtual Hard Disk v2) CLI tool")]
#[command(version)]
pub struct Cli {
    /// 要执行的子命令
    #[command(subcommand)]
    pub command: Commands,
}

/// 所有可用的子命令
#[derive(Subcommand)]
pub enum Commands {
    /// 显示 VHDX 文件信息
    #[command(about = "Display VHDX file information")]
    Info {
        /// VHDX 文件路径
        #[arg(help = "Path to the VHDX file")]
        file: PathBuf,
        /// 输出格式
        #[arg(
            short,
            long,
            value_enum,
            default_value = "text",
            help = "Output format"
        )]
        format: OutputFormat,
    },
    /// 创建新的 VHDX 虚拟磁盘文件
    #[command(about = "Create a new VHDX virtual disk file")]
    Create {
        /// VHDX 文件路径
        #[arg(help = "Path to the new VHDX file")]
        path: PathBuf,
        /// 虚拟磁盘大小（如 10GB、100MB）
        #[arg(short, long, value_parser = parse_size, help = "Virtual disk size (e.g. 10GB, 100MB)")]
        size: u64,
        /// 磁盘类型
        #[arg(short, long, value_enum, default_value = "dynamic", help = "Disk type")]
        disk_type: DiskType,
        /// 块大小（如 1MB、32MB）
        #[arg(short, long, default_value = "32MiB", value_parser = parse_block_size, help = "Block size (e.g. 1MB, 32MB)")]
        block_size: u32,
        /// 父磁盘路径（用于差分磁盘）
        #[arg(short, long, help = "Parent disk path (for differencing disks)")]
        parent: Option<PathBuf>,
    },
    /// 检查 VHDX 文件完整性
    #[command(about = "Check VHDX file integrity")]
    Check {
        /// VHDX 文件路径
        #[arg(help = "Path to the VHDX file")]
        file: PathBuf,
        /// 是否在检查时进行修复
        #[arg(short, long, help = "Repair during check")]
        repair: bool,
        /// 是否重放日志
        #[arg(short, long, help = "Replay log entries")]
        log_replay: bool,
    },
    /// 修复 VHDX 文件（重放日志）
    #[command(about = "Repair VHDX file (replay log)")]
    Repair {
        /// VHDX 文件路径
        #[arg(help = "Path to the VHDX file")]
        file: PathBuf,
        /// 仅检查，不实际修复
        #[arg(short, long, help = "Check only, do not modify")]
        dry_run: bool,
    },
    /// 查看 VHDX 文件各区域详情
    #[command(about = "View VHDX file section details")]
    Sections {
        /// VHDX 文件路径
        #[arg(help = "Path to the VHDX file")]
        file: PathBuf,
        /// 要查看的区域类型
        #[command(subcommand)]
        section: SectionCommand,
    },
    /// 差分磁盘操作
    #[command(about = "Differencing disk operations")]
    Diff {
        /// VHDX 文件路径
        #[arg(help = "Path to the VHDX file")]
        file: PathBuf,
        /// 差分操作子命令
        #[command(subcommand)]
        command: DiffCommand,
    },
}

/// 区域查看子命令
#[derive(Subcommand)]
pub enum SectionCommand {
    /// 查看文件头信息
    #[command(about = "View header section")]
    Header,
    /// 查看 BAT（块分配表）
    #[command(about = "View BAT (Block Allocation Table)")]
    Bat,
    /// 查看元数据
    #[command(about = "View metadata section")]
    Metadata,
    /// 查看日志
    #[command(about = "View log section")]
    Log,
}

/// 差分磁盘操作子命令
#[derive(Subcommand)]
pub enum DiffCommand {
    /// 查看父磁盘定位器信息
    #[command(about = "View parent locator information")]
    Parent,
    /// 查看磁盘链
    #[command(about = "View disk chain")]
    Chain,
}

/// 输出格式枚举
#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// 文本格式（默认）
    #[value(help = "Text format (default)")]
    Text,
    /// JSON 格式
    #[value(help = "JSON format")]
    Json,
}

/// 磁盘类型枚举
#[derive(Clone, ValueEnum)]
pub enum DiskType {
    /// 动态分配（默认）
    #[value(help = "Dynamic allocation (default)")]
    Dynamic,
    /// 固定大小
    #[value(help = "Fixed size")]
    Fixed,
    /// 差分磁盘
    #[value(help = "Differencing disk")]
    Differencing,
}
