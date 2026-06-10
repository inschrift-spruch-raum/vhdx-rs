use crate::constants::{HEADER_BUFFER_SIZE, MIB};
use crate::error::Error;
use crate::{
    CreateOptions, Len, LogReplayPolicy, Medium, ReadSemanticsPolicy, Result, SetLen, SyncData,
};
use std::io::Write;

/// Create a tempdir under `target/test/`, copy a reference file from misc/,
/// return (`TempDir`, `PathBuf`). The `TempDir` keeps the copy alive.
fn ref_to_tmp(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = std::path::Path::new("target").join("test");
    let _ = std::fs::create_dir_all(&root);
    let dir = tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(&root)
        .expect("tempdir");
    let src = format!("misc/{name}");
    let dst = dir.path().join(name);
    std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {src}: {e}"));
    (dir, dst)
}

fn open_test_medium(path: impl AsRef<std::path::Path>) -> Result<Medium> {
    let inner = std::fs::File::open(path)?;
    Medium::open(inner).finish()
}

fn create_test_medium(path: impl AsRef<std::path::Path>) -> CreateOptions<std::fs::File> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("prepare caller-owned create medium");
    Medium::create(inner)
}

// -- Open tests ---------------------------------------------------------

#[test]
fn open_void_vhdx() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let f = open_test_medium(&path);
    assert!(f.is_ok(), "failed to open test-void.vhdx: {:?}", f.err());
    let f = f.unwrap();
    assert!(!f.is_write());
    assert!(f.is_strict());
    assert_eq!(f.log_replay_policy(), LogReplayPolicy::Require);
}

#[test]
fn medium_open_accepts_caller_owned_std_file() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let inner = std::fs::File::open(&path).expect("open fixture medium");

    let f = Medium::open(inner).finish();

    assert!(
        f.is_ok(),
        "failed to open caller-owned medium: {:?}",
        f.err()
    );
    let f = f.unwrap();
    assert!(!f.is_write());
    assert!(f.is_strict());
    assert_eq!(f.log_replay_policy(), LogReplayPolicy::Require);
}

#[test]
fn open_options_builder_write() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("open writable fixture medium");
    let f = Medium::open(inner).write().finish();
    assert!(f.is_ok());
    let f = f.unwrap();
    assert!(f.is_write());
}

#[test]
fn open_options_builder_log_replay() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let inner = std::fs::File::open(&path).expect("open fixture medium");
    let f = Medium::open(inner)
        .log_replay(LogReplayPolicy::Auto)
        .finish();
    assert!(f.is_ok());
    assert_eq!(f.unwrap().log_replay_policy(), LogReplayPolicy::Auto);
}

#[test]
fn open_options_builder_non_strict() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let inner = std::fs::File::open(&path).expect("open fixture medium");
    let f = Medium::open(inner).strict(false).finish();
    assert!(f.is_ok());
    assert!(!f.unwrap().is_strict());
}

#[test]
fn open_nonexistent_file() {
    let f = std::fs::File::open("misc/does-not-exist.vhdx")
        .map(Medium::open)
        .and_then(|options| options.finish().map_err(std::io::Error::from));
    assert!(f.is_err());
}

#[test]
fn open_invalid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.vhdx");
    {
        let mut tmp = std::fs::File::create(&path).unwrap();
        tmp.write_all(b"not a vhdx file at all").unwrap();
        tmp.set_len(HEADER_BUFFER_SIZE as u64).unwrap();
    }

    let inner = std::fs::File::open(&path).expect("open invalid medium");
    let f = Medium::open(inner).finish();
    assert!(f.is_err());
    assert!(matches!(f.unwrap_err(), Error::InvalidSignature { .. }));
}

#[test]
fn open_rejects_header_section_shorter_than_one_mib() {
    let cursor = std::io::Cursor::new(b"vhdxfile".to_vec());

    let result = Medium::open(cursor).finish();

    let err = result.expect_err("short header section must fail");
    assert!(
        matches!(err, Error::InvalidFile(ref message) if message.contains("header section too small")),
        "expected header section too small error, got {err:?}"
    );
}

#[test]
fn log_replay_default_is_require() {
    assert_eq!(LogReplayPolicy::default(), LogReplayPolicy::Require);
}

#[test]
fn read_semantics_default() {
    assert_eq!(
        ReadSemanticsPolicy::default(),
        ReadSemanticsPolicy::EffectiveDataPreferred
    );
}

#[test]
fn medium_capability_traits_cover_std_file_and_cursor_vec() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("medium-capabilities.bin");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("open capability file");

    file.set_len(7).expect("std file SetLen works");
    assert_eq!(Len::len(&mut file).expect("std file Len works"), 7);
    file.sync_data().expect("std file SyncData works");

    let mut cursor = std::io::Cursor::new(vec![1, 2, 3]);
    assert_eq!(Len::len(&mut cursor).expect("cursor Len works"), 3);
    cursor.set_len(5).expect("cursor SetLen grows");
    assert_eq!(cursor.get_ref(), &[1, 2, 3, 0, 0]);
    cursor.set_len(2).expect("cursor SetLen shrinks");
    assert_eq!(cursor.get_ref(), &[1, 2]);
    cursor.sync_data().expect("cursor SyncData no-op works");
}

#[test]
fn create_options_finish_signature_names_len_capability() {
    let source = include_str!("../create.rs");
    let finish_signature = source
        .split("pub fn finish(mut self) -> Result<Medium<T>>")
        .nth(1)
        .and_then(|rest| rest.split('{').next())
        .expect("CreateOptions::finish signature should be present");

    assert!(
        finish_signature.contains("+ Len +"),
        "CreateOptions::finish must explicitly require Len per medium standard"
    );
}

#[test]
fn fixed_offset_io_paths_use_shared_helpers() {
    let core_source = include_str!("../core.rs");
    let open_source = include_str!("../open.rs");
    let create_source = include_str!("../create.rs");
    let log_replay_source = include_str!("../../log_replay/core.rs");

    assert!(
        core_source.contains("fn read_exact_at<T>(inner: &mut T, offset: u64, buf: &mut [u8])"),
        "read_exact_at signature must be ordered as (inner, offset, buf) per medium standard"
    );
    assert!(
        core_source.contains("fn write_all_at<T>(inner: &mut T, offset: u64, buf: &[u8])"),
        "write_all_at signature must be ordered as (inner, offset, buf) per medium standard"
    );

    for (path, source) in [
        ("src/medium/open.rs", open_source),
        ("src/medium/create.rs", create_source),
        ("src/log_replay/core.rs", log_replay_source),
    ] {
        assert!(
            !source.contains("Seek::seek(")
                && !source.contains(".seek(SeekFrom::Start")
                && !source.contains(".write_all("),
            "{path} must route fixed-offset I/O through read_exact_at/write_all_at"
        );
    }
}

#[test]
fn medium_create_accepts_caller_owned_cursor_and_returns_inner() {
    let cursor = std::io::Cursor::new(Vec::new());

    let medium = Medium::create(cursor)
        .size(1024 * 1024 * 1024)
        .finish()
        .expect("create VHDX in caller-owned cursor");

    assert!(medium.get_ref().get_ref().len() > 1024 * 1024);
    let inner = medium.into_inner();
    let data = inner.into_inner();
    assert_eq!(&data[0..8], b"vhdxfile");
}

#[test]
fn write_bat_entry_invalidates_validator_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("validator-cache.vhdx");
    let _created = create_test_medium(&path)
        .size(256 * 1024 * 1024)
        .block_size(32 * MIB)
        .finish()
        .expect("create test medium");
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("open writable medium");
    let mut medium = Medium::open(inner)
        .write()
        .finish()
        .expect("open writable medium");

    let _ = medium.validator_buf().expect("populate validator cache");
    assert!(
        medium.validator_buf.read().unwrap().is_some(),
        "validator cache should be populated before BAT write"
    );

    medium
        .write_bat_entry(0, [0u8; 8])
        .expect("write BAT entry");

    assert!(
        medium.validator_buf.read().unwrap().is_none(),
        "BAT writes must invalidate validator cache"
    );
}
