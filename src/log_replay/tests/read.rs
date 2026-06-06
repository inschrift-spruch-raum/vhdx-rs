use super::helpers::*;
use super::*;

// -----------------------------------------------------------------------
// ReplayOverlay::read() tests
// -----------------------------------------------------------------------

/// Helper: create a dummy `File` for `read()` calls (the parameter is unused).
fn dummy_file() -> std::fs::File {
    tempfile::tempfile().unwrap()
}

/// Helper: build an overlay from a single entry with the given descriptors.
fn build_overlay_from_descs(desc_specs: &[(bool, u64, u64)], fill_byte: u8) -> ReplayOverlay {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, desc_specs, fill_byte, &guid);
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    build_replay_overlay(&active).unwrap()
}

#[test]
fn read_returns_overlay_data_at_sector_offset() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
    let file = dummy_file();

    // Read at the exact sector offset
    let mut buf = [0u8; 4096];
    let n = overlay.read(&file, 0x1000, &mut buf);
    assert_eq!(n, 4096);

    // Verify assembled sector content: LeadingBytes + fill + TrailingBytes
    assert_eq!(&buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(buf[8], 0xAA);
    assert_eq!(buf[4091], 0xAA);
    assert_eq!(&buf[4092..4096], &0xDEAD_BEEFu32.to_le_bytes());
}

#[test]
fn read_returns_zero_for_non_overlaid_offset() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
    let file = dummy_file();

    let mut buf = [0u8; 64];
    let n = overlay.read(&file, 0x9000, &mut buf);
    assert_eq!(n, 0, "should return Ok(0) for non-overlaid offset");
}

#[test]
fn read_at_mid_sector_offset() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xBB);
    let file = dummy_file();

    // Read starting 100 bytes into the sector
    let mut buf = [0u8; 200];
    let n = overlay.read(&file, 0x1000 + 100, &mut buf);
    assert_eq!(n, 200);

    // The assembled sector has fill_byte 0xBB at indices 8..4092,
    // so at file offset 0x1000+100 = sector byte 100, which is in
    // the middle fill region (byte 100 > 8).
    assert_eq!(buf[0], 0xBB);
}

#[test]
fn read_partial_buf_smaller_than_sector() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xCC);
    let file = dummy_file();

    // Only read 10 bytes
    let mut buf = [0u8; 10];
    let n = overlay.read(&file, 0x1000, &mut buf);
    assert_eq!(n, 10);

    // First 8 bytes are leading bytes
    assert_eq!(&buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
    // Next 2 bytes are fill byte
    assert_eq!(buf[8], 0xCC);
    assert_eq!(buf[9], 0xCC);
}

#[test]
fn read_near_end_of_sector() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xDD);
    let file = dummy_file();

    // Read starting 2 bytes before the sector end
    let offset = 0x1000 + 4094;
    let mut buf = [0u8; 64];
    let n = overlay.read(&file, offset, &mut buf);

    // Only 2 bytes remain in the sector
    assert_eq!(n, 2);
    // Last 4 bytes of sector are trailing bytes 0xDEADBEEF
    // At offset 4094, we get bytes 4094..4096 = last 2 bytes of trailing
    let trailing = 0xDEAD_BEEFu32.to_le_bytes();
    assert_eq!(&buf[0..2], &trailing[2..4]);
}

#[test]
fn read_at_sector_boundary_returns_zero() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xEE);
    let file = dummy_file();

    // Exactly at the sector end — no more data
    let mut buf = [0u8; 64];
    let n = overlay.read(&file, 0x1000 + 4096, &mut buf);
    assert_eq!(n, 0);
}

#[test]
fn read_zero_region_returns_zeroes() {
    let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
    let file = dummy_file();

    let mut buf = [0xFFu8; 256];
    let n = overlay.read(&file, 0x5000, &mut buf);
    assert_eq!(n, 256);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn read_zero_region_mid_offset() {
    let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
    let file = dummy_file();

    // Read 100 bytes starting 1000 bytes into the zero region
    let mut buf = [0xFFu8; 100];
    let n = overlay.read(&file, 0x5000 + 1000, &mut buf);
    assert_eq!(n, 100);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn read_zero_region_beyond_returns_zero() {
    let overlay = build_overlay_from_descs(&[(false, 0x5000, 0x2000)], 0);
    let file = dummy_file();

    let mut buf = [0u8; 64];
    let n = overlay.read(&file, 0x5000 + 0x2000, &mut buf);
    assert_eq!(n, 0, "beyond zero region should return Ok(0)");
}

#[test]
fn read_empty_buf() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0)], 0xAA);
    let file = dummy_file();

    let mut buf = [];
    let n = overlay.read(&file, 0x1000, &mut buf);
    assert_eq!(n, 0);
}

#[test]
fn read_data_sector_overlaps_zero_region_at_different_offset() {
    // Data sector at 0x1000, zero region at 0x5000 — they don't overlap
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0), (false, 0x5000, 0x2000)], 0xAA);
    let file = dummy_file();

    // Read from data sector
    let mut buf = [0u8; 8];
    let n = overlay.read(&file, 0x1000, &mut buf);
    assert_eq!(n, 8);

    // Read from zero region
    buf.fill(0xFF);
    let n = overlay.read(&file, 0x5000, &mut buf);
    assert_eq!(n, 8);
    assert!(buf.iter().all(|&b| b == 0));

    // Read from neither
    buf.fill(0xFF);
    let n = overlay.read(&file, 0x3000, &mut buf);
    assert_eq!(n, 0);
}

#[test]
fn read_with_multiple_sectors() {
    let overlay = build_overlay_from_descs(&[(true, 0x1000, 0), (true, 0x3000, 0)], 0xAA);
    let file = dummy_file();

    // First sector at 0x1000
    let mut buf = [0u8; 8];
    let n = overlay.read(&file, 0x1000, &mut buf);
    assert_eq!(n, 8);

    // Second sector at 0x3000
    let n = overlay.read(&file, 0x3000, &mut buf);
    assert_eq!(n, 8);

    // Between sectors (0x2000) — no overlay
    let n = overlay.read(&file, 0x2000, &mut buf);
    assert_eq!(n, 0);
}
