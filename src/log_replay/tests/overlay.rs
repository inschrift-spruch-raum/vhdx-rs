use super::helpers::*;
use super::prelude::*;
use crc32c::crc32c;
use std::io::{Read, Seek, SeekFrom};

// -----------------------------------------------------------------------
// Replay overlay tests
// -----------------------------------------------------------------------

#[test]
fn build_overlay_single_data_descriptor() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    let overlay = build_replay_overlay(&active).unwrap();

    assert_eq!(overlay.last_file_offset(), 0x2_0000_0000);
    assert!(overlay.sectors().contains_key(&0x1000));
    assert_eq!(overlay.sectors()[&0x1000].len(), 4096);

    // Verify the assembled sector: LeadingBytes(8) + Data(4084) + TrailingBytes(4)
    let sector = &overlay.sectors()[&0x1000];
    // Leading bytes: 0x0102030405060708
    assert_eq!(&sector[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
    // Middle 4084 bytes: fill_byte 0xAA
    assert_eq!(sector[8], 0xAA);
    assert_eq!(sector[4091], 0xAA);
    // Trailing bytes: 0xDEADBEEF
    assert_eq!(&sector[4092..4096], &0xDEAD_BEEFu32.to_le_bytes());
}

#[test]
fn build_overlay_zero_descriptor() {
    let guid = test_log_guid();
    let entry = build_log_entry(
        1,
        0,
        &[(false, 0x5000, 0x2000)], // zero: offset 0x5000, length 0x2000
        0,
        &guid,
    );
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    let overlay = build_replay_overlay(&active).unwrap();

    assert_eq!(overlay.zeros().len(), 1);
    assert_eq!(overlay.zeros()[0], (0x5000, 0x2000));
    assert!(overlay.sectors().is_empty());
}

#[test]
fn build_overlay_mixed_descriptors() {
    let guid = test_log_guid();
    let entry = build_log_entry(
        1,
        0,
        &[
            (true, 0x1000, 0),       // data at 0x1000
            (false, 0x5000, 0x2000), // zero at 0x5000, len 0x2000
            (true, 0x2000, 0),       // data at 0x2000
        ],
        0xCC,
        &guid,
    );
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    let overlay = build_replay_overlay(&active).unwrap();

    assert_eq!(overlay.sectors().len(), 2);
    assert!(overlay.sectors().contains_key(&0x1000));
    assert!(overlay.sectors().contains_key(&0x2000));
    assert_eq!(overlay.zeros().len(), 1);
    assert_eq!(overlay.zeros()[0], (0x5000, 0x2000));
}

#[test]
fn active_sequence_entries_are_in_replay_order() {
    let guid = test_log_guid();
    let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
    let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);
    let buf = build_log_buffer(vec![e1, e2, e3]);

    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    // Replay order must be tail→head (1, 2, 3)
    let seqs: Vec<u64> = active
        .entries()
        .iter()
        .map(|e| e.entry.header().sequence_number())
        .collect();
    assert_eq!(seqs, vec![1, 2, 3]);
}

#[test]
fn entry_with_no_descriptors_replays_ok() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[], 0, &guid);
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    let overlay = build_replay_overlay(&active).unwrap();
    assert_eq!(overlay.sectors().len(), 0);
    assert_eq!(overlay.zeros().len(), 0);
}

// -----------------------------------------------------------------------
// replay_to_file tests (write to temp file)
// -----------------------------------------------------------------------

#[test]
fn replay_to_file_writes_data() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay_test.vhdx");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();

    // Pre-size to avoid set_len on non-extended regions
    file.set_len(0x2_0000_0000).unwrap();

    replay_to_file(&mut file, &active).unwrap();

    // Read back the written sector
    let mut read_buf = [0u8; 4096];
    let mut f = std::fs::File::open(&path).unwrap();
    f.seek(SeekFrom::Start(0x1000)).unwrap();
    f.read_exact(&mut read_buf).unwrap();

    // Verify assembled sector
    assert_eq!(&read_buf[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(read_buf[8], 0xAA);
}

#[test]
fn active_sequence_flushed_and_last_file_offsets() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    assert_eq!(active.flushed_file_offset(), 0x1_0000_0000);
    assert_eq!(active.last_file_offset(), 0x2_0000_0000);
}

#[test]
fn candidate_with_higher_seq_always_wins() {
    let guid = test_log_guid();
    // Build three separate 1-entry sequences with different seq numbers
    let e5 = build_log_entry(5, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let mut e10 = build_log_entry(10, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
    let mut e100 = build_log_entry(100, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

    // Fix tail offsets so each entry points to itself (self-tail)
    // e5 is at offset 0, tail=0 is already correct
    let e5_len = e5.len();
    let e10_offset = e5_len;
    let e100_offset = e5_len + e10.len();

    // Fix e10 tail to point to itself (zero CRC field before recomputing)
    e10[12..16].copy_from_slice(
        &u32::try_from(e10_offset)
            .expect("offset fits u32")
            .to_le_bytes(),
    );
    e10[4..8].copy_from_slice(&0u32.to_le_bytes());
    let e10_crc = crc32c(&e10);
    e10[4..8].copy_from_slice(&e10_crc.to_le_bytes());

    // Fix e100 tail to point to itself (zero CRC field before recomputing)
    e100[12..16].copy_from_slice(
        &u32::try_from(e100_offset)
            .expect("offset fits u32")
            .to_le_bytes(),
    );
    e100[4..8].copy_from_slice(&0u32.to_le_bytes());
    let e100_checksum = crc32c(&e100);
    e100[4..8].copy_from_slice(&e100_checksum.to_le_bytes());

    // Place them in buffer; each with self-tail pointing to its own offset
    let buf = build_log_buffer(vec![e5, e10, e100]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    // Should pick the 1-entry sequence with seq=100
    assert_eq!(active.len(), 1);
    assert_eq!(active.entries()[0].entry.header().sequence_number(), 100);
}
