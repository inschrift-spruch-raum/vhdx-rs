//! VHDX File module
//!
//! Provides high-level API for opening, reading, writing, and creating VHDX files.

pub mod builder;
pub mod vhdx_file;

pub use builder::VhdxBuilder;
pub use vhdx_file::VhdxFile;

/// VHDX disk type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskType {
    /// Fixed size disk
    Fixed,
    /// Dynamically expanding disk
    Dynamic,
    /// Differencing disk (has parent)
    Differencing,
}
