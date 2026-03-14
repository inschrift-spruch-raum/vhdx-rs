//! Utility module for common helper functions
//!
//! This module contains utility functions and helpers that are used
//! across multiple modules in the VHDX library.

// Re-export commonly used utilities from other modules
pub use crate::common::crc32c::crc32c_with_zero_field;
pub use crate::common::guid::Guid;
