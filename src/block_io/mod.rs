//! Block I/O module
//!
//! Provides block-level I/O operations for VHDX virtual disks
//! with support for fixed, dynamic, and differencing disk types.
//!
//! This module contains:
//! - `traits`: Core Block I/O trait definitions
//! - `fixed`: Fixed disk block I/O implementation
//! - `dynamic`: Dynamic disk block I/O implementation
//! - `differencing`: Differencing disk block I/O implementation
//! - `cache`: Block cache for performance optimization

pub mod cache;
pub mod differencing;
pub mod dynamic;
pub mod fixed;
pub mod traits;

// Re-export main types
pub use cache::BlockCache;
pub use differencing::DifferencingBlockIo;
pub use dynamic::DynamicBlockIo;
pub use fixed::FixedBlockIo;
pub use traits::{BlockAllocator, BlockIo, DifferencingIo};
