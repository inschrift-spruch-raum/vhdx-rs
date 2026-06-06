use super::*;
use crate::constants::{
    HEADER_SIZE, HEADER1_OFFSET, HEADER2_OFFSET, METADATA_TABLE_SIZE, MIB, REGION_TABLE_SIZE,
    REGION_TABLE1_OFFSET, REGION_TABLE2_OFFSET,
};
use bitvec::prelude::*;
use crc32c::crc32c;

pub(super) struct Encoded {
    pub(super) key: Vec<u8>,
    pub(super) val: Vec<u8>,
}

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

/// Build a minimal valid VHDX file in memory for validation testing.
pub(super) fn build_test_vhdx() -> Vec<u8> {
    let virtual_size: u64 = 1024 * 1024 * 1024; // 1 GB
    let block_size: u32 = 32 * 1024 * 1024; // 32 MB
    let logical_sector_size: u32 = 4096;
    let bat_entry_count = virtual_size.div_ceil(u64::from(block_size));
    let chunk_ratio = (1u64 << 23) * u64::from(logical_sector_size) / u64::from(block_size);
    let sector_bitmap_count = bat_entry_count.div_ceil(chunk_ratio);
    let total_bat_entries =
        usize::try_from(bat_entry_count + sector_bitmap_count).expect("BAT entry count fits usize");
    let bat_bytes = total_bat_entries * 8;
    let bat_size = std::cmp::max(
        u32::try_from(
            u64::try_from(bat_bytes)
                .expect("bat bytes fit u64")
                .div_ceil(u64::from(MIB)),
        )
        .expect("BAT size in MB units fits u32"),
        1,
    ) * MIB;

    let header_size = HEADER_SIZE;
    let region_table_size = REGION_TABLE_SIZE;

    let header1_offset = HEADER1_OFFSET;
    let header2_offset = HEADER2_OFFSET;
    let rt1_offset = REGION_TABLE1_OFFSET;
    let rt2_offset = REGION_TABLE2_OFFSET;
    let log_offset: u64 = u64::from(MIB);
    let log_length: u32 = MIB;
    let bat_offset: u64 = 2 * u64::from(MIB);
    let metadata_offset: u64 = bat_offset + u64::from(bat_size);
    let metadata_size: u32 = MIB;

    let file_end = metadata_offset + u64::from(metadata_size);
    let mut buf = vec![0u8; usize::try_from(file_end).expect("file size fits usize")];

    // File type identifier "vhdxfile"
    buf[0..8].copy_from_slice(b"vhdxfile");

    // Write headers
    let _ = (header_size, region_table_size, log_offset, log_length);

    write_header(&mut buf, header1_offset as usize, 5);
    write_header(&mut buf, header2_offset as usize, 3);

    // Write region tables
    write_region_table(
        &mut buf,
        rt1_offset as usize,
        bat_offset,
        bat_size,
        metadata_offset,
        metadata_size,
    );
    write_region_table(
        &mut buf,
        rt2_offset as usize,
        bat_offset,
        bat_size,
        metadata_offset,
        metadata_size,
    );

    // Write minimal BAT: payload entries = FullyPresent with block-aligned
    // offsets, sector bitmap entries = NotPresent.
    let bat_start = usize::try_from(bat_offset).expect("BAT offset fits usize");
    let block_size_mb = u64::from(block_size / MIB);
    let metadata_end_mb = (metadata_offset + u64::from(metadata_size)).div_ceil(u64::from(MIB));
    // Align first payload offset to block_size boundary.
    let first_payload_mb = metadata_end_mb.div_ceil(block_size_mb) * block_size_mb;
    let mut sb_written: u64 = 0;
    let mut payload_idx: u64 = 0;
    for i in 0..total_bat_entries {
        let entry_offset = bat_start + i * 8;
        let payloads_before = u64::try_from(i).expect("index fits u64") - sb_written;
        let is_sb = payloads_before > 0
            && payloads_before.is_multiple_of(chunk_ratio)
            && sb_written < sector_bitmap_count;
        let entry_val: u64 = if is_sb {
            // Sector bitmap: NotPresent
            sb_written += 1;
            0
        } else {
            // Payload: FullyPresent at block-aligned offset
            let offset_mb = first_payload_mb + payload_idx * block_size_mb;
            payload_idx += 1;
            {
                let mut entry_buf = [0u8; 8];
                let bits = entry_buf.view_bits_mut::<Lsb0>();
                bits[0..3].store::<u8>(6u8); // FullyPresent state
                bits[20..64].store::<u64>(offset_mb);
                u64::from_le_bytes(entry_buf)
            }
        };
        buf[entry_offset..entry_offset + 8].copy_from_slice(&entry_val.to_le_bytes());
    }

    // Write metadata table + items
    write_metadata(
        &mut buf,
        usize::try_from(metadata_offset).expect("metadata offset fits usize"),
        block_size,
        logical_sector_size,
    );

    buf
}

pub(super) fn write_header(buf: &mut [u8], offset: usize, seq: u64) {
    let header_size = HEADER_SIZE;
    let slice = &mut buf[offset..][..header_size as usize];
    slice[..4].copy_from_slice(b"head");
    slice[4..8].copy_from_slice(&0u32.to_le_bytes());
    slice[8..16].copy_from_slice(&seq.to_le_bytes());
    slice[64..66].copy_from_slice(&0u16.to_le_bytes()); // log_version
    slice[66..68].copy_from_slice(&1u16.to_le_bytes()); // version
    slice[68..72].copy_from_slice(&MIB.to_le_bytes()); // log_length
    slice[72..80].copy_from_slice(&u64::from(MIB).to_le_bytes()); // log_offset

    let checksum = crc32c(slice);
    slice[4..8].copy_from_slice(&checksum.to_le_bytes());
}

pub(super) fn write_region_table(
    buf: &mut [u8], offset: usize, bat_offset: u64, bat_size: u32, metadata_offset: u64,
    metadata_size: u32,
) {
    let region_table_size = REGION_TABLE_SIZE;
    let slice = &mut buf[offset..][..region_table_size as usize];

    slice[..4].copy_from_slice(b"regi");
    slice[4..8].copy_from_slice(&0u32.to_le_bytes()); // checksum placeholder
    slice[8..12].copy_from_slice(&2u32.to_le_bytes()); // 2 entries
    slice[12..16].copy_from_slice(&0u32.to_le_bytes()); // reserved

    // BAT region GUID
    let bat_guid: [u8; 16] = [
        0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A,
        0x08,
    ];
    // Metadata region GUID
    let meta_guid: [u8; 16] = [
        0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88,
        0x6E,
    ];

    let mut entry_off = 16;
    // Entry 0: BAT
    slice[entry_off..entry_off + 16].copy_from_slice(&bat_guid);
    slice[entry_off + 16..entry_off + 24].copy_from_slice(&bat_offset.to_le_bytes());
    slice[entry_off + 24..entry_off + 28].copy_from_slice(&bat_size.to_le_bytes());
    slice[entry_off + 28..entry_off + 32].copy_from_slice(&1u32.to_le_bytes()); // required

    entry_off += 32;
    // Entry 1: Metadata
    slice[entry_off..entry_off + 16].copy_from_slice(&meta_guid);
    slice[entry_off + 16..entry_off + 24].copy_from_slice(&metadata_offset.to_le_bytes());
    slice[entry_off + 24..entry_off + 28].copy_from_slice(&metadata_size.to_le_bytes());
    slice[entry_off + 28..entry_off + 32].copy_from_slice(&1u32.to_le_bytes()); // required

    let checksum = crc32c(slice);
    slice[4..8].copy_from_slice(&checksum.to_le_bytes());
}

pub(super) fn write_metadata(
    buf: &mut [u8], offset: usize, block_size: u32, logical_sector_size: u32,
) {
    let metadata_table_size = METADATA_TABLE_SIZE;

    // Table header
    buf[offset..offset + 8].copy_from_slice(b"metadata");
    buf[offset + 10..offset + 12].copy_from_slice(&6u16.to_le_bytes()); // 6 entries

    // Write 6 table entries. Item offsets are relative to the start of the
    // metadata region (which includes the 64KB table).
    let mut entry_off = offset + 32;
    let item_base = metadata_table_size; // items start right after the 64KB table

    // Entry 0: FileParameters (relative offset = 64KB+0, length=8)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::FILE_PARAMETERS,
        item_base,
        8,
        0x0000_0004, // is_required
    );

    // Entry 1: VirtualDiskSize (relative offset = 64KB+8, length=8)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::VIRTUAL_DISK_SIZE,
        item_base + 8,
        8,
        0x0000_0006, // is_virtual_disk + is_required
    );

    // Entry 2: VirtualDiskId (relative offset = 64KB+16, length=16)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::VIRTUAL_DISK_ID,
        item_base + 16,
        16,
        0x0000_0006,
    );

    // Entry 3: LogicalSectorSize (relative offset = 64KB+32, length=4)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::LOGICAL_SECTOR_SIZE,
        item_base + 32,
        4,
        0x0000_0006,
    );

    // Entry 4: PhysicalSectorSize (relative offset = 64KB+40, length=4)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::PHYSICAL_SECTOR_SIZE,
        item_base + 40,
        4,
        0x0000_0006,
    );

    // Entry 5: ParentLocator (empty, offset=0, length=0)
    write_metadata_entry(
        buf,
        &mut entry_off,
        &StandardItems::PARENT_LOCATOR,
        0,
        0,
        0x0000_0004,
    );

    // FileParameters per MS-VHDX §2.6.2.1: block_size first, flags second
    let items_base = offset + metadata_table_size as usize;
    let fp_flags: u32 = 0; // dynamic disk
    buf[items_base..items_base + 4].copy_from_slice(&block_size.to_le_bytes());
    buf[items_base + 4..items_base + 8].copy_from_slice(&fp_flags.to_le_bytes());

    // VirtualDiskSize: 1 GB
    let disk_size: u64 = 1024 * 1024 * 1024;
    buf[items_base + 8..items_base + 16].copy_from_slice(&disk_size.to_le_bytes());

    // VirtualDiskId: zeros (already zeroed)
    // LogicalSectorSize
    buf[items_base + 32..items_base + 36].copy_from_slice(&logical_sector_size.to_le_bytes());
    // PhysicalSectorSize
    buf[items_base + 40..items_base + 44].copy_from_slice(&4096u32.to_le_bytes());
}

pub(super) fn write_metadata_entry(
    buf: &mut [u8], entry_off: &mut usize, guid: &Guid, item_offset: u32, length: u32, flags: u32,
) {
    buf[*entry_off..*entry_off + 16].copy_from_slice(&guid.to_bytes());
    buf[*entry_off + 16..*entry_off + 20].copy_from_slice(&item_offset.to_le_bytes());
    buf[*entry_off + 20..*entry_off + 24].copy_from_slice(&length.to_le_bytes());
    buf[*entry_off + 24..*entry_off + 28].copy_from_slice(&flags.to_le_bytes());
    // reserved (4 bytes): 0
    *entry_off += 32;
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
