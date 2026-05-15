pub mod section;

#[cfg(feature = "gpt")]
pub mod gpt;

mod bat;
pub(crate) mod common;
mod error;
mod file;
mod header;
mod io;
mod log;
pub(crate) mod log_replay;
mod metadata;
mod sections;
mod types;
pub mod validation;

pub use error::{Error, Result, SignaturePosition};
pub use file::{CreateOptions, File, LogReplayPolicy, OpenOptions, ReadSemanticsPolicy};
pub use io::{IO, Sector};
pub use types::{Crc32c, Guid};
