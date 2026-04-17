//! VHDX 文件格式公共常量定义
//!
//! 本模块包含 VHDX 虚拟硬盘文件格式中使用的所有常量，
//! 如文件布局偏移量、结构签名、区域和元数据 GUID、块大小限制等。
//! 详见 [`constants`] 子模块。

pub mod constants;

pub use constants::*;
