pub mod section;

#[cfg(feature = "gpt")]
pub mod gpt;

mod bat;
pub(crate) mod common;
pub(crate) mod constants;
mod error;
mod header;
mod io;
mod log;
pub(crate) mod log_replay;
mod medium;
mod metadata;
mod types;
pub mod validation;

pub use error::{Error, Result, SignaturePosition};
pub use io::{IO, Sector};
pub use medium::{
    CreateOptions, InnerRef, Len, LogReplayPolicy, Medium, OpenOptions, ParentMedium,
    ParentRequest, ParentResolver, ReadOnly, ReadSemanticsPolicy, ReadWrite, SetLen, SyncData,
};
pub use types::{Crc32c, Guid};
