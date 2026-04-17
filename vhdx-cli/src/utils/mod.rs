//! 工具函数模块
//!
//! 提供命令行工具使用的辅助函数，主要包括：
//! - 大小字符串解析（如 "10GB"、"32MiB" → 字节数）
//! - 块大小验证（必须为 2 的幂）

pub mod size;

pub use size::{parse_block_size, parse_size};
