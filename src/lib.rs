//! VHDX (Virtual Hard Disk v2) Library
//!
//! This library provides complete support for reading and writing VHDX files
//! according to the Microsoft MS-VHDX specification.

pub mod bat;
pub mod block_io;
pub mod common;
pub mod error;
pub mod file;
pub mod header;
pub mod log;
pub mod metadata;
pub mod payload;
pub mod utils;

// Re-exports for convenience
pub use error::VhdxError;
pub use file::{DiskType, VhdxBuilder, VhdxFile};
