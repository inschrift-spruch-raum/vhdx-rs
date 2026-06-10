pub(super) use crate::constants::{MIB, SECTOR_SIZE};
pub(super) use crate::error::Error;
pub(super) use crate::io::*;
pub(super) use crate::log_replay::ReplayOverlay;
pub(super) use crate::medium::Medium;
pub(super) use bitvec::prelude::*;
pub(super) use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
pub(super) use std::sync::Arc;

/// Owns the temp directory and the VHDX file, ensuring cleanup on drop.
pub(super) struct TestContext {
    pub(super) _dir: tempfile::TempDir,
    pub(super) file: Medium,
    pub(super) overlay: Option<Arc<ReplayOverlay>>,
}

impl TestContext {
    /// Create a new IO context borrowing the owned file.
    pub(super) fn io(&mut self) -> IO<'_> {
        let mut io = IO::new(&mut self.file).expect("create IO");
        io.overlay = self.overlay.clone();
        io
    }
}

pub(super) fn create_vhdx(path: &std::path::Path) -> crate::medium::CreateOptions<std::fs::File> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("prepare caller-owned create medium");
    Medium::create(inner)
}

pub(super) fn open_vhdx(path: &std::path::Path) -> Medium {
    let inner = std::fs::File::open(path).expect("open caller-owned medium");
    Medium::open(inner).finish().expect("open vhdx")
}

pub(super) fn open_vhdx_writable(path: &std::path::Path) -> Medium {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open writable caller-owned medium");
    Medium::open(inner).write().finish().expect("open writable")
}

/// Helper: create a small dynamic VHDX and return an IO for it.
///
/// Uses `Medium::create` to produce a known-good test file with
/// `block_size=32MB`, `logical_sector_size=4096`.
pub(super) fn create_test_io() -> TestContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.vhdx");

    create_vhdx(&path)
            .size(256 * u64::from(MIB)) // 256 MB virtual
            .block_size(32 * MIB)
            .logical_sector_size(4096)
            .finish()
            .expect("create test vhdx");

    let file = open_vhdx(&path);

    TestContext {
        _dir: dir,
        file,
        overlay: None,
    }
}
