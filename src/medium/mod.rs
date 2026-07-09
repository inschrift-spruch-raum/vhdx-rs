//! Core medium types: [`Medium`], [`OpenOptions`], [`CreateOptions`], and opening policies.

mod core;
mod create;
mod open;
mod options;
mod policies;

pub(crate) use self::core::{
    CacheEntry, ParentReadCache, is_known_metadata_guid, is_known_region_guid,
};
pub use self::core::{InnerRef, Medium};
pub(crate) use self::core::{read_exact_at, write_all_at};
pub(crate) use self::options::ParentCreateInfo;
pub use self::options::{CreateOptions, Len, OpenOptions, ReadOnly, ReadWrite, SetLen, SyncData};
pub use self::policies::{
    LogReplayPolicy, ParentMedium, ParentRequest, ParentResolver, ReadSemanticsPolicy,
};

#[cfg(test)]
mod tests;
