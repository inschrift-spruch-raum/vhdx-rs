//! Section module re-exports per API.md.
//!
//! Groups all VHDX physical file structure types under a single namespace.

pub use crate::sections::Sections;

// Header section types
pub use crate::header::{
    FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry, RegionTableHeader,
};

// BAT section types
pub use crate::bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};

// Metadata section types
pub use crate::metadata::{
    EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, Metadata, MetadataItems,
    MetadataTable, ParentLocator, StandardItems, TableEntry, TableHeader,
};

// Log section types
pub use crate::log::{
    DataDescriptor, DataSector, Descriptor, Entry, Log, LogEntryHeader, ZeroDescriptor,
};
