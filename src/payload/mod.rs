//! VHDX Payload Blocks and Sector Bitmap Module
//!
//! This module provides structures and operations for:
//! - Payload Block state management (re-exported from bat module)
//! - Sector Bitmap operations for differencing disks
//! - Chunk size and ratio calculations

pub mod bitmap;
pub mod chunk;

// Re-export core types from bat module
pub use crate::bat::{BatEntry, PayloadBlockState, SectorBitmapState};
pub use bitmap::SectorBitmap;
pub use chunk::{ChunkCalculator, ChunkInfo};
