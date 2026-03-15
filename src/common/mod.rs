//! Common utilities for VHDX
//!
//! This module provides common infrastructure used throughout the library:
//! - GUID handling
//! - CRC-32C checksums

pub mod crc32c;
pub mod guid;

pub use crc32c::crc32c_with_zero_field;
pub use guid::Guid;
