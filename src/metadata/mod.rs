//! metadata module facade.

mod core;

pub use self::core::{
    EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, Metadata, MetadataItems,
    MetadataTable, ParentLocator, StandardItems, TableEntry, TableHeader,
};

#[cfg(test)]
mod tests;
