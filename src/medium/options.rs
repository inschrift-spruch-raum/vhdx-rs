//! Medium open/create option builder state.

use std::marker::PhantomData;
use std::path::PathBuf;

use super::{LogReplayPolicy, ParentResolver};
use crate::types::Guid;

pub trait Len {
    /// Return the current byte length of the medium.
    ///
    /// # Errors
    ///
    /// Returns an error if the medium length cannot be queried.
    fn len(&mut self) -> std::io::Result<u64>;

    /// Return whether the medium length is zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the medium length cannot be queried.
    fn is_empty(&mut self) -> std::io::Result<bool> {
        self.len().map(|len| len == 0)
    }
}

pub trait SetLen: Len {
    /// Set the byte length of the medium.
    ///
    /// # Errors
    ///
    /// Returns an error if the medium length cannot be changed.
    fn set_len(&mut self, len: u64) -> std::io::Result<()>;
}

pub trait SyncData {
    /// Flush buffered file data to stable storage when supported.
    ///
    /// # Errors
    ///
    /// Returns an error if synchronization fails.
    fn sync_data(&mut self) -> std::io::Result<()>;
}

impl Len for std::fs::File {
    fn len(&mut self) -> std::io::Result<u64> {
        Ok(self.metadata()?.len())
    }
}

impl SetLen for std::fs::File {
    fn set_len(&mut self, len: u64) -> std::io::Result<()> {
        std::fs::File::set_len(self, len)
    }
}

impl SyncData for std::fs::File {
    fn sync_data(&mut self) -> std::io::Result<()> {
        std::fs::File::sync_data(self)
    }
}

impl Len for std::io::Cursor<Vec<u8>> {
    fn len(&mut self) -> std::io::Result<u64> {
        u64::try_from(self.get_ref().len())
            .map_err(|_| std::io::Error::other("cursor length does not fit u64"))
    }
}

impl SetLen for std::io::Cursor<Vec<u8>> {
    fn set_len(&mut self, len: u64) -> std::io::Result<()> {
        let len = usize::try_from(len)
            .map_err(|_| std::io::Error::other("cursor length does not fit usize"))?;
        self.get_mut().resize(len, 0);
        if self.position() > len as u64 {
            self.set_position(len as u64);
        }
        Ok(())
    }
}

impl SyncData for std::io::Cursor<Vec<u8>> {
    fn sync_data(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Builder for configuring how an existing VHDX file is opened.
///
/// Obtain via [`Medium::open`]. The default configuration is:
/// - read-only (no write access)
/// - strict validation enabled
/// - log replay policy: [`LogReplayPolicy::Require`]
///
/// # Standard
///
/// docs/Standard/MS-VHDX-只读扩展标准.md §3/§4
pub struct ReadOnly;

pub struct ReadWrite;

pub struct OpenOptions<T, Mode = ReadOnly> {
    pub(super) inner: T,
    pub(super) strict: bool,
    pub(super) log_replay_policy: LogReplayPolicy,
    pub(super) parent_resolver: Option<Box<dyn ParentResolver + Send>>,
    pub(super) _mode: PhantomData<Mode>,
}

/// Builder for creating a new VHDX file.
pub struct CreateOptions<T = std::fs::File> {
    pub(super) inner: Option<T>,
    pub(super) virtual_size: u64,
    pub(super) fixed: bool,
    pub(super) block_size: u32,
    pub(super) logical_sector_size: u32,
    pub(super) physical_sector_size: u32,
    pub(super) parent: Option<ParentCreateInfo>,
}

pub(crate) struct ParentCreateInfo {
    pub(super) relative_path: PathBuf,
    pub(super) data_write_guid: Guid,
}
