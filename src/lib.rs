pub use error::{Error, Result};
pub use types::Guid;

pub use sections::{
    Bat, BatEntry, BatState, DataDescriptor, DataSector, Descriptor, EntryFlags, FileParameters,
    FileTypeIdentifier, Header, HeaderStructure, KeyValueEntry, LocatorHeader, Log, LogEntry,
    LogEntryHeader, Metadata, MetadataItems, MetadataTable, ParentLocator, PayloadBlockState,
    RegionTable, RegionTableEntry, RegionTableHeader, Sections, SectorBitmapState, TableEntry,
    TableHeader, ZeroDescriptor,
};

pub use io_module::{IO, PayloadBlock, Sector};

pub use file::File;

mod common;
mod error;
mod file;
mod io_module;
mod sections;
mod types;
