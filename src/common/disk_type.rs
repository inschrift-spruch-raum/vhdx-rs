//! VHDX disk type definitions
//!
//! Defines the three VHDX disk types according to MS-VHDX specification.

use std::fmt;

/// VHDX disk type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskType {
    /// Fixed size disk - all space pre-allocated
    Fixed,
    /// Dynamically expanding disk - grows as data is written
    Dynamic,
    /// Differencing disk - has parent disk for changes
    Differencing,
}

impl fmt::Display for DiskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiskType::Fixed => write!(f, "Fixed"),
            DiskType::Dynamic => write!(f, "Dynamic"),
            DiskType::Differencing => write!(f, "Differencing"),
        }
    }
}
