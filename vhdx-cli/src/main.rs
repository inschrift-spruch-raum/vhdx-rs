//! vhdx-cli：VHDX (虚拟硬盘 v2) 命令行工具入口
#![allow(clippy::too_many_lines, clippy::manual_let_else)]
//!
//! 本模块是 vhdx-tool 可执行程序的入口点。
//! 它解析命令行参数并分派到对应的子命令处理函数。

mod cli; // 命令行界面定义
mod commands; // 各子命令的实现
mod utils; // 工具函数（如大小解析）

use clap::Parser;

use cli::{Cli, Commands};

/// 程序入口函数
///
/// 解析命令行参数，根据子命令类型调用对应的处理函数。
/// 各子命令的处理逻辑位于 `commands` 模块中。
fn main() {
    let cli = Cli::parse();

    match cli.command {
        // 显示 VHDX 文件信息
        Commands::Info { file, format } => {
            commands::cmd_info(&file, &format);
        }
        // 创建新的 VHDX 虚拟磁盘
        Commands::Create {
            path,
            size,
            disk_type,
            disk_type_compat,
            block_size,
            parent,
            force,
        } => {
            // 解析磁盘类型：--type 优先，--disk-type 作为兼容回退，默认 dynamic
            let resolved = disk_type
                .or(disk_type_compat)
                .unwrap_or(cli::DiskType::Dynamic);
            commands::cmd_create(&path, size, &resolved, block_size, parent.as_deref(), force);
        }
        // 检查 VHDX 文件完整性
        Commands::Check {
            file,
            repair,
            log_replay,
        } => {
            commands::cmd_check(&file, repair, log_replay);
        }
        // 修复 VHDX 文件（重放日志）
        Commands::Repair { file, dry_run } => {
            commands::cmd_repair(&file, dry_run);
        }
        // 查看 VHDX 文件各区域详情
        Commands::Sections { file, section } => {
            commands::cmd_sections(&file, &section);
        }
        // 差分磁盘操作
        Commands::Diff { file, command } => {
            commands::cmd_diff(&file, &command);
        }
    }
}
