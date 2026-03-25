//! VHDX (Virtual Hard Disk v2) library
//!
//! This library provides support for reading and writing VHDX files,
//! a virtual disk format used by Microsoft Hyper-V and other virtualization platforms.
//!
//! # Example
//!
//! ```rust,no_run
//! use vhdx_rs::File;
//!
//! // Open an existing VHDX file
//! let file = File::open("disk.vhdx").finish()?;
//!
//! // Access sections
//! let header = file.sections().header()?;
//! let metadata = file.sections().metadata()?;
//!
//! println!("Virtual size: {}", metadata.items().virtual_disk_size().unwrap_or(0));
//! # Ok::<(), vhdx_rs::Error>(())
//! ```

// Core types
pub use error::{Error, Result};
pub use types::Guid;

// Re-export section types
pub use sections::{
    Bat, BatEntry, BatState, DataDescriptor, DataSector, Descriptor, EntryFlags, FileParameters,
    FileTypeIdentifier, Header, HeaderStructure, KeyValueEntry, LocatorHeader, Log, LogEntry,
    LogEntryHeader, Metadata, MetadataItems, MetadataTable, ParentLocator, PayloadBlockState,
    RegionTable, RegionTableEntry, RegionTableHeader, Sections, SectorBitmapState, TableEntry,
    TableHeader, ZeroDescriptor,
};

// IO module
pub use io_module::{IO, PayloadBlock, Sector};

// File operations
pub use file::File;

// Internal modules
mod common;
mod error;
mod file;
mod io_module;
mod sections;
mod types;
