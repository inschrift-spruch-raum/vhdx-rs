//! VHDX (Virtual Hard Disk v2) Library
//!
//! This library provides complete support for reading and writing VHDX files
//! according to the Microsoft MS-VHDX specification.

pub mod bat;
pub mod block;
pub mod crc32c;
pub mod error;
pub mod guid;
pub mod header;
pub mod log;
pub mod metadata;
pub mod region;
pub mod vhdx;

pub use error::{Result, VhdxError};
pub use guid::Guid;
pub use vhdx::{DiskType, VhdxBuilder, VhdxFile};
