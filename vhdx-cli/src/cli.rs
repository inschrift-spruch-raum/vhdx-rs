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
#[command(about = "VHDX (虚拟硬盘 v2) 命令行工具")]
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
    Info {
        /// VHDX 文件路径
        file: PathBuf,
        /// 输出格式
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
    /// 创建新的 VHDX 虚拟磁盘文件
    Create {
        /// VHDX 文件路径
        path: PathBuf,
        /// 虚拟磁盘大小（如 10GB、100MB）
        #[arg(short, long, value_parser = parse_size)]
        size: u64,
        /// 磁盘类型
        #[arg(short, long, value_enum, default_value = "dynamic")]
        disk_type: DiskType,
        /// 块大小（如 1MB、32MB）
        #[arg(short, long, default_value = "32MiB", value_parser = parse_block_size)]
        block_size: u32,
        /// 父磁盘路径（用于差分磁盘）
        #[arg(short, long)]
        parent: Option<PathBuf>,
    },
    /// 检查 VHDX 文件完整性
    Check {
        /// VHDX 文件路径
        file: PathBuf,
        /// 是否在检查时进行修复
        #[arg(short, long)]
        repair: bool,
        /// 是否重放日志
        #[arg(short, long)]
        log_replay: bool,
    },
    /// 修复 VHDX 文件（重放日志）
    Repair {
        /// VHDX 文件路径
        file: PathBuf,
        /// 仅检查，不实际修复
        #[arg(short, long)]
        dry_run: bool,
    },
    /// 查看 VHDX 文件各区域详情
    Sections {
        /// VHDX 文件路径
        file: PathBuf,
        /// 要查看的区域类型
        #[command(subcommand)]
        section: SectionCommand,
    },
    /// 差分磁盘操作
    Diff {
        /// VHDX 文件路径
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
    Header,
    /// 查看 BAT（块分配表）
    Bat,
    /// 查看元数据
    Metadata,
    /// 查看日志
    Log,
}

/// 差分磁盘操作子命令
#[derive(Subcommand)]
pub enum DiffCommand {
    /// 查看父磁盘定位器信息
    Parent,
    /// 查看磁盘链
    Chain,
}

/// 输出格式枚举
#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// 文本格式（默认）
    Text,
    /// JSON 格式
    Json,
}

/// 磁盘类型枚举
#[derive(Clone, ValueEnum)]
pub enum DiskType {
    /// 动态分配（默认）
    Dynamic,
    /// 固定大小
    Fixed,
    /// 差分磁盘
    Differencing,
}
