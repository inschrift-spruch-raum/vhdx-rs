//! File open/create option builder state.

use std::path::PathBuf;

use super::LogReplayPolicy;

/// Builder for configuring how an existing VHDX file is opened.
///
/// Obtain via [`File::open`]. The default configuration is:
/// - read-only (no write access)
/// - strict validation enabled
/// - log replay policy: [`LogReplayPolicy::Require`]
///
/// # Standard
///
/// docs/Standard/MS-VHDX-只读扩展标准.md §3/§4
pub struct OpenOptions {
    pub(super) path: PathBuf,
    pub(super) write: bool,
    pub(super) strict: bool,
    pub(super) log_replay_policy: LogReplayPolicy,
}

/// Builder for creating a new VHDX file.
pub struct CreateOptions {
    pub(super) path: PathBuf,
    pub(super) virtual_size: u64,
    pub(super) fixed: bool,
    pub(super) block_size: u32,
    pub(super) logical_sector_size: u32,
    pub(super) physical_sector_size: u32,
    pub(super) parent_path: Option<PathBuf>,
}
