//! VHDX File module
//!
//! Provides high-level API for opening, reading, writing, and creating VHDX files.

pub mod builder;
pub mod file;

pub use builder::Builder;
pub use file::{CheckReport, VhdxFile as File};

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

impl std::fmt::Display for DiskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskType::Fixed => write!(f, "Fixed"),
            DiskType::Dynamic => write!(f, "Dynamic"),
            DiskType::Differencing => write!(f, "Differencing"),
        }
    }
}
