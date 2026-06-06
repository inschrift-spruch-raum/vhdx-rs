//! Core file types: [`File`], [`OpenOptions`], [`CreateOptions`], and opening policies.

mod core;
mod create;
mod open;
mod options;
mod policies;

pub use self::core::File;
pub(crate) use self::core::HEADER_BUFFER_SIZE;
pub(crate) use self::core::{is_known_metadata_guid, is_known_region_guid};
pub use self::options::{CreateOptions, OpenOptions};
pub use self::policies::{LogReplayPolicy, ReadSemanticsPolicy};

#[cfg(test)]
pub(crate) use self::policies::ParentChainInfo;

#[cfg(test)]
mod tests;
