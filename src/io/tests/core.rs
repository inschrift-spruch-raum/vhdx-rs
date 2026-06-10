use super::support::*;

#[test]
fn io_creation_from_test_file() {
    let mut ctx = create_test_io();
    let io = ctx.io();
    assert!(io.block_size > 0);
    assert_eq!(io.block_size, 32 * MIB);
    assert!(io.logical_sector_size > 0);
    assert_eq!(io.logical_sector_size, u32::from(SECTOR_SIZE));
}

#[test]
fn sector_out_of_bounds() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    // start + count overflow → InvalidParameter
    let result = io.sector(u64::MAX, 1);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::InvalidParameter(..)));
}

#[test]
fn sector_zero_is_valid() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let result = io.sector(0, 1);
    assert!(result.is_ok(), "sector 0 failed: {:?}", result.err());
}

#[test]
fn sector_read_returns_logical_sector_size() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("get sector 0");
    let mut buf = vec![0u8; SECTOR_SIZE.into()];
    sector.read_exact(&mut buf).expect("read sector 0");
}

#[test]
fn sector_read_byte_range_exceeds_range() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("get sector 0");
    let mut buf = [0u8; 4097]; // 1 byte too many for a single 4096-byte sector
    let n = sector.read(&mut buf).expect("should read what's available");
    assert_eq!(n, SECTOR_SIZE.into(), "reads max available");
}

#[test]
fn sector_zero_read_is_all_zeros_for_dynamic_disk() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");
    let mut buf = vec![0xFFu8; SECTOR_SIZE.into()];
    sector.read_exact(&mut buf).expect("read sector 0");
    // Dynamic disk: sector 0 is in a NotPresent block → should be zeros
    assert!(
        buf.iter().all(|&b| b == 0),
        "expected all zeros, got non-zero data"
    );
}

#[test]
fn sector_write_fails_on_read_only() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");
    let data = vec![0x42u8; SECTOR_SIZE.into()];
    let result = sector.write(&data);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);
}

// -- Overlay read-through tests ----------------------------------------

/// Helper: create a fixed VHDX so that blocks are `FullyPresent`,
/// then attach a `ReplayOverlay` to the IO.
fn create_fixed_io_with_overlay(overlay: ReplayOverlay) -> TestContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-fixed.vhdx");

    create_vhdx(&path)
            .size(4 * u64::from(MIB)) // 4 MB virtual
            .block_size(MIB) // 1 MB blocks
            .logical_sector_size(4096)
            .fixed(true)
            .finish()
            .expect("create fixed test vhdx");

    let file = open_vhdx(&path);

    TestContext {
        _dir: dir,
        file,
        overlay: Some(Arc::new(overlay)),
    }
}

/// Helper: resolve the file offset for sector 0's payload data.
fn sector_zero_file_offset(io: &mut IO<'_>) -> u64 {
    let mut sector = io.sector(0, 1).expect("sector 0");
    let entry = sector.resolve_bat_entry().expect("resolve BAT");
    entry.file_offset_mb() * u64::from(MIB)
}

#[test]
fn overlay_data_served_through_sector_read() {
    use std::collections::HashMap;

    // Build a minimal overlay with a known sector.
    let dir = tempfile::tempdir().expect("tempdir for baseline");
    let path = dir.path().join("base.vhdx");
    create_vhdx(&path)
        .size(4 * u64::from(MIB))
        .block_size(MIB)
        .logical_sector_size(4096)
        .fixed(true)
        .finish()
        .expect("create baseline fixed vhdx");

    let baseline_file = open_vhdx(&path);
    let mut baseline_ctx = TestContext {
        _dir: dir,
        file: baseline_file,
        overlay: None,
    };
    let mut baseline_io = baseline_ctx.io();
    let payload_offset = sector_zero_file_offset(&mut baseline_io);

    // Construct overlay with a sector full of 0xAA at the payload offset.
    let mut sectors = HashMap::new();
    sectors.insert(payload_offset, vec![0xAAu8; SECTOR_SIZE.into()]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut ctx = create_fixed_io_with_overlay(overlay);
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");
    let mut buf = vec![0u8; SECTOR_SIZE.into()];
    sector
        .read_exact(&mut buf)
        .expect("read sector 0 with overlay");
    assert!(
        buf.iter().all(|&b| b == 0xAA),
        "expected all 0xAA from overlay, got {:?}",
        &buf[..32]
    );
}

#[test]
fn no_overlay_falls_through_to_file() {
    // Create a fixed VHDX — all blocks are FullyPresent, filled with zeros.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("no-overlay.vhdx");
    create_vhdx(&path)
        .size(4 * u64::from(MIB))
        .block_size(MIB)
        .logical_sector_size(4096)
        .fixed(true)
        .finish()
        .expect("create fixed vhdx");

    let file = open_vhdx(&path);
    let mut ctx = TestContext {
        _dir: dir,
        file,
        overlay: None,
    };
    let mut io = ctx.io();
    // No overlay — should be None.
    assert!(io.overlay.is_none(), "expected no overlay");

    let mut sector = io.sector(0, 1).expect("sector 0");
    let mut buf = vec![0xFFu8; SECTOR_SIZE.into()];
    sector.read_exact(&mut buf).expect("read sector 0");
    // Fixed disk: sector 0 is in a FullyPresent block, zero-filled on create.
    assert!(
        buf.iter().all(|&b| b == 0),
        "expected all zeros from file, got non-zero"
    );
}

#[test]
fn overlay_zero_region_served_through_sector_read() {
    use std::collections::HashMap;

    // Build baseline to find payload offset.
    let dir = tempfile::tempdir().expect("tempdir for baseline");
    let path = dir.path().join("base-zero.vhdx");
    create_vhdx(&path)
        .size(4 * u64::from(MIB))
        .block_size(MIB)
        .logical_sector_size(4096)
        .fixed(true)
        .finish()
        .expect("create baseline fixed vhdx");

    let baseline_file = open_vhdx(&path);
    let mut baseline_ctx = TestContext {
        _dir: dir,
        file: baseline_file,
        overlay: None,
    };
    let mut baseline_io = baseline_ctx.io();
    let payload_offset = sector_zero_file_offset(&mut baseline_io);

    // Construct overlay with a zero region covering sector 0.
    let overlay = ReplayOverlay::from_raw(
        HashMap::new(),
        vec![(payload_offset, u64::from(SECTOR_SIZE))],
    );

    let mut ctx = create_fixed_io_with_overlay(overlay);
    let mut io = ctx.io();
    let mut sector = io.sector(0, 1).expect("sector 0");
    let mut buf = vec![0xFFu8; SECTOR_SIZE.into()];
    sector
        .read_exact(&mut buf)
        .expect("read sector 0 with zero overlay");
    // Overlay has a zero region at this offset → should be zeros
    // even though the underlying file has actual data.
    assert!(
        buf.iter().all(|&b| b == 0),
        "expected all zeros from zero-region overlay"
    );
}

// -- Multi-sector tests -------------------------------------------------

/// Helper: create a small fixed VHDX opened in **write** mode.
///
/// 4 MB virtual, 1 MB block, 4096 logical sector size.
/// Blocks are `FullyPresent` and zero-initialized (fixed disk).
pub(super) fn create_fixed_test_io_writable() -> TestContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-fixed-rw.vhdx");

    create_vhdx(&path)
        .size(4 * u64::from(MIB))
        .block_size(MIB)
        .logical_sector_size(4096)
        .fixed(true)
        .finish()
        .expect("create fixed test vhdx");

    let file = open_vhdx_writable(&path);

    TestContext {
        _dir: dir,
        file,
        overlay: None,
    }
}

#[test]
fn multi_sector_read_within_single_block() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    // 1 MB / 4096 = 256 sectors per block; 3 sectors fit in block 0.
    let mut sector = io.sector(0, 3).expect("sector(0,3)");
    let mut buf = vec![0u8; 3 * SECTOR_SIZE as usize];
    sector.read_exact(&mut buf).expect("read 3 sectors");
    assert_eq!(buf.len(), 3 * SECTOR_SIZE as usize);
    // Fixed disk is zero-initialized.
    assert!(buf.iter().all(|&b| b == 0), "expected all zeros");
}

#[test]
fn multi_sector_read_count_one_regression() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    // Write a known pattern to sector 0.
    let mut sw = io.sector(0, 1).expect("sector 0");
    sw.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw.write_all(&[0x42u8; SECTOR_SIZE as usize])
        .expect("write 0x42");

    let mut buf = vec![0u8; SECTOR_SIZE.into()];
    let mut sr = io.sector(0, 1).expect("sector 0");
    sr.read_exact(&mut buf).expect("read back");
    assert!(
        buf.iter().all(|&b| b == 0x42),
        "expected all 0x42, got {:?}",
        &buf[..32]
    );
}

#[test]
fn multi_sector_write_count_one_regression() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    let mut sw = io.sector(0, 1).expect("sector 0");
    sw.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw.write_all(&[0xAAu8; SECTOR_SIZE as usize])
        .expect("write 0xAA");

    let mut buf = vec![0u8; SECTOR_SIZE.into()];
    let mut sr = io.sector(0, 1).expect("sector 0");
    sr.read_exact(&mut buf).expect("read back");
    assert!(
        buf.iter().all(|&b| b == 0xAA),
        "expected all 0xAA, got {:?}",
        &buf[..32]
    );
}

#[test]
fn multi_sector_read_buffer_size_mismatch() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 3).expect("sector(0,3)");
    let mut buf = vec![0u8; SECTOR_SIZE.into()]; // smaller than 3*4096
    let n = sector.read(&mut buf).expect("read from 3-sector range");
    assert_eq!(n, SECTOR_SIZE.into(), "reads partial from 3-sector range");
}

#[test]
fn multi_sector_write_data_size_mismatch() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    let mut sector = io.sector(0, 2).expect("sector(0,2)");
    let data = vec![0u8; SECTOR_SIZE.into()]; // smaller than 2*4096
    let n = sector.write(&data).expect("write to 2-sector range");
    assert_eq!(n, SECTOR_SIZE.into(), "writes partial to 2-sector range");
}

#[test]
fn multi_sector_count_zero_is_error() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let result = io.sector(0, 0);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), Error::InvalidParameter(..)),
        "expected InvalidParameter for count=0"
    );
}

#[test]
fn multi_sector_start_plus_count_overflow() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    let result = io.sector(u64::MAX, 2);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), Error::InvalidParameter(..)),
        "expected InvalidParameter for overflow"
    );
}

#[test]
fn multi_sector_out_of_bounds_range() {
    let mut ctx = create_test_io();
    let mut io = ctx.io();
    // dynamic VHDX: 256 MB / 4096 = 65536 sectors, max_sector = 65535
    // requesting 70000 sectors from start=0 exceeds range
    let result = io.sector(0, 70000);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), Error::SectorOutOfBounds { .. }),
        "expected SectorOutOfBounds"
    );
}

#[test]
fn multi_sector_read_spanning_block_boundary() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    // 1 MB / 4096 = 256 sectors per block
    // Read sectors 254-257: spans block 0 (sectors 0-255) → block 1 (sectors 256-511)
    let mut sector = io.sector(254, 4).expect("sector(254,4)");
    let mut buf = vec![0xFFu8; 4 * SECTOR_SIZE as usize];
    sector.read_exact(&mut buf).expect("read spanning boundary");
    // Fixed disk is zero-initialized
    assert!(
        buf.iter().all(|&b| b == 0),
        "expected all zeros across block boundary"
    );
}

#[test]
fn multi_sector_write_spanning_block_boundary() {
    let mut ctx = create_fixed_test_io_writable();
    let mut io = ctx.io();
    // 1 MB / 4096 = 256 sectors per block
    // Write to sectors 254-257: spans block 0 → block 1
    let data = vec![0x42u8; 4 * SECTOR_SIZE as usize];
    let mut sw = io.sector(254, 4).expect("sector(254,4)");
    sw.seek(SeekFrom::Start(0)).expect("seek to 0");
    sw.write_all(&data).expect("write spanning boundary");

    // Read back and verify
    let mut buf = vec![0u8; 4 * SECTOR_SIZE as usize];
    let mut sr = io.sector(254, 4).expect("sector(254,4)");
    sr.read_exact(&mut buf)
        .expect("read back spanning boundary");
    assert!(
        buf.iter().all(|&b| b == 0x42),
        "expected all 0x42 across block boundary, got {:?}",
        &buf[..32]
    );
}

// -- T14: Sector bitmap per-bit lookup correctness test ----------------

/// Verify the sector bitmap lookup math used by `is_sector_in_child()`.
///
/// This test validates the computation of `byte_idx`, `bit_idx` from
/// (`block_idx`, `sector_in_block`) parameters using a known bitmap pattern,
/// without needing an actual VHDX file.
#[test]
fn sector_bitmap_bit_lookup_correctness() {
    let block_size: u64 = 32 * u64::from(MIB);
    let logical_sector_size: u64 = u64::from(SECTOR_SIZE);
    let sectors_per_block = block_size / logical_sector_size; // 8192
    let chunk_ratio: u64 = (1u64 << 23) * logical_sector_size / block_size; // 1024
    let stride = chunk_ratio + 1; // 1025

    // Build a synthetic 1 MB bitmap with known patterns
    let mut bitmap = vec![0u8; MIB as usize];

    // Set specific bits to validate lookup:
    {
        let bits = bitmap.view_bits_mut::<Lsb0>();
        bits.set(0, true); // Sector 0 (byte 0, bit 0)
        bits.set(7, true); // Sector 7 (byte 0, bit 7)
        bits.set(8, true); // Sector 8 (byte 1, bit 0)
        bits.set(1000, true); // Sector 1000 (byte 125, bit 0)
    }

    // Test lookup for various (block_idx, sector_in_block) combinations.
    // sector_in_chunk = block_in_chunk * sectors_per_block + sector_in_block

    // Case 1: block_idx=0 (chunk 0), sector_in_block=0
    {
        let block_in_chunk = 0u64 % chunk_ratio; // 0
        let sector_in_chunk = block_in_chunk * sectors_per_block; // 0
        let byte_idx = usize::try_from(sector_in_chunk / 8).expect("byte index fits usize");
        let bit_idx = u8::try_from(sector_in_chunk % 8).expect("bit index fits u8");
        assert_eq!(byte_idx, 0);
        assert_eq!(bit_idx, 0);
        let bits = bitmap.view_bits::<Lsb0>();
        assert!(
            bits[usize::try_from(sector_in_chunk).expect("sector index fits usize")],
            "sector 0 should be present"
        );
    }

    // Case 2: block_idx=0, sector_in_block=7
    {
        let block_in_chunk = 0u64;
        let sector_in_chunk = block_in_chunk * sectors_per_block + 7u64; // 7
        let byte_idx = usize::try_from(sector_in_chunk / 8).expect("byte index fits usize");
        let bit_idx = u8::try_from(sector_in_chunk % 8).expect("bit index fits u8");
        assert_eq!(byte_idx, 0);
        assert_eq!(bit_idx, 7);
        let bits = bitmap.view_bits::<Lsb0>();
        let sector_idx = usize::try_from(sector_in_chunk).expect("sector index fits usize");
        assert!(bits[sector_idx], "sector 7 should be present");
    }

    // Case 3: block_idx=0, sector_in_block=8
    {
        let block_in_chunk = 0u64;
        let sector_in_chunk = block_in_chunk * sectors_per_block + 8u64; // 8
        let byte_idx = usize::try_from(sector_in_chunk / 8).expect("byte index fits usize");
        let bit_idx = u8::try_from(sector_in_chunk % 8).expect("bit index fits u8");
        assert_eq!(byte_idx, 1);
        assert_eq!(bit_idx, 0);
        let bits = bitmap.view_bits::<Lsb0>();
        let sector_idx = usize::try_from(sector_in_chunk).expect("sector index fits usize");
        assert!(bits[sector_idx], "sector 8 should be present");
    }

    // Case 4: block_idx=0, sector_in_block=1000
    {
        let block_in_chunk = 0u64;
        let sector_in_chunk = block_in_chunk * sectors_per_block + 1000u64; // 1000
        let byte_idx = usize::try_from(sector_in_chunk / 8).expect("byte index fits usize");
        let bit_idx = u8::try_from(sector_in_chunk % 8).expect("bit index fits u8");
        assert_eq!(byte_idx, 125);
        assert_eq!(bit_idx, 0);
        let bits = bitmap.view_bits::<Lsb0>();
        assert!(
            bits[usize::try_from(sector_in_chunk).expect("sector index fits usize")],
            "sector 1000 should be present"
        );
    }

    // Case 5: block_idx=1 (chunk 0), sector_in_block=0
    // sector_in_chunk = 1 * 8192 + 0 = 8192 → byte 1024, bit 0 → not set
    {
        let block_in_chunk = 1u64 % chunk_ratio; // 1
        let sector_in_chunk = block_in_chunk * sectors_per_block; // 8192
        let byte_idx = usize::try_from(sector_in_chunk / 8).expect("byte index fits usize");
        let bit_idx = u8::try_from(sector_in_chunk % 8).expect("bit index fits u8");
        assert_eq!(byte_idx, 1024);
        assert_eq!(bit_idx, 0);
        let bits = bitmap.view_bits::<Lsb0>();
        assert!(
            !bits[usize::try_from(sector_in_chunk).expect("sector index fits usize")],
            "block 1 sector 0 should NOT be present"
        );
    }

    // Verify chunk_idx and sb_bat_idx computation
    let block_idx: u64 = 5;
    let chunk_idx = block_idx / chunk_ratio; // 0
    let sb_bat_idx = chunk_idx * stride + chunk_ratio; // 1024
    assert_eq!(chunk_idx, 0);
    assert_eq!(sb_bat_idx, 1024);

    let block_idx_2: u64 = 1024;
    let chunk_idx_2 = block_idx_2 / chunk_ratio; // 1
    let sb_bat_idx_2 = chunk_idx_2 * stride + chunk_ratio; // 2049
    assert_eq!(chunk_idx_2, 1);
    assert_eq!(sb_bat_idx_2, 2049);
}
