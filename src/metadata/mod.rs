//! VHDX Metadata Region structures and operations
//!
//! The metadata region contains file parameters and disk properties.
//! This module organizes metadata items into separate files for clarity.

pub mod disk_id;
pub mod disk_size;
pub mod file_parameters;
pub mod parent_locator;
pub mod region;
pub mod sector_size;
pub mod table;

// Re-export commonly used types
pub use disk_id::{VirtualDiskId, VIRTUAL_DISK_ID_GUID};
pub use disk_size::{VirtualDiskSize, VIRTUAL_DISK_SIZE_GUID};
pub use file_parameters::{FileParameters, FILE_PARAMETERS_GUID};
pub use parent_locator::{ParentLocator, ParentLocatorEntry, PARENT_LOCATOR_GUID};
pub use region::MetadataRegion;
pub use sector_size::{SectorSize, LOGICAL_SECTOR_SIZE_GUID, PHYSICAL_SECTOR_SIZE_GUID};
pub use table::{MetadataTableEntry, MetadataTableHeader, METADATA_SIGNATURE};
