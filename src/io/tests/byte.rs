use super::*;

// -----------------------------------------------------------------------
// Byte-level read/write tests (T4)
// -----------------------------------------------------------------------

/// Helper: create a small fixed VHDX opened in **read-only** mode.
fn create_fixed_test_io() -> TestContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-fixed-ro.vhdx");

    VhdxFile::create(&path)
        .size(4 * u64::from(MIB))
        .block_size(MIB)
        .logical_sector_size(4096)
        .fixed(true)
        .finish()
        .expect("create fixed test vhdx");

    let file = VhdxFile::open(&path).finish().expect("open read-only");

    TestContext {
        _dir: dir,
        file,
        overlay: None,
    }
}

// T4.1: byte_offset_zero_read_matches_full_sector
#[test]
fn byte_offset_zero_read_matches_full_sector() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    // Write pattern to sector 0
    let mut sw = io.sector(0, 1).expect("sector 0");
    sw.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw.write_all(&[0x42u8; SECTOR_SIZE as usize])
        .expect("write sector 0");

    // Read full sector via byte_offset=0
    let mut full_buf = vec![0u8; SECTOR_SIZE.into()];
    let mut sr = io.sector(0, 1).expect("sector 0");
    sr.seek(SeekFrom::Start(0)).expect("seek to 0");
    sr.read_exact(&mut full_buf).expect("read sector 0");
    assert_eq!(full_buf, [0x42u8; SECTOR_SIZE as usize]);

    // Read small slice via byte_offset=0
    let mut small_buf = [0u8; 10];
    let mut sr2 = io.sector(0, 1).expect("sector 0");
    sr2.seek(SeekFrom::Start(0)).expect("seek to 0");
    sr2.read_exact(&mut small_buf).expect("read 10 bytes");
    assert_eq!(small_buf, [0x42u8; 10]);

    // Both reads return same first 10 bytes
    assert_eq!(&full_buf[..10], &small_buf[..10]);
}

// T4.2: byte_offset_non_aligned_read_extracts_correct_bytes
#[test]
fn byte_offset_non_aligned_read_extracts_correct_bytes() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    let mut sw0 = io.sector(0, 1).expect("sector 0");
    sw0.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw0.write_all(&[0x11u8; SECTOR_SIZE as usize])
        .expect("write");

    // Read 100 bytes at offset 50
    let mut buf = [0u8; 100];
    let mut sr = io.sector(0, 1).expect("sector 0");
    sr.seek(SeekFrom::Start(50)).expect("seek to 50");
    sr.read_exact(&mut buf).expect("read at offset 50");
    assert_eq!(buf, [0x11u8; 100]);

    // Write 0x11 to sector 1 so the cross-sector read is consistent
    let mut sw1 = io.sector(1, 1).expect("sector 1");
    sw1.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw1.write_all(&[0x11u8; SECTOR_SIZE as usize])
        .expect("write sector 1");

    // Read 50 bytes at offset 4090 (crosses into sector 1)
    let mut sector = io.sector(0, 2).expect("sector(0,2)");
    let mut cross_buf = [0u8; 50];
    sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
    sector
        .read_exact(&mut cross_buf)
        .expect("read at offset 4090");
    assert_eq!(cross_buf, [0x11u8; 50]);
}

// T4.3: byte_offset_rmw_write_preserves_surrounding_bytes
#[test]
fn byte_offset_rmw_write_preserves_surrounding_bytes() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");

    // Write full sector 0 with 0xAA
    sector.seek(SeekFrom::Start(0)).expect("seek to 0");
    sector
        .write_all(&[0xAAu8; SECTOR_SIZE as usize])
        .expect("write full sector");

    // Write 10 bytes at offset 100
    sector.seek(SeekFrom::Start(100)).expect("seek to 100");
    sector
        .write_all(&[0xBBu8; 10])
        .expect("write 10 bytes at offset 100");

    // Read back full sector
    let mut full_buf = vec![0u8; SECTOR_SIZE.into()];
    sector.seek(SeekFrom::Start(0)).expect("seek to 0");
    sector.read_exact(&mut full_buf).expect("read full sector");

    // Verify before patch preserved
    assert_eq!(&full_buf[0..100], &[0xAAu8; 100], "before patch preserved");
    // Verify patch applied
    assert_eq!(&full_buf[100..110], &[0xBBu8; 10], "patch applied");
    // Verify after patch preserved
    assert_eq!(
        &full_buf[110..SECTOR_SIZE.into()],
        &[0xAAu8; 3986],
        "after patch preserved"
    );
}

// T4.4: byte_offset_cross_block_boundary_read
#[test]
fn byte_offset_cross_block_boundary_read() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    // 1 MB / 4096 = 256 sectors per block
    // sector(254, 4) spans block 0 (sectors 0-255) and block 1 (sectors 256-511)
    let mut sector = io.sector(254, 4).expect("sector(254,4)");

    // Write known pattern to all 4 sectors
    let mut pattern = Vec::with_capacity(4 * SECTOR_SIZE as usize);
    for i in 0..(4 * SECTOR_SIZE) {
        pattern.push(u8::try_from(i % 256).expect("modulo 256 fits u8"));
    }
    sector.seek(SeekFrom::Start(0)).expect("seek to 0");
    sector.write_all(&pattern).expect("write pattern");

    // Read a byte range that crosses the block boundary
    // Block boundary at byte offset: 2 * 4096 = 8192 (sector 256 is first of block 1)
    // Cross boundary: read 200 bytes starting at byte_offset = 8192 - 100 = 8092
    let mut buf = [0u8; 200];
    sector.seek(SeekFrom::Start(8092)).expect("seek to 8092");
    sector
        .read_exact(&mut buf)
        .expect("read crossing block boundary");
    assert_eq!(buf, &pattern[8092..8292], "cross-boundary read mismatch");
}

// T4.5: byte_offset_cross_block_boundary_write_rmw
#[test]
fn byte_offset_cross_block_boundary_write_rmw() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    // 1 MB / 4096 = 256 sectors per block
    let mut sector = io.sector(254, 4).expect("sector(254,4)");

    // Write initial data: all 0xDD
    let init = [0xDDu8; 4 * SECTOR_SIZE as usize];
    sector.seek(SeekFrom::Start(0)).expect("seek to 0");
    sector.write_all(&init).expect("write initial");

    // Write a small patch that crosses the block boundary
    // Block boundary at byte 8192; write starting at byte 8180, 30 bytes
    let patch = [0xEEu8; 30];
    sector.seek(SeekFrom::Start(8180)).expect("seek to 8180");
    sector
        .write_all(&patch)
        .expect("write cross-boundary patch");

    // Read back and verify
    let mut buf = vec![0u8; 4 * SECTOR_SIZE as usize];
    sector.seek(SeekFrom::Start(0)).expect("seek to 0");
    sector.read_exact(&mut buf).expect("read back");

    assert_eq!(&buf[0..8180], &[0xDDu8; 8180], "before patch preserved");
    assert_eq!(&buf[8180..8210], &[0xEEu8; 30], "patch applied");
    assert_eq!(
        &buf[8210..],
        &[0xDDu8; 4 * SECTOR_SIZE as usize - 8210],
        "after patch preserved"
    );
}

// T4.6: byte_offset_validation_exceeds_range
#[test]
fn byte_offset_validation_exceeds_range() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0"); // 1 sector = 4096 bytes

    // Read at EOF
    sector
        .seek(SeekFrom::Start(u64::from(SECTOR_SIZE)))
        .expect("seek to EOF");
    let mut tiny = [0u8; 1];
    let n = sector.read(&mut tiny).expect("read at EOF");
    assert_eq!(n, 0, "read at EOF should return 0");

    // Partial read at end
    sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
    let mut buf = [0u8; 10];
    let n = sector.read(&mut buf).expect("read at 4090");
    assert_eq!(n, 6, "should read partial at end");

    // Same for write
    // Write at EOF
    sector
        .seek(SeekFrom::Start(u64::from(SECTOR_SIZE)))
        .expect("seek to EOF");
    let n = sector.write(b"x").expect("write at EOF");
    assert_eq!(n, 0, "write at EOF should return 0");

    // Write partial at end
    sector.seek(SeekFrom::Start(4090)).expect("seek to 4090");
    let n = sector.write(&[0xFFu8; 10]).expect("write partial at end");
    assert_eq!(n, 6, "should write partial at end");
}

// T4.7: byte_offset_empty_buf_is_noop
#[test]
fn byte_offset_empty_buf_is_noop() {
    let ctx = create_fixed_test_io_writable();
    let io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");

    // Empty read
    assert_eq!(
        sector.read(&mut []).expect("empty read"),
        0,
        "empty read returns 0"
    );
    // Empty write
    assert_eq!(
        sector.write(&[]).expect("empty write"),
        0,
        "empty write returns 0"
    );
}

// T4.8: byte_offset_write_to_read_only_returns_error
#[test]
fn byte_offset_write_to_read_only_returns_error() {
    let ctx = create_fixed_test_io();
    let io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");

    let err = sector.write(&[0x42u8; 10]).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::PermissionDenied,
        "should be PermissionDenied error"
    );
}

// T4.9: byte_offset_write_to_not_present_block_allocates
#[test]
fn byte_offset_write_to_not_present_block_allocates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-dynamic-rw.vhdx");

    VhdxFile::create(&path)
        .size(256 * u64::from(MIB))
        .block_size(32 * MIB)
        .logical_sector_size(4096)
        .finish()
        .expect("create dynamic test vhdx");

    let file = VhdxFile::open(&path)
        .write()
        .finish()
        .expect("open writable dynamic");
    let ctx = TestContext {
        _dir: dir,
        file,
        overlay: None,
    };
    let io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");

    let written = sector.write(&[0x42u8; 10]).expect("write allocates block");
    assert_eq!(written, 10, "partial write should report bytes written");

    sector.seek(SeekFrom::Start(0)).expect("seek to start");
    let mut buf = vec![0xFFu8; SECTOR_SIZE.into()];
    sector.read_exact(&mut buf).expect("read allocated sector");
    assert_eq!(&buf[..10], &[0x42u8; 10]);
    assert!(buf[10..].iter().all(|&byte| byte == 0));
}
