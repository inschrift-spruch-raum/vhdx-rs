use super::helpers::*;
use super::*;
use crc32c::crc32c;

// -----------------------------------------------------------------------
// Active sequence detection tests
// -----------------------------------------------------------------------

#[test]
fn empty_log_no_sequence() {
    let buf = vec![0u8; SECTOR_SIZE as usize * 4]; // empty (no valid entries)
    let log = Log::new(&buf).unwrap();
    let guid = test_log_guid();
    assert!(detect_active_sequence(&log, &guid).is_err());
}

#[test]
fn single_entry_self_tail() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);

    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active.flushed_file_offset(), 0x1_0000_0000);
    assert_eq!(active.last_file_offset(), 0x2_0000_0000);
}

#[test]
fn three_entry_sequence() {
    let guid = test_log_guid();
    // Three entries forming a sequence: seq 1, 2, 3
    // Entry 1: tail=0 (self), Entry 2: tail=0, Entry 3: tail=0
    let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
    let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);
    let buf = build_log_buffer(vec![e1, e2, e3]);

    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();
    assert_eq!(active.len(), 3);

    // Verify entry sequence numbers
    let seqs: Vec<u64> = active
        .entries()
        .iter()
        .map(|e| e.entry.header().sequence_number())
        .collect();
    assert_eq!(seqs, vec![1, 2, 3]);
}

#[test]
fn multiple_sequences_picks_newest() {
    let guid = test_log_guid();
    // Old sequence: 1, 2, 3
    let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);
    let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

    // New sequence: 10, 11, 12 (head entry 12 has tail pointing to entry 10's offset)
    let e10 = build_log_entry(10, 0, &[(true, 0x4000, 0)], 0xDD, &guid);
    let e11 = build_log_entry(11, 0, &[(true, 0x5000, 0)], 0xEE, &guid);
    let e12 = build_log_entry(12, 0, &[(true, 0x6000, 0)], 0xFF, &guid);

    let mut entries = vec![e1, e2, e3, e10, e11, e12];
    // Fix tail of e12 to point to e10's offset
    // We need to know e10's offset. Let's calculate it.
    let e1_len = entries[0].len();
    let e2_len = entries[1].len();
    let e3_len = entries[2].len();
    let e10_offset = e1_len + e2_len + e3_len;
    // Rewrite e12's tail to point to e10
    let e12_idx = 5;
    entries[e12_idx][12..16].copy_from_slice(
        &u32::try_from(e10_offset)
            .expect("offset fits u32")
            .to_le_bytes(),
    );
    // Recompute CRC for e12 (zero CRC field before recomputing)
    entries[e12_idx][4..8].copy_from_slice(&0u32.to_le_bytes());
    let e12_crc = crc32c(&entries[e12_idx]);
    entries[e12_idx][4..8].copy_from_slice(&e12_crc.to_le_bytes());

    let buf = build_log_buffer(entries);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    let seqs: Vec<u64> = active
        .entries()
        .iter()
        .map(|e| e.entry.header().sequence_number())
        .collect();
    assert_eq!(seqs, vec![10, 11, 12], "should pick newest sequence");
}

#[test]
fn guid_mismatch_entry_ignored() {
    let guid = test_log_guid();
    let other_guid = Guid::from_bytes([0xFFu8; 16]);

    let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &other_guid); // wrong GUID
    let e3 = build_log_entry(3, 0, &[(true, 0x3000, 0)], 0xCC, &guid);

    let buf = build_log_buffer(vec![e1, e2, e3]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    // Only entry 1 should be in the sequence (e2 has wrong GUID, breaks chain)
    assert_eq!(active.len(), 1);
    assert_eq!(active.entries()[0].entry.header().sequence_number(), 1);
}

#[test]
fn checksum_failure_entry_ignored() {
    let guid = test_log_guid();
    let e1 = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let mut e2 = build_log_entry(2, 0, &[(true, 0x2000, 0)], 0xBB, &guid);

    // Corrupt e2's CRC so it gets ignored during scanning
    e2[100] ^= 0xFF;

    let buf = build_log_buffer(vec![e1, e2]);
    let log = Log::new(&buf).unwrap();
    let active = detect_active_sequence(&log, &guid).unwrap();

    // Only e1 should be in the active sequence — e2 is ignored due to CRC failure
    assert_eq!(active.len(), 1);
    assert_eq!(active.entries()[0].entry.header().sequence_number(), 1);
}

#[test]
fn has_pending_log_zero_guid() {
    let guid = test_log_guid();
    let zero_guid = Guid::from_bytes([0u8; 16]);
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);

    let log = Log::new(&buf).unwrap();
    // With zero GUID, should report no pending log
    assert!(!has_pending_log(&log, &zero_guid));
}

#[test]
fn has_pending_log_with_sequence() {
    let guid = test_log_guid();
    let entry = build_log_entry(1, 0, &[(true, 0x1000, 0)], 0xAA, &guid);
    let buf = build_log_buffer(vec![entry]);

    let log = Log::new(&buf).unwrap();
    assert!(has_pending_log(&log, &guid));
}

#[test]
fn has_pending_log_empty_buffer() {
    let guid = test_log_guid();
    let buf = vec![0u8; SECTOR_SIZE as usize * 4];
    let log = Log::new(&buf).unwrap();
    assert!(!has_pending_log(&log, &guid));
}
