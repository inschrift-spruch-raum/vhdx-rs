//! File access and log replay policies.

#[cfg(test)]
use std::path::PathBuf;

// Policies
// ---------------------------------------------------------------------------

/// Log replay policy controlling how pending logs are handled on open.
///
/// # Standard
///
/// MS-VHDX §2.3 + MS-VHDX-只读扩展标准 §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogReplayPolicy {
    /// If a replayable log exists, `finish()` returns
    /// [`crate::error::Error::LogReplayRequired`]. No implicit replay.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.1
    #[default]
    Require,

    /// Automatically replay the log during `finish()`.
    /// On read-only open, replay is done in memory only.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.2
    Auto,

    /// In-memory replay is allowed for read-only opens.
    /// Not valid for read-write opens.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.3
    InMemoryOnReadOnly,

    /// Open read-only without replaying the log.
    /// Only structure-level reads are guaranteed consistent;
    /// payload data-plane reads may be inconsistent.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.4
    ReadOnlyNoReplay,
}

/// BAT read semantics policy.
///
/// Controls whether effective data or raw data is preferred when resolving
/// block reads. For differencing disks, child data is always preferred
/// regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadSemanticsPolicy {
    /// Prefer effective (possibly parent-assembled) data.
    #[default]
    EffectiveDataPreferred,
    /// Prefer raw on-disk data.
    RawDataPreferred,
}

/// Result of parent chain validation for differencing disks.
#[cfg(test)]
pub(crate) struct ParentChainInfo {
    pub(crate) _child_path: PathBuf,
    pub(crate) _parent_path: PathBuf,
    pub(crate) _linkage_matched: bool,
}
