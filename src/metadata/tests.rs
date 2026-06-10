use super::*;
use crate::constants::{
    KV_ENTRY_SIZE, LOCATOR_HEADER_SIZE, METADATA_TABLE_SIZE, TABLE_ENTRY_SIZE, TABLE_HEADER_SIZE,
};
use crate::metadata::core::decode_utf16le;
use crate::types::Guid;

/// Build a minimal valid metadata region for testing.
fn build_test_metadata() -> Vec<u8> {
    let mut buf = vec![0u8; METADATA_TABLE_SIZE as usize + 4096];

    // -- Table Header (32 bytes) --
    buf[0..8].copy_from_slice(b"metadata"); // signature
    // reserved (2 bytes): 0
    // entry_count (2 bytes): 6
    buf[10..12].copy_from_slice(&6u16.to_le_bytes());
    // reserved2 (20 bytes): 0

    let mut off: usize = TABLE_HEADER_SIZE as usize;

    // Helper to write an entry
    let mut write_entry = |guid: &Guid, item_offset: u32, length: u32, flags: u32| {
        buf[off..off + 16].copy_from_slice(&guid.to_bytes());
        buf[off + 16..off + 20].copy_from_slice(&item_offset.to_le_bytes());
        buf[off + 20..off + 24].copy_from_slice(&length.to_le_bytes());
        buf[off + 24..off + 28].copy_from_slice(&flags.to_le_bytes());
        // reserved (4 bytes): 0
        off += TABLE_ENTRY_SIZE as usize;
    };

    // Entry 0: File Parameters (at offset 64KB, length 8)
    write_entry(
        &StandardItems::FILE_PARAMETERS,
        METADATA_TABLE_SIZE,
        8,
        0x0000_0004, // is_virtual_disk=0, is_required=1 (bit 2)
    );

    // Entry 1: Virtual Disk Size (at 64KB+8, length 8)
    write_entry(
        &StandardItems::VIRTUAL_DISK_SIZE,
        METADATA_TABLE_SIZE + 8,
        8,
        0x0000_0006, // is_virtual_disk + is_required
    );

    // Entry 2: Virtual Disk ID (at 64KB+24, length 16)
    write_entry(
        &StandardItems::VIRTUAL_DISK_ID,
        METADATA_TABLE_SIZE + 24,
        16,
        0x0000_0006,
    );

    // Entry 3: Logical Sector Size (at 64KB+40, length 4)
    write_entry(
        &StandardItems::LOGICAL_SECTOR_SIZE,
        METADATA_TABLE_SIZE + 40,
        4,
        0x0000_0006,
    );

    // Entry 4: Physical Sector Size (at 64KB+48, length 4)
    write_entry(
        &StandardItems::PHYSICAL_SECTOR_SIZE,
        METADATA_TABLE_SIZE + 48,
        4,
        0x0000_0006,
    );

    // Entry 5: Parent Locator (empty, offset=0, length=0)
    write_entry(&StandardItems::PARENT_LOCATOR, 0, 0, 0x0000_0004);

    // -- Metadata Items --
    let items_base = METADATA_TABLE_SIZE as usize;

    // File Parameters per MS-VHDX §2.6.2.1: block_size first, flags second
    let fp_block = (32 * 1024 * 1024u32).to_le_bytes();
    let fp_flags = 0u32.to_le_bytes();
    buf[items_base..items_base + 4].copy_from_slice(&fp_block);
    buf[items_base + 4..items_base + 8].copy_from_slice(&fp_flags);

    // Virtual Disk Size: 10 GB
    let disk_size = (10u64 * 1024 * 1024 * 1024).to_le_bytes();
    buf[items_base + 8..items_base + 16].copy_from_slice(&disk_size);

    // Virtual Disk ID: all zeros GUID
    // (already zeroed)

    // Logical Sector Size: 4096
    buf[items_base + 40..items_base + 44].copy_from_slice(&4096u32.to_le_bytes());

    // Physical Sector Size: 4096
    buf[items_base + 48..items_base + 52].copy_from_slice(&4096u32.to_le_bytes());

    buf
}

#[test]
fn metadata_signature_valid() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    let header = meta.table().header();
    assert_eq!(header.signature(), b"metadata");
    header.validate_signature().unwrap();
}

#[test]
fn metadata_entry_count() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    assert_eq!(meta.table().header().entry_count(), 6);
}

#[test]
fn metadata_entries_iterator_count() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    assert_eq!(meta.table().entries().count(), 6);
}

#[test]
fn metadata_entry_lookup_found() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    let entry = meta.table().entry(&StandardItems::FILE_PARAMETERS).unwrap();
    assert_eq!(entry.offset(), METADATA_TABLE_SIZE);
    assert_eq!(entry.length(), 8);
}

#[test]
fn metadata_entry_lookup_not_found() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    let unknown = Guid::from_bytes([0xFF; 16]);
    let result = meta.table().entry(&unknown);
    assert!(result.is_err());
}

#[test]
fn file_parameters_dynamic_disk() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    let fp = meta.items().file_parameters().unwrap();
    assert_eq!(fp.block_size(), 32 * 1024 * 1024);
    assert!(!fp.leave_block_allocated());
    assert!(!fp.has_parent());
}

#[test]
fn virtual_disk_size() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    assert_eq!(
        meta.items().virtual_disk_size().unwrap(),
        10 * 1024 * 1024 * 1024
    );
}

#[test]
fn logical_sector_size() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    assert_eq!(meta.items().logical_sector_size().unwrap(), 4096);
}

#[test]
fn physical_sector_size() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    assert_eq!(meta.items().physical_sector_size().unwrap(), 4096);
}

#[test]
fn entry_flags() {
    // Per MS-VHDX §2.6.1.2 diagram: A=IsUser(bit0), B=IsVirtualDisk(bit1),
    // C=IsRequired(bit2).
    let bytes = 0x0000_0007u32.to_le_bytes(); // bits 0 + 1 + 2 = all three set
    let flags = EntryFlags { data: &bytes };
    assert!(flags.is_user());
    assert!(flags.is_virtual_disk());
    assert!(flags.is_required());

    let bytes = 0x0000_0000u32.to_le_bytes();
    let flags = EntryFlags { data: &bytes };
    assert!(!flags.is_user());
    assert!(!flags.is_virtual_disk());
    assert!(!flags.is_required());

    // Reserved bits 3-31: detection
    let bytes = 0x0000_0008u32.to_le_bytes(); // bit 3 set
    let flags = EntryFlags { data: &bytes };
    assert!(flags.has_reserved_bits());

    let bytes = 0xFFFF_FFF8u32.to_le_bytes(); // all reserved bits set
    let flags = EntryFlags { data: &bytes };
    assert!(flags.has_reserved_bits());

    let bytes = 0x0000_0007u32.to_le_bytes(); // only valid bits
    let flags = EntryFlags { data: &bytes };
    assert!(!flags.has_reserved_bits());
}

#[test]
fn parent_locator_empty() {
    let buf = build_test_metadata();
    let meta = Metadata::new(&buf).unwrap();
    // Parent locator has length=0, so item_data returns Some(&[])
    let locator = meta.items().parent_locator().unwrap();
    // Header data is 0 bytes - accessing it would panic, but key_value_count will be 0
    // since data.len() < LOCATOR_HEADER_SIZE
    assert_eq!(locator.entries().count(), 0);
}

#[test]
fn utf16le_decoding() {
    // "hello" in UTF-16LE
    let data: Vec<u8> = "hello".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let result = decode_utf16le(&data, 0, data.len()).unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn parent_locator_with_entries() {
    // Build a parent locator with one key-value pair: relative_path -> "Cargo.toml"
    let key = "relative_path";
    let value = "Cargo.toml";
    let key_utf16: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let value_utf16: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();

    // Header (20) + 1 KV entry (12) + key data + value data
    let kv_data_start = LOCATOR_HEADER_SIZE as usize + KV_ENTRY_SIZE as usize;
    let total_len = kv_data_start + key_utf16.len() + value_utf16.len();

    let mut buf = vec![0u8; METADATA_TABLE_SIZE as usize + total_len];

    // Table header
    buf[0..8].copy_from_slice(b"metadata");
    buf[10..12].copy_from_slice(&1u16.to_le_bytes()); // 1 entry

    // Entry 0: Parent Locator
    let entry_off = TABLE_HEADER_SIZE as usize;
    buf[entry_off..entry_off + 16].copy_from_slice(&StandardItems::PARENT_LOCATOR.to_bytes());
    buf[entry_off + 16..entry_off + 20].copy_from_slice(&METADATA_TABLE_SIZE.to_le_bytes());
    buf[entry_off + 20..entry_off + 24].copy_from_slice(
        &u32::try_from(total_len)
            .expect("total length fits u32")
            .to_le_bytes(),
    );
    buf[entry_off + 24..entry_off + 28].copy_from_slice(&0x0000_0004u32.to_le_bytes());

    // Locator data
    let base = METADATA_TABLE_SIZE as usize;
    // Locator header
    buf[base..base + 16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
    buf[base + 16..base + 18].copy_from_slice(&0u16.to_le_bytes()); // reserved
    buf[base + 18..base + 20].copy_from_slice(&1u16.to_le_bytes()); // 1 kv entry

    // KV entry: key at kv_data_start, value at kv_data_start + key_len
    let kv_entry_off = base + LOCATOR_HEADER_SIZE as usize;
    buf[kv_entry_off..kv_entry_off + 4].copy_from_slice(
        &u32::try_from(kv_data_start)
            .expect("key/value data start fits u32")
            .to_le_bytes(),
    );
    buf[kv_entry_off + 4..kv_entry_off + 8].copy_from_slice(
        &u32::try_from(kv_data_start + key_utf16.len())
            .expect("value offset fits u32")
            .to_le_bytes(),
    );
    buf[kv_entry_off + 8..kv_entry_off + 10].copy_from_slice(
        &u16::try_from(key_utf16.len())
            .expect("key length fits u16")
            .to_le_bytes(),
    );
    buf[kv_entry_off + 10..kv_entry_off + 12].copy_from_slice(
        &u16::try_from(value_utf16.len())
            .expect("value length fits u16")
            .to_le_bytes(),
    );

    // Key and value data
    let key_off = base + kv_data_start;
    buf[key_off..key_off + key_utf16.len()].copy_from_slice(&key_utf16);
    let val_off = key_off + key_utf16.len();
    buf[val_off..val_off + value_utf16.len()].copy_from_slice(&value_utf16);

    // Parse
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    assert_eq!(locator.header().key_value_count(), 1);

    let kv_entries: Vec<_> = locator.entries().collect();
    assert_eq!(kv_entries.len(), 1);

    let kv = &kv_entries[0];
    assert_eq!(kv.key(locator.key_value_data()).unwrap(), "relative_path");
    assert_eq!(kv.value(locator.key_value_data()).unwrap(), "Cargo.toml");
}

#[test]
fn metadata_region_too_small() {
    let buf = vec![0u8; 100];
    assert!(Metadata::new(&buf).is_err());
}

/// Helper: build a parent locator buffer with arbitrary key-value pairs.
/// Each pair is (&str, &str) for (key, value).
fn build_locator_buf(entries: &[(&str, &str)]) -> Vec<u8> {
    let count = entries.len();
    // Encode all keys and values as UTF-16LE
    let encoded: Vec<(Vec<u8>, Vec<u8>)> = entries
        .iter()
        .map(|(k, v)| {
            let ku: Vec<u8> = k.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let vu: Vec<u8> = v.encode_utf16().flat_map(u16::to_le_bytes).collect();
            (ku, vu)
        })
        .collect();

    let kv_data_start = LOCATOR_HEADER_SIZE as usize + count * (KV_ENTRY_SIZE as usize);
    let total_data_len: usize = encoded.iter().map(|(k, v)| k.len() + v.len()).sum();
    let total_len = kv_data_start + total_data_len;

    let mut buf = vec![0u8; METADATA_TABLE_SIZE as usize + total_len];

    // Table header
    buf[0..8].copy_from_slice(b"metadata");
    buf[10..12].copy_from_slice(&1u16.to_le_bytes()); // 1 entry

    // Entry 0: Parent Locator
    let entry_off = TABLE_HEADER_SIZE as usize;
    buf[entry_off..entry_off + 16].copy_from_slice(&StandardItems::PARENT_LOCATOR.to_bytes());
    buf[entry_off + 16..entry_off + 20].copy_from_slice(&METADATA_TABLE_SIZE.to_le_bytes());
    buf[entry_off + 20..entry_off + 24].copy_from_slice(
        &u32::try_from(total_len)
            .expect("total length fits u32")
            .to_le_bytes(),
    );
    buf[entry_off + 24..entry_off + 28].copy_from_slice(&0x0000_0004u32.to_le_bytes());

    // Locator header
    let base = METADATA_TABLE_SIZE as usize;
    buf[base..base + 16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
    buf[base + 16..base + 18].copy_from_slice(&0u16.to_le_bytes()); // reserved
    buf[base + 18..base + 20].copy_from_slice(
        &u16::try_from(count)
            .expect("entry count fits u16")
            .to_le_bytes(),
    );

    // Write KV entries and data
    let mut data_offset = kv_data_start;
    for (i, (key_bytes, val_bytes)) in encoded.iter().enumerate() {
        let kv_entry_off = base + LOCATOR_HEADER_SIZE as usize + i * KV_ENTRY_SIZE as usize;
        buf[kv_entry_off..kv_entry_off + 4].copy_from_slice(
            &u32::try_from(data_offset)
                .expect("data offset fits u32")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 4..kv_entry_off + 8].copy_from_slice(
            &u32::try_from(data_offset + key_bytes.len())
                .expect("value offset fits u32")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 8..kv_entry_off + 10].copy_from_slice(
            &u16::try_from(key_bytes.len())
                .expect("key length fits u16")
                .to_le_bytes(),
        );
        buf[kv_entry_off + 10..kv_entry_off + 12].copy_from_slice(
            &u16::try_from(val_bytes.len())
                .expect("value length fits u16")
                .to_le_bytes(),
        );

        let koff = base + data_offset;
        buf[koff..koff + key_bytes.len()].copy_from_slice(key_bytes);
        let voff = koff + key_bytes.len();
        buf[voff..voff + val_bytes.len()].copy_from_slice(val_bytes);

        data_offset += key_bytes.len() + val_bytes.len();
    }

    buf
}

#[test]
fn parent_locator_decodes_relative_path_value() {
    let buf = build_locator_buf(&[("relative_path", "Cargo.toml")]);
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    let kv = locator.entries().next().expect("relative_path entry");
    assert_eq!(kv.key(locator.key_value_data()).unwrap(), "relative_path");
    assert_eq!(kv.value(locator.key_value_data()).unwrap(), "Cargo.toml");
}

#[test]
fn parent_locator_resolves_parent_path_by_spec_order() {
    let buf = build_locator_buf(&[
        ("absolute_win32_path", r"\\?\C:\absolute-parent.vhdx"),
        (
            "volume_path",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\volume-parent.vhdx",
        ),
        ("relative_path", r"..\relative-parent.vhdx"),
    ]);
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    assert_eq!(
        locator.resolve_parent_path().unwrap(),
        std::path::PathBuf::from(r"..\relative-parent.vhdx")
    );
}

#[test]
fn parent_locator_resolves_parent_path_falls_back_to_volume_then_absolute() {
    let buf = build_locator_buf(&[
        ("absolute_win32_path", r"\\?\C:\absolute-parent.vhdx"),
        (
            "volume_path",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\volume-parent.vhdx",
        ),
    ]);
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    assert_eq!(
        locator.resolve_parent_path().unwrap(),
        std::path::PathBuf::from(
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\volume-parent.vhdx"
        )
    );
}

#[test]
fn parent_locator_resolve_parent_path_errors_when_no_path_key_exists() {
    let buf = build_locator_buf(&[("parent_linkage", "{00000000-0000-0000-0000-000000000000}")]);
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    assert!(matches!(
        locator.resolve_parent_path(),
        Err(crate::error::Error::ParentNotFound)
    ));
}

#[test]
fn parent_locator_preserves_all_path_candidates_without_fallback() {
    let buf = build_locator_buf(&[
        ("relative_path", "nonexistent_file_xyz.vhdx"),
        ("volume_path", "Cargo.toml"),
    ]);
    let meta = Metadata::new(&buf).unwrap();
    let locator = meta.items().parent_locator().unwrap();

    let entries: Vec<_> = locator
        .entries()
        .map(|kv| {
            (
                kv.key(locator.key_value_data()).unwrap(),
                kv.value(locator.key_value_data()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        vec![
            (
                "relative_path".to_string(),
                "nonexistent_file_xyz.vhdx".to_string()
            ),
            ("volume_path".to_string(), "Cargo.toml".to_string()),
        ]
    );
}
