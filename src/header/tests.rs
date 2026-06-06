use super::*;
use crate::constants::{
    HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, REGION_ENTRY_SIZE, REGION_TABLE_SIZE,
    REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET, RT_HEADER_SIZE,
};
use crate::error::Error;
use crate::types::Crc32c;
use bitvec::prelude::*;
use crc32c::crc32c;

/// Build a minimal valid 320 KB header section for testing.
fn build_test_header_section() -> Vec<u8> {
    let mut buf = vec![0u8; (REGION_TABLE2_OFFSET + REGION_TABLE_SIZE) as usize];

    // File type identifier
    buf[0..8].copy_from_slice(b"vhdxfile");

    // Header 1 at 64 KB (sequence_number = 5)
    write_header(&mut buf, HEADER1_OFFSET as usize, 5);

    // Header 2 at 128 KB (sequence_number = 3)
    write_header(&mut buf, HEADER2_OFFSET as usize, 3);

    // Region table 1 at 192 KB (2 entries)
    write_region_table(&mut buf, REGION_TABLE1_OFFSET as usize, 2);

    // Region table 2 at 256 KB (2 entries)
    write_region_table(&mut buf, REGION_TABLE2_OFFSET as usize, 2);

    buf
}

fn write_header(buf: &mut [u8], offset: usize, seq: u64) {
    let slice = &mut buf[offset..][..HEADER_SIZE as usize];
    slice[..4].copy_from_slice(b"head");
    slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
    slice[8..16].copy_from_slice(&seq.to_le_bytes());
    // file_write_guid, data_write_guid, log_guid: 3 x 16 zero bytes (offsets 16..64)
    // log_version at 64, version at 66
    slice[64..66].copy_from_slice(&0u16.to_le_bytes()); // log_version
    slice[66..68].copy_from_slice(&1u16.to_le_bytes()); // version
    slice[68..72].copy_from_slice(&(1024u32 * 1024).to_le_bytes()); // log_length
    slice[72..80].copy_from_slice(&(1024u64 * 1024).to_le_bytes()); // log_offset

    // Compute CRC-32C over the entire 4 KB with checksum zeroed.
    let checksum = crc32c(slice);
    slice[4..8].copy_from_slice(&checksum.to_le_bytes());
}

fn write_region_table(buf: &mut [u8], offset: usize, entry_count: u32) {
    let slice = &mut buf[offset..][..REGION_TABLE_SIZE as usize];
    slice[..4].copy_from_slice(b"regi");
    slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
    slice[8..12].copy_from_slice(&entry_count.to_le_bytes());
    slice[12..16].copy_from_slice(&0u32.to_le_bytes()); // reserved

    // Write entry_count entries starting at byte 16
    let entries_start = offset + RT_HEADER_SIZE as usize;
    for i in 0..entry_count as usize {
        let eoff = entries_start + i * REGION_ENTRY_SIZE as usize;
        // guid (16 bytes of incrementing pattern)
        buf[eoff..eoff + 16].copy_from_slice(&[u8::try_from(i).expect("entry index fits u8"); 16]);
        // file_offset
        buf[eoff + 16..eoff + 24].copy_from_slice(
            &(1024 * 1024 * (u64::try_from(i).expect("entry index fits u64") + 2)).to_le_bytes(),
        );
        // length
        buf[eoff + 24..eoff + 28].copy_from_slice(&(1024u32 * 1024).to_le_bytes());
        // required (bit 0 set)
        buf[eoff + 28..eoff + 32]
            .view_bits_mut::<Lsb0>()
            .set(0, true);
    }

    // Compute CRC-32C over the full 64 KB with checksum zeroed.
    let slice = &mut buf[offset..][..REGION_TABLE_SIZE as usize];
    let checksum = crc32c(slice);
    slice[4..8].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn file_type_signature() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let ft = header.file_type();
    assert_eq!(ft.signature(), b"vhdxfile");
}

#[test]
fn file_type_creator_is_512_bytes() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let creator = header.file_type().creator();
    assert_eq!(creator.len(), 512);
}

#[test]
fn header1_valid() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let h = header.header(1).unwrap();
    assert_eq!(h.signature(), b"head");
    assert_eq!(h.sequence_number(), 5);
}

#[test]
fn header2_valid() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let h = header.header(2).unwrap();
    assert_eq!(h.signature(), b"head");
    assert_eq!(h.sequence_number(), 3);
}

#[test]
fn current_header_picks_higher_sequence() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let h = header.header(0).unwrap();
    // Header 1 has seq=5, Header 2 has seq=3 → header 1 is current
    assert_eq!(h.sequence_number(), 5);
}

#[test]
fn current_header_picks_only_valid() {
    let mut buf = build_test_header_section();
    // Corrupt header 1 by overwriting signature
    buf[HEADER1_OFFSET as usize] = 0xFF;
    let header = Header::new(&buf).unwrap();
    let h = header.header(0).unwrap();
    assert_eq!(h.sequence_number(), 3); // falls back to header 2
}

#[test]
fn both_headers_corrupt_fails() {
    let mut buf = build_test_header_section();
    buf[HEADER1_OFFSET as usize] = 0xFF;
    buf[HEADER2_OFFSET as usize] = 0xFF;
    let header = Header::new(&buf).unwrap();
    let result = header.header(0);
    assert!(result.is_err());
}

#[test]
fn header_index_out_of_range() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    assert!(header.header(3).is_err());
}

#[test]
fn header_crc_validated() {
    let mut buf = build_test_header_section();
    // Corrupt the CRC of header 1
    buf[HEADER1_OFFSET as usize + 4] ^= 0xFF;
    let header = Header::new(&buf).unwrap();
    assert!(header.header(1).is_err());
}

#[test]
fn header_fields() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let h = header.header(1).unwrap();
    assert_eq!(h.log_version(), 0);
    assert_eq!(h.version(), 1);
    assert_eq!(h.log_length(), 1024 * 1024);
    assert_eq!(h.log_offset(), 1024 * 1024);
}

#[test]
fn region_table1_valid() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(1).unwrap();
    assert_eq!(rt.header().signature(), b"regi");
    assert_eq!(rt.header().entry_count(), 2);
}

#[test]
fn region_table2_valid() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(2).unwrap();
    assert_eq!(rt.header().signature(), b"regi");
    assert_eq!(rt.header().entry_count(), 2);
}

#[test]
fn current_region_table_follows_current_header() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(0).unwrap();
    // Current header is header 1 (seq=5 > 3) → current region table is RT 1
    assert_eq!(rt.header().entry_count(), 2);
}

#[test]
fn region_table_index_out_of_range() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    assert!(header.region_table(3).is_err());
}

#[test]
fn region_table_crc_validated() {
    let mut buf = build_test_header_section();
    buf[REGION_TABLE1_OFFSET as usize + 4] ^= 0xFF;
    let header = Header::new(&buf).unwrap();
    assert!(header.region_table(1).is_err());
}

#[test]
fn region_table_entries_iterator() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(1).unwrap();
    let entries: Vec<_> = rt.entries().collect();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].required());
    assert!(entries[1].required());
}

#[test]
fn region_table_entry_fields() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(1).unwrap();
    let first = rt.entries().next().unwrap();
    assert_eq!(first.file_offset(), 2 * 1024 * 1024);
    assert_eq!(first.length(), 1024 * 1024);
}

#[test]
fn buffer_too_small_fails() {
    let buf = vec![0u8; 100];
    assert!(Header::new(&buf).is_err());
}

#[test]
fn region_table_header_reserved() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(1).unwrap();
    assert_eq!(rt.header().reserved(), 0);
}

#[test]
fn region_table_header_checksum() {
    let buf = build_test_header_section();
    let header = Header::new(&buf).unwrap();
    let rt = header.region_table(1).unwrap();
    let stored = rt.header().checksum();
    // Re-verify manually
    let slice = &buf[REGION_TABLE1_OFFSET as usize..][..REGION_TABLE_SIZE as usize];
    let mut tmp = slice.to_vec();
    tmp[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(stored, Crc32c::from_raw(crc32c(&tmp)));
}

#[test]
fn current_header_rejects_equal_sequence() {
    let mut buf = build_test_header_section();
    // Both headers have seq=5: header 1 already has seq=5, set header 2 to seq=5
    let h2_offset = HEADER2_OFFSET as usize;
    buf[h2_offset + 8..h2_offset + 16].copy_from_slice(&5u64.to_le_bytes());
    // Recompute CRC for header 2
    buf[h2_offset + 4..h2_offset + 8].copy_from_slice(&0u32.to_le_bytes());
    let slice = &buf[h2_offset..h2_offset + HEADER_SIZE as usize];
    let checksum = crc32c(slice);
    buf[h2_offset + 4..h2_offset + 8].copy_from_slice(&checksum.to_le_bytes());

    let header = Header::new(&buf).unwrap();
    let result = header.header(0);
    assert!(
        matches!(result, Err(Error::HeaderSequenceNumberInvalid { .. })),
        "equal sequence numbers should be rejected"
    );
}
