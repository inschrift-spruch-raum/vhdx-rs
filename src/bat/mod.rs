//! VHDX Block Allocation Table (BAT) module
//!
//! The BAT is a redirection table that translates virtual disk offsets
//! to file offsets for payload blocks and sector bitmap blocks.

pub mod entry;
pub mod states;
pub mod table;

pub use entry::BatEntry;
pub use states::{PayloadBlockState, SectorBitmapState};
pub use table::Bat;
