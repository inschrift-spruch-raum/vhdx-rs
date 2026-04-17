//! 命令实现模块
//!
//! 本模块包含所有 VHDX CLI 子命令的具体实现。
//! 每个子命令对应一个独立的子模块，通过 `pub use` 重新导出命令处理函数。
//!
//! 支持的命令：
//! - `info`：显示 VHDX 文件信息
//! - `create`：创建新的 VHDX 虚拟磁盘
//! - `check`：检查文件完整性
//! - `repair`：修复 VHDX 文件
//! - `sections_cmd`：查看文件各区域详情
//! - `diff`：差分磁盘操作

pub mod check;
pub mod create;
pub mod diff;
pub mod info;
pub mod repair;
pub mod sections_cmd;

pub use check::cmd_check;
pub use create::cmd_create;
pub use diff::cmd_diff;
pub use info::cmd_info;
pub use repair::cmd_repair;
pub use sections_cmd::cmd_sections;
