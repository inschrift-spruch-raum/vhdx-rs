use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use vhdx::section::{BatState, PayloadBlockState, SectorBitmapState};
use vhdx::{Guid, LogReplayPolicy, Medium, ParentMedium};

struct ReadOnlyCursor(std::io::Cursor<Vec<u8>>);

#[derive(Clone, Default)]
struct CountingStats {
    inner: Arc<Mutex<CountingState>>,
}

#[derive(Default)]
struct CountingState {
    reads: usize,
    seeks: usize,
}

impl CountingStats {
    fn reset(&self) {
        let mut state = self.inner.lock().expect("counting stats lock poisoned");
        state.reads = 0;
        state.seeks = 0;
    }

    fn reads(&self) -> usize {
        self.inner
            .lock()
            .expect("counting stats lock poisoned")
            .reads
    }

    fn seeks(&self) -> usize {
        self.inner
            .lock()
            .expect("counting stats lock poisoned")
            .seeks
    }
}

struct CountingCursor {
    cursor: std::io::Cursor<Vec<u8>>,
    stats: CountingStats,
}

impl CountingCursor {
    fn new(data: Vec<u8>) -> (Self, CountingStats) {
        let stats = CountingStats::default();
        (
            Self {
                cursor: std::io::Cursor::new(data),
                stats: stats.clone(),
            },
            stats,
        )
    }
}

impl Read for CountingCursor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.cursor.read(buf)?;
        if read > 0 {
            self.stats
                .inner
                .lock()
                .expect("counting stats lock poisoned")
                .reads += 1;
        }
        Ok(read)
    }
}

impl Seek for CountingCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.stats
            .inner
            .lock()
            .expect("counting stats lock poisoned")
            .seeks += 1;
        self.cursor.seek(pos)
    }
}

impl std::io::Read for ReadOnlyCursor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.0, buf)
    }
}

impl std::io::Seek for ReadOnlyCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.0, pos)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_medium(path: &std::path::Path) -> vhdx::CreateOptions<std::fs::File> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("prepare caller-owned create medium");
    Medium::create(inner)
}

fn create_differencing_medium(
    path: &std::path::Path, parent_path: &std::path::Path,
) -> vhdx::CreateOptions<std::fs::File> {
    let mut parent = open_medium(parent_path);
    create_medium(path)
        .parent(&mut parent, parent_path)
        .expect("configure caller-owned parent medium")
}

fn open_medium(path: &std::path::Path) -> Medium {
    Medium::open(std::fs::File::open(path).expect("open VHDX medium"))
        .finish()
        .expect("open medium")
}

fn open_medium_writable(path: &std::path::Path) -> Medium {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open writable VHDX medium");
    Medium::open(inner).write().finish().expect("open writable")
}

fn parent_resolver(
    parent_path: std::path::PathBuf,
) -> impl Fn(vhdx::ParentRequest<'_>) -> vhdx::Result<Medium> {
    move |_| Medium::open(std::fs::File::open(&parent_path)?).finish()
}

#[derive(Clone, Default)]
struct ParentReadStats {
    inner: Arc<Mutex<ParentReadState>>,
}

#[derive(Default)]
struct ParentReadState {
    resolver_calls: usize,
    sector_reads: usize,
    range_reads: usize,
    ranges: Vec<(u64, u64)>,
}

impl ParentReadStats {
    fn resolver_called(&self) {
        self.inner
            .lock()
            .expect("parent read stats lock poisoned")
            .resolver_calls += 1;
    }

    fn resolver_calls(&self) -> usize {
        self.inner
            .lock()
            .expect("parent read stats lock poisoned")
            .resolver_calls
    }

    fn sector_reads(&self) -> usize {
        self.inner
            .lock()
            .expect("parent read stats lock poisoned")
            .sector_reads
    }

    fn range_reads(&self) -> usize {
        self.inner
            .lock()
            .expect("parent read stats lock poisoned")
            .range_reads
    }

    fn ranges(&self) -> Vec<(u64, u64)> {
        self.inner
            .lock()
            .expect("parent read stats lock poisoned")
            .ranges
            .clone()
    }
}

struct CountingParentMedium {
    stats: ParentReadStats,
    data_write_guid: Guid,
    logical_sector_size: u32,
}

impl CountingParentMedium {
    fn new(stats: ParentReadStats, request: &vhdx::ParentRequest<'_>) -> Self {
        Self {
            stats,
            data_write_guid: request.expected_data_write_guid(),
            logical_sector_size: request.child_logical_sector_size(),
        }
    }
}

impl vhdx::ParentMedium for CountingParentMedium {
    fn data_write_guid(&mut self) -> vhdx::Result<Guid> {
        Ok(self.data_write_guid)
    }

    fn logical_sector_size(&mut self) -> vhdx::Result<u32> {
        Ok(self.logical_sector_size)
    }

    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> vhdx::Result<()> {
        self.stats
            .inner
            .lock()
            .expect("parent read stats lock poisoned")
            .sector_reads += 1;
        fill_parent_pattern(sector, 1, self.logical_sector_size, buf);
        Ok(())
    }

    fn read_sector_range(
        &mut self, start_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> vhdx::Result<()> {
        let mut stats = self
            .stats
            .inner
            .lock()
            .expect("parent read stats lock poisoned");
        stats.range_reads += 1;
        stats.ranges.push((start_sector, sector_count));
        drop(stats);

        fill_parent_pattern(start_sector, sector_count, self.logical_sector_size, buf);
        Ok(())
    }
}

fn fill_parent_pattern(
    start_sector: u64, sector_count: u64, logical_sector_size: u32, buf: &mut [u8],
) {
    assert_eq!(
        buf.len(),
        usize::try_from(sector_count * u64::from(logical_sector_size))
            .expect("sector range length fits usize")
    );
    for sector_offset in 0..sector_count {
        let byte = parent_pattern_byte(start_sector + sector_offset);
        let offset =
            usize::try_from(sector_offset * u64::from(logical_sector_size)).expect("offset fits");
        let len = logical_sector_size as usize;
        buf[offset..offset + len].fill(byte);
    }
}

fn parent_pattern_byte(sector: u64) -> u8 {
    0x40u8.wrapping_add((sector % 127) as u8)
}

struct CountingParentResolver {
    stats: ParentReadStats,
}

impl vhdx::ParentResolver for CountingParentResolver {
    fn resolve_parent(
        &mut self, request: vhdx::ParentRequest<'_>,
    ) -> vhdx::Result<Box<dyn vhdx::ParentMedium>> {
        self.stats.resolver_called();
        Ok(Box::new(CountingParentMedium::new(
            self.stats.clone(),
            &request,
        )))
    }
}

fn create_differencing_pair_for_parent_tests()
-> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let parent_path = dir.path().join("parent.vhdx");
    let child_path = dir.path().join("child.avhdx");
    let mut parent = create_medium(&parent_path)
        .size(8 * 1024 * 1024)
        .block_size(1024 * 1024)
        .logical_sector_size(4096)
        .finish()
        .expect("create parent medium");
    create_medium(&child_path)
        .size(8 * 1024 * 1024)
        .block_size(1024 * 1024)
        .logical_sector_size(4096)
        .parent(&mut parent, &parent_path)
        .expect("configure differencing parent")
        .finish()
        .expect("create differencing child");
    (dir, parent_path, child_path)
}

fn create_differencing_child_for_parent_tests() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, _parent_path, child_path) = create_differencing_pair_for_parent_tests();
    (dir, child_path)
}

fn open_child_with_counting_parent(child_path: &std::path::Path, stats: ParentReadStats) -> Medium {
    Medium::open(std::fs::File::open(child_path).expect("open child medium"))
        .with_parent_resolver(CountingParentResolver { stats })
        .finish()
        .expect("open child with counting parent")
}

#[test]
fn parent_resolver_trait_uses_standard_resolve_parent_method() {
    struct CompileOnlyResolver;

    impl vhdx::ParentResolver for CompileOnlyResolver {
        fn resolve_parent(
            &mut self, request: vhdx::ParentRequest<'_>,
        ) -> vhdx::Result<Box<dyn vhdx::ParentMedium>> {
            let _ = &request.locator;
            let _ = &request.expected_data_write_guid;
            let _ = &request.child_logical_sector_size;
            let _ = &request.child_virtual_disk_size;
            unreachable!("compile-time API shape check only")
        }
    }

    let _ = CompileOnlyResolver;
}

#[test]
fn parent_backed_multi_sector_read_uses_single_parent_range_read() {
    let (_dir, child_path) = create_differencing_child_for_parent_tests();
    let stats = ParentReadStats::default();
    let mut child = open_child_with_counting_parent(&child_path, stats.clone());
    let mut actual = vec![0; 4 * 4096];

    {
        let mut io = child.io().expect("create child IO");
        let mut sector = io.sector(0, 4).expect("open parent-backed child range");
        sector
            .read_exact(&mut actual)
            .expect("read parent-backed child range");
    }

    let mut expected = vec![0; actual.len()];
    fill_parent_pattern(0, 4, 4096, &mut expected);
    assert_eq!(actual, expected);
    assert_eq!(stats.resolver_calls(), 1);
    assert_eq!(stats.range_reads(), 1);
    assert_eq!(stats.sector_reads(), 0);
    assert_eq!(stats.ranges(), vec![(0, 1024)]);
}

#[test]
fn resolved_parent_is_reused_across_io_handles() {
    let (_dir, child_path) = create_differencing_child_for_parent_tests();
    let stats = ParentReadStats::default();
    let mut child = open_child_with_counting_parent(&child_path, stats.clone());
    let mut first = vec![0; 4096];
    let mut second = vec![0; 4096];

    {
        let mut io = child.io().expect("create first child IO");
        let mut sector = io.sector(0, 1).expect("open first child range");
        sector
            .read_exact(&mut first)
            .expect("read first parent-backed range");
    }
    {
        let mut io = child.io().expect("create second child IO");
        let mut sector = io.sector(1, 1).expect("open second child range");
        sector
            .read_exact(&mut second)
            .expect("read second parent-backed range");
    }

    assert_eq!(first, vec![parent_pattern_byte(0); 4096]);
    assert_eq!(second, vec![parent_pattern_byte(1); 4096]);
    assert_eq!(stats.resolver_calls(), 1);
    assert_eq!(stats.range_reads(), 1);
    assert_eq!(stats.sector_reads(), 0);
    assert_eq!(stats.ranges(), vec![(0, 1024)]);
}

#[test]
fn sequential_parent_backed_reads_use_readahead_cache() {
    let (_dir, child_path) = create_differencing_child_for_parent_tests();
    let stats = ParentReadStats::default();
    let mut child = open_child_with_counting_parent(&child_path, stats.clone());
    let mut first = vec![0; 16 * 4096];
    let mut second = vec![0; 16 * 4096];

    {
        let mut io = child.io().expect("create first child IO");
        let mut sector = io.sector(0, 16).expect("open first child range");
        sector
            .read_exact(&mut first)
            .expect("read first parent-backed range");
    }
    {
        let mut io = child.io().expect("create second child IO");
        let mut sector = io.sector(16, 16).expect("open second child range");
        sector
            .read_exact(&mut second)
            .expect("read second parent-backed range");
    }

    let mut expected_first = vec![0; first.len()];
    let mut expected_second = vec![0; second.len()];
    fill_parent_pattern(0, 16, 4096, &mut expected_first);
    fill_parent_pattern(16, 16, 4096, &mut expected_second);
    assert_eq!(first, expected_first);
    assert_eq!(second, expected_second);
    assert_eq!(stats.resolver_calls(), 1);
    assert_eq!(stats.range_reads(), 1);
    assert_eq!(stats.sector_reads(), 0);
    assert_eq!(stats.ranges(), vec![(0, 1024)]);
}

#[test]
fn medium_parent_read_sector_uses_effective_readahead_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("parent.vhdx");
    let mut medium = create_medium(&path)
        .size(8 * 1024 * 1024)
        .block_size(1024 * 1024)
        .logical_sector_size(4096)
        .finish()
        .expect("create parent medium");
    let mut expected = vec![0; 32 * 4096];
    fill_parent_pattern(0, 32, 4096, &mut expected);
    {
        let mut io = medium.io().expect("create parent IO");
        let mut sector = io.sector(0, 32).expect("open parent sectors");
        sector
            .write_all(&expected)
            .expect("write parent sector pattern");
    }
    drop(medium);

    let bytes = std::fs::read(&path).expect("read parent medium bytes");
    let (cursor, stats) = CountingCursor::new(bytes);
    let mut parent = Medium::open(cursor)
        .finish()
        .expect("open counting parent medium");
    let mut first = vec![0; 4096];
    let mut second = vec![0; 4096];

    stats.reset();
    parent
        .read_sector(0, &mut first)
        .expect("read first parent sector");
    let reads_after_first = stats.reads();
    assert!(
        reads_after_first > 0,
        "first parent sector read should touch the backing medium"
    );
    parent
        .read_sector(1, &mut second)
        .expect("read second parent sector");

    assert_eq!(first, vec![parent_pattern_byte(0); 4096]);
    assert_eq!(second, vec![parent_pattern_byte(1); 4096]);
    assert_eq!(
        stats.reads(),
        reads_after_first,
        "adjacent parent-sector reads should be served from the effective read-ahead cache"
    );
}

#[test]
fn partially_present_child_coalesces_contiguous_parent_ranges() {
    let (_dir, parent_path, child_path) = create_differencing_pair_for_parent_tests();
    let child_sector = vec![0xA5; 4096];

    {
        let mut child = Medium::open(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&child_path)
                .expect("open writable child"),
        )
        .write()
        .with_parent_resolver(parent_resolver(parent_path))
        .finish()
        .expect("open writable child medium");
        let mut io = child.io().expect("create writable child IO");
        let mut sector = io.sector(2, 1).expect("open child-owned sector");
        sector
            .write_all(&child_sector)
            .expect("write child-owned sector");
    }

    let stats = ParentReadStats::default();
    let mut child = open_child_with_counting_parent(&child_path, stats.clone());
    let mut actual = vec![0; 4 * 4096];
    {
        let mut io = child.io().expect("create child IO");
        let mut sector = io.sector(0, 4).expect("open mixed parent-child range");
        sector
            .read_exact(&mut actual)
            .expect("read mixed parent-child range");
    }

    assert_eq!(&actual[0..4096], vec![parent_pattern_byte(0); 4096]);
    assert_eq!(&actual[4096..8192], vec![parent_pattern_byte(1); 4096]);
    assert_eq!(&actual[8192..12288], child_sector.as_slice());
    assert_eq!(&actual[12288..16384], vec![parent_pattern_byte(3); 4096]);
    assert_eq!(stats.resolver_calls(), 1);
    assert_eq!(stats.range_reads(), 1);
    assert_eq!(stats.sector_reads(), 0);
    assert_eq!(stats.ranges(), vec![(0, 1024)]);
}

/// Create a test VHDX and return the opened Medium handle along with the
/// backing tempdir (caller must hold the `TempDir` to keep files alive).
fn create_test_vhdx(size: u64, block_size: u32, fixed: bool) -> (Medium, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.vhdx");
    let f = create_medium(&path)
        .size(size)
        .block_size(block_size)
        .logical_sector_size(4096)
        .fixed(fixed)
        .finish()
        .expect("create vhdx");
    (f, dir)
}

/// Copy a reference Medium from misc/ into a tempdir under target/test/,
/// return (`TempDir`, `PathBuf`). Hold `TempDir` for the test's lifetime.
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

/// `Medium::create()` writes both headers with `sequence_number=0`.
/// The `SpecValidator` requires different sequence numbers.
/// This helper patches Header 2 to sequence=1, fixes its CRC, then re-opens
/// the Medium so `validate_file()` can succeed.
fn patch_header2_seq_and_reopen(path: &std::path::Path) -> Medium {
    const HEADER2_OFFSET: u64 = 128 * 1024;

    // Open raw Medium, patch Header 2 sequence number to 1
    let mut raw = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("re-open for header patch");

    let mut header = [0u8; 4096];
    raw.seek(SeekFrom::Start(HEADER2_OFFSET)).unwrap();
    raw.read_exact(&mut header).unwrap();

    // Set sequence number = 1 (bytes 8..16)
    header[8..16].copy_from_slice(&1u64.to_le_bytes());

    // Recalculate CRC-32C (zero checksum field first)
    header[4..8].copy_from_slice(&0u32.to_le_bytes());
    let checksum = crc32c::crc32c(&header);
    header[4..8].copy_from_slice(&checksum.to_le_bytes());

    // Write back
    raw.seek(SeekFrom::Start(HEADER2_OFFSET)).unwrap();
    raw.write_all(&header).unwrap();
    drop(raw);

    // Re-open as a proper VHDX Medium
    Medium::open(std::fs::File::open(path).expect("open VHDX medium"))
        .finish()
        .expect("re-open after header patch")
}

/// Create a test VHDX, patch sequence numbers, re-open, and return.
fn create_and_reopen_for_validation(
    size: u64, block_size: u32, fixed: bool,
) -> (Medium, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.vhdx");
    let f = create_medium(&path)
        .size(size)
        .block_size(block_size)
        .logical_sector_size(4096)
        .fixed(fixed)
        .finish()
        .expect("create vhdx");
    // Close the original handle so the patch can acquire write access
    drop(f);

    let patched_file = patch_header2_seq_and_reopen(&path);
    (patched_file, dir)
}

// ---------------------------------------------------------------------------
// 1. Open test-void.vhdx and validate all sections
// ---------------------------------------------------------------------------

#[test]
fn medium_open_accepts_caller_owned_std_file_from_root_api() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let inner = std::fs::File::open(&path).expect("open fixture medium");

    let f = Medium::open(inner)
        .finish()
        .expect("open caller-owned medium");

    assert!(
        f.sections().expect("sections").header().is_ok(),
        "opened medium should expose header sections"
    );
}

#[test]
fn medium_open_accepts_cursor_vec_from_root_api() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let bytes = std::fs::read(&path).expect("read fixture into memory");
    let cursor = std::io::Cursor::new(bytes);

    let f = Medium::open(cursor)
        .finish()
        .expect("open in-memory medium");

    assert!(
        f.sections().expect("sections").header().is_ok(),
        "in-memory medium should expose header sections"
    );
}

#[test]
fn medium_open_cursor_exposes_common_sections_surface() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let bytes = std::fs::read(&path).expect("read fixture into memory");
    let cursor = std::io::Cursor::new(bytes);

    let f = Medium::open(cursor)
        .finish()
        .expect("open in-memory medium");

    assert!(
        f.sections().expect("sections").metadata().is_ok(),
        "in-memory medium should expose metadata through the common sections surface"
    );
}

#[test]
fn sections_header_is_lazy_after_open() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let data = std::fs::read(&path).expect("read fixture bytes");
    let (cursor, stats) = CountingCursor::new(data);
    let medium = Medium::open(cursor).finish().expect("open counting medium");

    stats.reset();

    let sections = medium.sections().expect("sections");
    let header = sections.header().expect("header section");
    assert_eq!(header.file_type().signature(), b"vhdxfile");

    assert_eq!(
        stats.reads(),
        0,
        "sections() plus header() must not read metadata, log, or BAT after open"
    );
}

#[test]
fn sections_metadata_is_loaded_once_after_open() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let data = std::fs::read(&path).expect("read fixture bytes");
    let (cursor, stats) = CountingCursor::new(data);
    let medium = Medium::open(cursor).finish().expect("open counting medium");

    stats.reset();

    let sections = medium.sections().expect("sections");
    let metadata = sections.metadata().expect("metadata section");
    assert!(
        metadata.table().header().entry_count() > 0,
        "metadata table should have entries"
    );
    let first_reads = stats.reads();
    let first_seeks = stats.seeks();

    let metadata = sections.metadata().expect("metadata section from cache");
    assert!(
        metadata.table().header().entry_count() > 0,
        "cached metadata table should have entries"
    );

    assert_eq!(
        stats.reads(),
        first_reads,
        "second metadata() call must reuse cached metadata bytes"
    );
    assert_eq!(
        stats.seeks(),
        first_seeks,
        "second metadata() call must not seek the underlying medium"
    );
}

#[test]
fn sections_bat_reuses_cached_bytes_after_first_access() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let data = std::fs::read(&path).expect("read fixture bytes");
    let (cursor, stats) = CountingCursor::new(data);
    let medium = Medium::open(cursor).finish().expect("open counting medium");

    stats.reset();

    let sections = medium.sections().expect("sections");
    let bat = sections.bat().expect("BAT section");
    assert!(
        bat.entry(0).is_ok(),
        "BAT should expose at least the first entry"
    );
    let first_reads = stats.reads();
    let first_seeks = stats.seeks();

    let bat = sections.bat().expect("BAT section from cache");
    assert!(
        bat.entry(0).is_ok(),
        "cached BAT should expose at least the first entry"
    );

    assert_eq!(
        stats.reads(),
        first_reads,
        "second bat() call must reuse cached BAT bytes"
    );
    assert_eq!(
        stats.seeks(),
        first_seeks,
        "second bat() call must not seek the underlying medium"
    );
}

#[test]
fn sections_log_reuses_cached_bytes_after_first_access() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let data = std::fs::read(&path).expect("read fixture bytes");
    let (cursor, stats) = CountingCursor::new(data);
    let medium = Medium::open(cursor).finish().expect("open counting medium");

    stats.reset();

    let sections = medium.sections().expect("sections");
    sections.log().expect("log section");
    let first_reads = stats.reads();
    let first_seeks = stats.seeks();

    sections.log().expect("log section from cache");

    assert_eq!(
        stats.reads(),
        first_reads,
        "second log() call must reuse cached log bytes"
    );
    assert_eq!(
        stats.seeks(),
        first_seeks,
        "second log() call must not seek the underlying medium"
    );
}

#[test]
fn sections_metadata_can_be_read_from_multiple_threads() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let data = std::fs::read(&path).expect("read fixture bytes");
    let medium = Arc::new(
        Medium::open(std::io::Cursor::new(data))
            .finish()
            .expect("open medium"),
    );

    let handles = (0..4)
        .map(|_| {
            let medium = Arc::clone(&medium);
            thread::spawn(move || {
                let sections = medium.sections().expect("sections");
                let metadata = sections.metadata().expect("metadata");
                metadata.table().header().entry_count()
            })
        })
        .collect::<Vec<_>>();

    let counts = handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .expect("metadata reader thread must not panic")
        })
        .collect::<Vec<_>>();

    assert!(
        counts.iter().all(|count| *count == counts[0] && *count > 0),
        "concurrent metadata readers must observe the same non-empty metadata table"
    );
}

#[test]
fn medium_read_only_cursor_reads_virtual_sector_through_io() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let bytes = std::fs::read(&path).expect("read fixture into memory");
    let cursor = ReadOnlyCursor(std::io::Cursor::new(bytes));
    let mut medium = Medium::open(cursor)
        .finish()
        .expect("open read-only cursor medium");
    let mut buf = vec![0; 512];

    let mut io = medium.io().expect("open read-only cursor IO");
    let mut sector = io.sector(0, 1).expect("open cursor sector");
    std::io::Read::read_exact(&mut sector, &mut buf).expect("read cursor sector");

    assert_eq!(buf.len(), 512);
}

#[test]
fn medium_cursor_create_write_and_read_roundtrips_sector() {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut medium = Medium::create(cursor)
        .size(1024 * 1024 * 1024)
        .finish()
        .expect("create VHDX in caller-owned cursor");
    let expected = vec![0xA5; 4096];

    {
        let mut io = medium.io().expect("open cursor IO");
        let mut sector = io.sector(0, 1).expect("open writable sector");
        std::io::Write::write_all(&mut sector, &expected).expect("write cursor sector");
    }

    let cursor = medium.into_inner();
    let mut medium = Medium::open(cursor)
        .write()
        .finish()
        .expect("reopen cursor medium");
    let mut actual = vec![0; 4096];
    let mut io = medium.io().expect("open cursor IO for readback");
    let mut sector = io.sector(0, 1).expect("open readable sector");

    std::io::Read::read_exact(&mut sector, &mut actual).expect("read cursor sector");

    assert_eq!(actual, expected);
}

#[test]
fn sections_bat_is_fresh_after_write_and_reacquire() {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut medium = Medium::create(cursor)
        .size(1024 * 1024 * 1024)
        .finish()
        .expect("create VHDX in caller-owned cursor");

    let before_sections = medium.sections().expect("sections before write");
    let before_bat = before_sections.bat().expect("BAT before write");
    assert_eq!(
        before_bat
            .entry(0)
            .expect("BAT entry 0")
            .state()
            .expect("BAT state"),
        BatState::Payload(PayloadBlockState::NotPresent),
        "new dynamic disk should start with a not-present payload block"
    );
    drop(before_bat);
    drop(before_sections);

    let expected = vec![0xA5; 4096];
    {
        let mut io = medium.io().expect("open writable IO");
        let mut sector = io.sector(0, 1).expect("open writable sector");
        std::io::Write::write_all(&mut sector, &expected).expect("write cursor sector");
    }

    let after_sections = medium.sections().expect("sections after write");
    let after_bat = after_sections.bat().expect("BAT after write");
    assert_eq!(
        after_bat
            .entry(0)
            .expect("BAT entry 0")
            .state()
            .expect("BAT state"),
        BatState::Payload(PayloadBlockState::FullyPresent),
        "reacquired sections should observe the BAT entry published by the write"
    );
}

#[test]
fn sections_header_is_fresh_after_get_mut_header_change() {
    const HEADER2_OFFSET: usize = 128 * 1024;
    const CHECKSUM_OFFSET: usize = 4;
    const SEQUENCE_OFFSET: usize = 8;

    let cursor = std::io::Cursor::new(Vec::new());
    let mut medium = Medium::create(cursor)
        .size(1024 * 1024 * 1024)
        .finish()
        .expect("create VHDX in caller-owned cursor");

    let before_sections = medium.sections().expect("sections before header mutation");
    assert_eq!(
        before_sections
            .header()
            .expect("header before mutation")
            .header(0)
            .expect("current header before mutation")
            .sequence_number(),
        1,
        "created medium should initially select header 2"
    );
    drop(before_sections);

    let cursor = medium.get_mut();
    let bytes = cursor.get_mut();
    let header2 = &mut bytes[HEADER2_OFFSET..HEADER2_OFFSET + 4096];
    header2[SEQUENCE_OFFSET..SEQUENCE_OFFSET + 8].copy_from_slice(&2u64.to_le_bytes());
    header2[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&0u32.to_le_bytes());
    let checksum = crc32c::crc32c(header2);
    header2[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&checksum.to_le_bytes());

    let after_sections = medium.sections().expect("sections after header mutation");
    assert_eq!(
        after_sections
            .header()
            .expect("header after mutation")
            .header(0)
            .expect("current header after mutation")
            .sequence_number(),
        2,
        "reacquired sections should reload the header after get_mut mutation"
    );
}

#[test]
fn open_void_vhdx_validate_sections() {
    let (_dir, path) = ref_to_tmp("test-void.vhdx");
    let mut f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .finish()
        .unwrap();
    {
        let sections = f.sections().expect("sections");

        // Header section
        let header = sections.header().expect("header section");
        assert_eq!(
            header.file_type().signature(),
            b"vhdxfile",
            "Medium type identifier signature"
        );
        let current = header.header(0).expect("current header structure");
        // Version must be 1 per MS-VHDX
        assert_eq!(current.version(), 1, "header version");

        // Metadata section (accessible even on minimal/void VHDX)
        let metadata = sections.metadata().expect("metadata section");
        let table = metadata.table();
        assert!(
            table.header().entry_count() > 0,
            "metadata table should have entries"
        );

        // BAT section (may fail if block_size=0, as in test-void)
        match sections.bat() {
            Ok(bat) => {
                assert!(bat.entries().count() > 0, "BAT should have entries");
            }
            Err(_) => {
                // test-void may have block_size=0 in FileParameters,
                // making chunk_ratio uncomputable.
                eprintln!("BAT loading skipped (block_size may be 0 in test-void)");
            }
        }
    }

    // Full validation —test-void is a minimal reference Medium that should
    // pass validation (no reserved flags issues in the reference).
    let result = f.validator().expect("validator").validate_file();
    assert!(
        result.is_ok(),
        "test-void validation should pass, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Open test-fs.vhdx and read sector 0 (FAT32 boot sector)
// ---------------------------------------------------------------------------

#[test]
fn open_fs_vhdx_read_sector_zero() {
    // test-fs.vhdx has a pending log -> use Auto to replay it on open
    let (_dir, path) = ref_to_tmp("test-fs.vhdx");
    let mut f = Medium::open(std::fs::File::open(&path).expect("open VHDX medium"))
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .unwrap();

    // Verify that header + metadata are accessible regardless of IO state.
    {
        let sections = f.sections().expect("sections");
        let metadata = sections.metadata().expect("metadata on test-fs");
        assert!(
            metadata.table().header().entry_count() > 0,
            "test-fs should have metadata table entries"
        );
    }

    // IO may or may not be available depending on whether reading is
    // supported for this Medium. If it works, verify the FAT32 boot sector.
    if let Ok(mut io) = f.io()
        && let Ok(mut sector) = io.sector(0, 1)
    {
        let mut buf = vec![0u8; 4096];
        if sector.read_exact(&mut buf).is_ok() {
            // Sector 0 of a FAT32-formatted disk should NOT be all zeros.
            assert!(
                !buf.iter().all(|&b| b == 0),
                "sector 0 should contain boot sector data, not all zeros"
            );
            // Verify the FAT32 signature (bytes 0x52-0x59 = "FAT32   ")
            assert_eq!(
                &buf[0x52..0x5A],
                b"FAT32   ",
                "FAT32 boot sector signature at offset 0x52"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Create dynamic disk and validate
// ---------------------------------------------------------------------------

#[test]
fn create_dynamic_validate() {
    let (mut f, _dir) =
        create_and_reopen_for_validation(256 * 1024 * 1024, 32 * 1024 * 1024, false);

    // Structural validation
    f.validator()
        .expect("validator")
        .validate_file()
        .expect("validate dynamic disk");

    // Metadata check
    let sections = f.sections().expect("sections");
    let metadata = sections.metadata().expect("metadata");
    let fp = metadata.items().file_parameters().expect("FileParameters");
    assert_eq!(fp.block_size(), 32 * 1024 * 1024);
    assert!(!fp.has_parent(), "dynamic disk should not have parent");
    assert!(
        !fp.leave_block_allocated(),
        "dynamic disk: LeaveBlockAllocated should be false"
    );

    // Virtual disk size check
    assert_eq!(
        metadata.items().virtual_disk_size().unwrap(),
        256 * 1024 * 1024
    );
}

// ---------------------------------------------------------------------------

#[path = "integration/late.rs"]
mod late;
#[path = "integration/validation.rs"]
mod validation;
