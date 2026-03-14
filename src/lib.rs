//! vhdx-rs - VHDX (Virtual Hard Disk v2) Library
//!
//! This library provides complete support for reading and writing VHDX files
//! according to the Microsoft MS-VHDX specification.
//!
//! ## Features
//! - Fixed, Dynamic, and Differencing disk support
//! - Block-level I/O with caching
//! - Crash recovery via log replay
//! - CLI tool `vhdx-tool` included

pub mod bat;
pub mod block_io;
pub mod common;
pub mod error;
pub mod file;
pub mod header;
pub mod log;
pub mod metadata;
pub mod payload;

// Re-exports for convenience
pub use error::VhdxError;
pub use file::{DiskType, VhdxBuilder, VhdxFile};
