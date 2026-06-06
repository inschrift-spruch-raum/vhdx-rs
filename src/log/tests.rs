use super::*;
use crate::constants::{
    SECTOR_SIZE, SIGNATURE_DATA, SIGNATURE_DESC, SIGNATURE_LOGE, SIGNATURE_ZERO,
};
use crate::error::Error;
use crc32c::crc32c;

/// Build a minimal log entry with the given descriptor count and data sectors.
fn build_entry(seq: u64, desc_count: u32, descriptors: &[u8], data_sectors: &[u8]) -> Vec<u8> {
    let header_size = 64;
    let desc_bytes = descriptors.len();
    // First descriptor sector: 4096 bytes total (64-byte header + up to 126 descriptors)
    let desc_sectors = if desc_bytes + header_size <= SECTOR_SIZE as usize {
        1
    } else {
        let overflow = desc_bytes + header_size - SECTOR_SIZE as usize;
        1 + overflow.div_ceil(SECTOR_SIZE as usize)
    };
    let desc_sector_bytes = desc_sectors * SECTOR_SIZE as usize;
    let total = desc_sector_bytes + data_sectors.len();
    let total_aligned = total.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;

    let mut buf = vec![0u8; total_aligned];

    // Signature "loge"
    buf[0..4].copy_from_slice(&SIGNATURE_LOGE.into_inner().to_le_bytes());
    // EntryLength
    buf[8..12].copy_from_slice(
        &u32::try_from(total_aligned)
            .expect("total_aligned fits u32")
            .to_le_bytes(),
    );
    // Tail (points to self)
    buf[12..16].copy_from_slice(&0u32.to_le_bytes());
    // SequenceNumber
    buf[16..24].copy_from_slice(&seq.to_le_bytes());
    // DescriptorCount
    buf[24..28].copy_from_slice(&desc_count.to_le_bytes());
    // LogGuid — all zeros for test
    // FlushedFileOffset
    buf[48..56].copy_from_slice(&0x1_0000_0000u64.to_le_bytes());
    // LastFileOffset
    buf[56..64].copy_from_slice(&0x1_0000_0000u64.to_le_bytes());

    // Copy descriptors after header
    buf[header_size..header_size + desc_bytes].copy_from_slice(descriptors);

    // Copy data sectors
    if !data_sectors.is_empty() {
        buf[desc_sector_bytes..desc_sector_bytes + data_sectors.len()]
            .copy_from_slice(data_sectors);
    }

    // Compute and write CRC-32C (with checksum field zeroed)
    let checksum = crc32c(&buf);
    buf[4..8].copy_from_slice(&checksum.to_le_bytes());

    buf
}

fn make_data_descriptor(seq: u64, file_offset: u64) -> [u8; 32] {
    let mut d = [0u8; 32];
    d[0..4].copy_from_slice(&SIGNATURE_DESC.into_inner().to_le_bytes());
    // TrailingBytes = 0xDEADBEEF
    d[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    // LeadingBytes = 0x0102030405060708
    d[8..16].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
    // FileOffset
    d[16..24].copy_from_slice(&file_offset.to_le_bytes());
    // SequenceNumber
    d[24..32].copy_from_slice(&seq.to_le_bytes());
    d
}

fn make_zero_descriptor(seq: u64, file_offset: u64, zero_length: u64) -> [u8; 32] {
    let mut z = [0u8; 32];
    z[0..4].copy_from_slice(&SIGNATURE_ZERO.into_inner().to_le_bytes());
    z[4..8].copy_from_slice(&0u32.to_le_bytes()); // reserved
    z[8..16].copy_from_slice(&zero_length.to_le_bytes());
    z[16..24].copy_from_slice(&file_offset.to_le_bytes());
    z[24..32].copy_from_slice(&seq.to_le_bytes());
    z
}

fn make_data_sector(seq: u64, fill: u8) -> [u8; 4096] {
    let mut s = [0u8; 4096];
    s[0..4].copy_from_slice(&SIGNATURE_DATA.into_inner().to_le_bytes());
    // SequenceHigh
    s[4..8].copy_from_slice(
        &u32::try_from(seq >> 32)
            .expect("upper sequence bits fit u32")
            .to_le_bytes(),
    );
    // Middle data: fill byte
    for b in &mut s[8..4092] {
        *b = fill;
    }
    // SequenceLow
    s[4092..4096].copy_from_slice(
        &u32::try_from(seq & u64::from(u32::MAX))
            .expect("lower sequence bits fit u32")
            .to_le_bytes(),
    );
    s
}

#[test]
fn empty_log() {
    let log = Log::new(&[]).unwrap();
    assert!(log.is_empty());
    assert_eq!(log.len(), 0);
    assert_eq!(log.entries().count(), 0);
}

#[test]
fn log_buffer_must_be_4kb_aligned() {
    let buf = vec![0u8; 100];
    assert!(Log::new(&buf).is_err());
}

#[test]
fn single_entry_header_fields() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    // Wrap in log buffer
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();
    let hdr = entry.header();

    assert_eq!(hdr.signature(), &SIGNATURE_LOGE.into_inner().to_le_bytes());
    assert_eq!(hdr.sequence_number(), 1);
    assert_eq!(hdr.descriptor_count(), 1);
    assert_eq!(hdr.tail(), 0);
}

#[test]
fn entry_checksum_valid() {
    let desc = make_data_descriptor(42, 0x2000);
    let sector = make_data_sector(42, 0xBB);
    let entry_buf = build_entry(42, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();
    assert!(entry.verify_checksum().is_ok());
}

#[test]
fn entry_checksum_invalid() {
    let desc = make_data_descriptor(42, 0x2000);
    let sector = make_data_sector(42, 0xBB);
    let mut entry_buf = build_entry(42, 1, &desc, &sector);
    // Corrupt a byte in the middle
    entry_buf[100] ^= 0xFF;
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();
    assert!(entry.verify_checksum().is_err());
}

#[test]
fn descriptors_data_and_zero() {
    let data_desc = make_data_descriptor(7, 0x3000);
    let zero_desc = make_zero_descriptor(7, 0x4000, 0x1000);
    let mut all_descs = Vec::new();
    all_descs.extend_from_slice(&data_desc);
    all_descs.extend_from_slice(&zero_desc);
    let sector = make_data_sector(7, 0xCC);
    let entry_buf = build_entry(7, 2, &all_descs, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let desc0 = entry.descriptor(0).unwrap();
    match &desc0 {
        Descriptor::Data(d) => {
            assert_eq!(d.file_offset(), 0x3000);
            assert_eq!(d.sequence_number(), 7);
            assert_eq!(d.trailing_bytes(), 0xDEAD_BEEF);
            assert_eq!(d.leading_bytes(), 0x0102_0304_0506_0708);
        }
        Descriptor::Zero(_) => panic!("expected Data descriptor"),
    }

    let desc1 = entry.descriptor(1).unwrap();
    match &desc1 {
        Descriptor::Zero(z) => {
            assert_eq!(z.file_offset(), 0x4000);
            assert_eq!(z.zero_length(), 0x1000);
            assert_eq!(z.sequence_number(), 7);
        }
        Descriptor::Data(_) => panic!("expected Zero descriptor"),
    }
}

#[test]
fn descriptors_iterator() {
    let data_desc = make_data_descriptor(5, 0x5000);
    let all_descs = &data_desc[..];
    let sector = make_data_sector(5, 0xDD);
    let entry_buf = build_entry(5, 1, all_descs, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let descs: Vec<_> = entry.descriptors().collect();
    assert_eq!(descs.len(), 1);
    assert!(descs[0].is_ok());
}

#[test]
fn data_sectors_iterator() {
    let desc = make_data_descriptor(10, 0x6000);
    let sector = make_data_sector(10, 0xEE);
    let entry_buf = build_entry(10, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let sectors: Vec<_> = entry.data().collect();
    assert_eq!(sectors.len(), 1);
    let s = &sectors[0];
    assert_eq!(s.signature(), &SIGNATURE_DATA.into_inner().to_le_bytes());
    assert_eq!(s.sequence_number(), 10);
    assert_eq!(
        u32::from_le_bytes(s.data[4..8].try_into().unwrap()),
        u32::try_from(10u64 >> 32).expect("upper sequence bits fit u32")
    );
    assert_eq!(
        u32::from_le_bytes(s.data[4092..4096].try_into().unwrap()),
        10u32
    );
}

#[test]
fn data_sector_assembly() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let data_sectors: Vec<_> = entry.data().collect();
    let data_sec = &data_sectors[0];

    let assembled_data = data_sec.data();
    assert_eq!(assembled_data.len(), 4096);

    // Leading bytes from descriptor
    assert_eq!(&assembled_data[0..8], &desc[8..16]);
    // Middle from sector
    assert_eq!(&assembled_data[8..4092], &sector[8..4092]);
    // Trailing bytes from descriptor
    assert_eq!(&assembled_data[4092..4096], &desc[4..8]);

    // Cross-check with DataSectorAssembly
    let Descriptor::Data(data_desc) = entry.descriptor(0).unwrap() else {
        panic!("expected data descriptor");
    };
    let classic = DataSectorAssembly::new(&data_desc, data_sec);
    assert_eq!(assembled_data, classic.data());
}

#[test]
fn multiple_entries_in_log() {
    let desc1 = make_data_descriptor(1, 0x1000);
    let sector1 = make_data_sector(1, 0xAA);
    let entry1 = build_entry(1, 1, &desc1, &sector1);

    let desc2 = make_data_descriptor(2, 0x2000);
    let sector2 = make_data_sector(2, 0xBB);
    let entry2 = build_entry(2, 1, &desc2, &sector2);

    let mut log_buf = Vec::new();
    log_buf.extend_from_slice(&entry1);
    log_buf.extend_from_slice(&entry2);
    // Pad to 4KB alignment if needed
    while log_buf.len() % SECTOR_SIZE as usize != 0 {
        log_buf.push(0);
    }

    let log = Log::new(&log_buf).unwrap();
    let entries: Vec<_> = log.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].header().sequence_number(), 1);
    assert_eq!(entries[1].header().sequence_number(), 2);
}

#[test]
fn entry_index_out_of_bounds() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let err = log.entry(1).unwrap_err();
    assert!(
        matches!(err, Error::InvalidParameter(ref msg) if msg.contains("out of bounds") || msg.contains("not found"))
    );
}

#[test]
fn entry_out_of_bounds_returns_invalid_parameter() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let err = log.entry(5).unwrap_err();
    assert!(matches!(err, Error::InvalidParameter(_)));
}

#[test]
fn descriptor_index_out_of_bounds() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();
    assert!(entry.descriptor(1).is_err());
}

#[test]
fn descriptor_unknown_signature_returns_log_entry_corrupted() {
    // Build a 32-byte descriptor with "xxxx" signature instead of "desc" or "zero"
    let mut bad_desc = make_data_descriptor(1, 0x1000);
    bad_desc[0..4].copy_from_slice(b"xxxx");
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &bad_desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();
    let err = entry.descriptor(0).unwrap_err();
    assert!(
        matches!(err, crate::error::Error::LogEntryCorrupted(_)),
        "expected LogEntryCorrupted, got {err:?}"
    );
}

#[test]
fn no_data_sectors_for_zero_only_entry() {
    let zero_desc = make_zero_descriptor(3, 0x5000, 0x2000);
    let entry_buf = build_entry(3, 1, &zero_desc, &[]);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let sectors: Vec<_> = entry.data().collect();
    assert_eq!(sectors.len(), 0);
}

#[test]
fn zero_descriptors_count_doesnt_affect_data_sectors() {
    // 1 data desc + 2 zero desc → 1 data sector
    let d1 = make_data_descriptor(5, 0x1000);
    let z1 = make_zero_descriptor(5, 0x2000, 0x1000);
    let z2 = make_zero_descriptor(5, 0x3000, 0x2000);
    let mut all_descs = Vec::new();
    all_descs.extend_from_slice(&d1);
    all_descs.extend_from_slice(&z1);
    all_descs.extend_from_slice(&z2);
    let sector = make_data_sector(5, 0xFF);
    let entry_buf = build_entry(5, 3, &all_descs, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    assert_eq!(entry.header().descriptor_count(), 3);
    let sectors: Vec<_> = entry.data().collect();
    assert_eq!(sectors.len(), 1);
}

#[test]
fn entry_with_no_descriptors() {
    let entry_buf = build_entry(1, 0, &[], &[]);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    assert_eq!(entry.header().descriptor_count(), 0);
    let descs: Vec<_> = entry.descriptors().collect();
    assert_eq!(descs.len(), 0);
    let sectors: Vec<_> = entry.data().collect();
    assert_eq!(sectors.len(), 0);
}

#[test]
fn descriptor_bytes_second_sector() {
    // 127 data descriptors → first sector: 126, second sector: 1
    let mut all_descs = Vec::new();
    for i in 0..127u64 {
        all_descs.extend_from_slice(&make_data_descriptor(1, i * 0x1000));
    }
    // 127 data descriptors → 127 data sectors
    let mut all_sectors = Vec::new();
    for _ in 0..127 {
        all_sectors.extend_from_slice(&make_data_sector(1, 0xAA));
    }
    let entry_buf = build_entry(1, 127, &all_descs, &all_sectors);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    // Descriptor 126 should be in second descriptor sector
    let d126 = entry.descriptor(126).unwrap();
    match &d126 {
        Descriptor::Data(d) => {
            assert_eq!(d.file_offset(), 126 * 0x1000);
        }
        Descriptor::Zero(_) => panic!("expected data"),
    }

    // All 127 data sectors should be present
    let sectors: Vec<_> = entry.data().collect();
    assert_eq!(sectors.len(), 127);
}

#[test]
fn data_sector_data_method_returns_4096_bytes() {
    let desc = make_data_descriptor(1, 0x1000);
    let sector = make_data_sector(1, 0xAA);
    let entry_buf = build_entry(1, 1, &desc, &sector);
    let log = Log::new(&entry_buf).unwrap();
    let entry = log.entry(0).unwrap();

    let Descriptor::Data(data_desc) = entry.descriptor(0).unwrap() else {
        panic!("expected data descriptor");
    };
    let data_sectors: Vec<_> = entry.data().collect();
    let data_sec = &data_sectors[0];

    let assembled = data_sec.data();

    // Total length is exactly 4096
    assert_eq!(assembled.len(), 4096);

    // First 8 bytes match LeadingBytes from descriptor
    assert_eq!(&assembled[0..8], &desc[8..16]);

    // Last 4 bytes match TrailingBytes from descriptor
    assert_eq!(&assembled[4092..4096], &desc[4..8]);

    // Middle 4084 bytes match the sector's middle data
    assert_eq!(&assembled[8..4092], &sector[8..4092]);

    // Must produce identical result to DataSectorAssembly
    let classic = DataSectorAssembly::new(&data_desc, data_sec);
    assert_eq!(assembled, classic.data());
}
