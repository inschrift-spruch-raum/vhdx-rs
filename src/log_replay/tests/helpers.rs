use super::*;
use crate::constants::{SIGNATURE_DATA, SIGNATURE_DESC, SIGNATURE_LOGE, SIGNATURE_ZERO};
use crc32c::crc32c;

/// Build a complete log entry buffer (header + descriptors + data sectors).
pub(super) fn build_log_entry(
    seq: u64,
    tail_offset: u32,
    desc_specs: &[(bool, u64, u64)], // (is_data, file_offset, extra) — extra = 0 for data, zero_length for zero
    fill_byte: u8,
    log_guid: &Guid,
) -> Vec<u8> {
    let header_size = 64;
    let desc_count = u32::try_from(desc_specs.len()).expect("descriptor count fits u32");

    // Build descriptors
    let mut desc_bytes = Vec::new();
    for &(is_data, file_offset, extra) in desc_specs {
        let mut d = [0u8; 32];
        if is_data {
            d[0..4].copy_from_slice(&SIGNATURE_DESC.into_inner().to_le_bytes());
            d[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // trailing
            d[8..16].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes()); // leading
            d[16..24].copy_from_slice(&file_offset.to_le_bytes());
            d[24..32].copy_from_slice(&seq.to_le_bytes());
        } else {
            d[0..4].copy_from_slice(&SIGNATURE_ZERO.into_inner().to_le_bytes());
            d[4..8].copy_from_slice(&0u32.to_le_bytes()); // reserved
            d[8..16].copy_from_slice(&extra.to_le_bytes()); // zero_length
            d[16..24].copy_from_slice(&file_offset.to_le_bytes());
            d[24..32].copy_from_slice(&seq.to_le_bytes());
        }
        desc_bytes.extend_from_slice(&d);
    }

    // Build data sectors (one per data descriptor)
    let data_desc_count = desc_specs.iter().filter(|(is_data, _, _)| *is_data).count();
    let mut data_bytes = Vec::new();
    for _ in 0..data_desc_count {
        let mut s = [0u8; 4096];
        s[0..4].copy_from_slice(&SIGNATURE_DATA.into_inner().to_le_bytes());
        s[4..8].copy_from_slice(
            &u32::try_from(seq >> 32)
                .expect("upper sequence bits fit u32")
                .to_le_bytes(),
        );
        for b in &mut s[8..4092] {
            *b = fill_byte;
        }
        s[4092..4096].copy_from_slice(
            &u32::try_from(seq & u64::from(u32::MAX))
                .expect("lower sequence bits fit u32")
                .to_le_bytes(),
        );
        data_bytes.extend_from_slice(&s);
    }

    // Calculate descriptor sectors
    let desc_sectors = if desc_bytes.len() + header_size <= SECTOR_SIZE.into() {
        1
    } else {
        let overflow = desc_bytes.len() + header_size - SECTOR_SIZE as usize;
        1 + overflow.div_ceil(SECTOR_SIZE.into())
    };
    let desc_sector_bytes = desc_sectors * SECTOR_SIZE as usize;
    let total = desc_sector_bytes + data_bytes.len();
    let total_aligned = total.div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;

    let mut buf = vec![0u8; total_aligned];

    // Header
    buf[0..4].copy_from_slice(&SIGNATURE_LOGE.into_inner().to_le_bytes());
    buf[8..12].copy_from_slice(
        &u32::try_from(total_aligned)
            .expect("total_aligned fits u32")
            .to_le_bytes(),
    );
    buf[12..16].copy_from_slice(&tail_offset.to_le_bytes());
    buf[16..24].copy_from_slice(&seq.to_le_bytes());
    buf[24..28].copy_from_slice(&desc_count.to_le_bytes());
    buf[32..48].copy_from_slice(&log_guid.to_bytes());
    buf[48..56].copy_from_slice(&0x1_0000_0000u64.to_le_bytes()); // FlushedFileOffset
    buf[56..64].copy_from_slice(&0x2_0000_0000u64.to_le_bytes()); // LastFileOffset

    // Descriptors
    buf[header_size..header_size + desc_bytes.len()].copy_from_slice(&desc_bytes);

    // Data sectors
    if !data_bytes.is_empty() {
        buf[desc_sector_bytes..desc_sector_bytes + data_bytes.len()].copy_from_slice(&data_bytes);
    }

    // CRC-32C
    let checksum = crc32c(&buf);
    buf[4..8].copy_from_slice(&checksum.to_le_bytes());

    buf
}

pub(super) fn test_log_guid() -> Guid {
    Guid::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ])
}

/// Build a log buffer containing multiple entries.
pub(super) fn build_log_buffer(entries: Vec<Vec<u8>>) -> Vec<u8> {
    let mut buf = Vec::new();
    for e in entries {
        buf.extend_from_slice(&e);
    }
    // Pad to 4KB alignment
    while buf.len() % SECTOR_SIZE as usize != 0 {
        buf.push(0);
    }
    buf
}
