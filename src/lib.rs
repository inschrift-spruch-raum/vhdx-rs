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

mod bat;
mod block_io;
mod common;
mod error;
mod file;
mod header;
mod log;
mod metadata;
mod payload;

// Re-exports for convenience
pub use error::Error;
pub use file::{Builder, DiskType, File};
