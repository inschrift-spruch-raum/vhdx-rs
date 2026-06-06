//! log module facade.

mod core;

pub use self::core::{
    DataDescriptor, DataSector, Descriptor, Entry, Log, LogEntryHeader, ZeroDescriptor,
};

#[cfg(test)]
pub(crate) use self::core::DataSectorAssembly;

#[cfg(test)]
mod tests;
