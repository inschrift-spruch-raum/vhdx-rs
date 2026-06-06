//! header module facade.

mod core;

pub use self::core::{
    FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry, RegionTableHeader,
};

#[cfg(test)]
mod tests;
