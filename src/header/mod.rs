//! VHDX Header module
//!
//! Contains:
//! - File Type Identifier
//! - VHDX Headers (dual header mechanism)
//! - Region Table

pub mod file_type;
pub mod region_table;
pub mod vhdx_header;

pub use file_type::{FileTypeIdentifier, FILE_TYPE_SIGNATURE};
pub use region_table::{
    read_region_tables, RegionTable, RegionTableEntry, RegionTableHeader, BAT_GUID, METADATA_GUID,
    REGION_SIGNATURE,
};
pub use vhdx_header::{read_headers, update_headers, VhdxHeader, HEADER_SIGNATURE};

/// Type alias for internal use
pub type Header = VhdxHeader;
