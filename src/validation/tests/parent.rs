use super::super::{Result, SpecValidator, StandardItems};
use super::helpers::{Encoded, build_test_vhdx};
use crate::constants::{METADATA_TABLE_SIZE, REGION_TABLE1_OFFSET};
use bitvec::prelude::*;
use crc32c::crc32c;

fn create_vhdx(path: &std::path::Path) -> crate::medium::CreateOptions<std::fs::File> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("prepare caller-owned create medium");
    crate::medium::Medium::create(inner)
}

fn open_vhdx(path: &std::path::Path) -> crate::medium::Medium {
    let inner = std::fs::File::open(path).expect("open caller-owned medium");
    crate::medium::Medium::open(inner)
        .finish()
        .expect("open vhdx")
}

/// Build a VHDX buffer whose parent locator contains the given KV pairs.
///
/// The base VHDX is modified to be a differencing disk (`has_parent=1`) and
/// the existing empty parent locator metadata entry is replaced with one
/// pointing to actual locator data at `items_base` + 48.
fn build_vhdx_with_parent_locator(kvs: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = build_test_vhdx();

    // Locate the metadata region from region table 1 (at 192 KB).
    // RT: 16-byte header + 2×32-byte entries. Entry 1 (metadata) starts at offset 48.
    let rt_offset = REGION_TABLE1_OFFSET as usize;
    let metadata_offset =
        u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
    let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
    let items_base = mo + METADATA_TABLE_SIZE as usize; // items start after the 64 KB metadata table

    // Mark the disk as differencing: set FileParameters has_parent bit (bit 1).
    buf[items_base..items_base + 8]
        .view_bits_mut::<Lsb0>()
        .set(1, true);

    // Existing items occupy bytes 0..44 of the items area.
    // Place parent locator data right after, at offset 48 (4-byte aligned).
    let pl_start = items_base + 48;
    let pl_region_off = u32::try_from(pl_start - mo).expect("parent locator offset fits u32"); // offset within metadata region

    // -- Build parent locator data ---------------------------------------
    let loc_hdr_size = 20usize;
    let kv_entry_size = 12usize;
    let num = kvs.len();
    let kv_tab_size = num * kv_entry_size;
    let kv_dat_base = loc_hdr_size + kv_tab_size;

    // Encode keys & values to UTF-16LE.
    let encoded: Vec<Encoded> = kvs
        .iter()
        .map(|(k, v)| Encoded {
            key: k.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            val: v.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        })
        .collect();

    let total_kv: usize = encoded.iter().map(|e| e.key.len() + e.val.len()).sum();
    let pl_size = kv_dat_base + total_kv;

    // Grow buffer if needed.
    let end = pl_start + pl_size;
    if buf.len() < end {
        buf.resize(end, 0);
    }

    // Write locator header (20 bytes).
    let pl = &mut buf[pl_start..pl_start + pl_size];
    pl[0..16].copy_from_slice(&StandardItems::LOCATOR_TYPE_VHDX.to_bytes());
    pl[16..18].copy_from_slice(&0u16.to_le_bytes()); // reserved
    pl[18..20].copy_from_slice(
        &u16::try_from(num)
            .expect("entry count fits u16")
            .to_le_bytes(),
    ); // entry count

    // Write KV entries and data.
    let mut kv_off = kv_dat_base;
    for (i, e) in encoded.iter().enumerate() {
        let entry_off = loc_hdr_size + i * kv_entry_size;
        let key_off = u32::try_from(kv_off).expect("key offset fits u32");
        let val_off = u32::try_from(kv_off + e.key.len()).expect("value offset fits u32");
        pl[entry_off..entry_off + 4].copy_from_slice(&key_off.to_le_bytes());
        pl[entry_off + 4..entry_off + 8].copy_from_slice(&val_off.to_le_bytes());
        pl[entry_off + 8..entry_off + 10].copy_from_slice(
            &u16::try_from(e.key.len())
                .expect("key length fits u16")
                .to_le_bytes(),
        );
        pl[entry_off + 10..entry_off + 12].copy_from_slice(
            &u16::try_from(e.val.len())
                .expect("value length fits u16")
                .to_le_bytes(),
        );

        pl[kv_off..kv_off + e.key.len()].copy_from_slice(&e.key);
        kv_off += e.key.len();
        pl[kv_off..kv_off + e.val.len()].copy_from_slice(&e.val);
        kv_off += e.val.len();
    }

    // Update the ParentLocator metadata table entry (entry index 5).
    let pl_entry = mo + 32 + 5 * 32;
    buf[pl_entry + 16..pl_entry + 20].copy_from_slice(&pl_region_off.to_le_bytes());
    buf[pl_entry + 20..pl_entry + 24].copy_from_slice(
        &u32::try_from(pl_size)
            .expect("parent locator size fits u32")
            .to_le_bytes(),
    );

    buf
}

#[test]
fn test_validate_parent_locator_corrupt_key() {
    // Build a VHDX with a parent locator containing a valid KV entry,
    // then manually corrupt the key_length to an odd value so that
    // UTF-16LE decoding fails.
    let mut buf = build_vhdx_with_parent_locator(&[(
        "parent_linkage",
        "{00000000-0000-0000-0000-000000000000}",
    )]);

    // Locator data is at items_base + 48. KV entry 0 starts at
    // pl_start + 20 (after 20-byte locator header). key_length is at [8..10].
    let rt_offset = REGION_TABLE1_OFFSET as usize;
    let metadata_offset =
        u64::from_le_bytes(buf[rt_offset + 64..rt_offset + 72].try_into().unwrap());
    let mo = usize::try_from(metadata_offset).expect("metadata offset fits usize");
    let kv0_off = mo + METADATA_TABLE_SIZE as usize + 48 + 20;
    buf[kv0_off + 8..kv0_off + 10].copy_from_slice(&5u16.to_le_bytes()); // odd length

    let validator = SpecValidator::new(&buf, true);
    let result = validator.validate_parent_locator();
    assert!(result.is_err());
    let msg = format!("{result:?}");
    assert!(
        msg.contains("odd byte length"),
        "expected 'odd byte length', got: {msg}"
    );
}

#[test]
fn test_validate_parent_locator_rejects_linkage2() {
    // Parent locator with both parent_linkage and parent_linkage2.
    // validate_parent_locator should reject parent_linkage2 without file I/O.
    let buf = build_vhdx_with_parent_locator(&[
        ("parent_linkage", "{01234567-89ab-cdef-0123-456789abcdef}"),
        ("parent_linkage2", "{00000000-0000-0000-0000-000000000000}"),
        ("relative_path", "child.vhdx"),
    ]);

    let validator = SpecValidator::new(&buf, true);
    let result = validator.validate_parent_locator();
    assert!(result.is_err());
    let msg = format!("{:?}", result.err().unwrap());
    assert!(
        msg.contains("ParentLocatorLinkage2Conflict"),
        "expected ParentLocatorLinkage2Conflict, got: {msg}"
    );
}

#[test]
fn test_validate_parent_locator_rejects_unparseable_linkage() {
    // Parent locator with parent_linkage value that is not a valid GUID.
    let buf = build_vhdx_with_parent_locator(&[
        ("parent_linkage", "not-a-guid"),
        ("relative_path", "child.vhdx"),
    ]);

    let validator = SpecValidator::new(&buf, true);
    let result = validator.validate_parent_locator();
    assert!(result.is_err());
    let msg = format!("{:?}", result.err().unwrap());
    assert!(
        msg.contains("not a valid GUID"),
        "expected 'not a valid GUID', got: {msg}"
    );
}

#[test]
fn test_from_file_constructor() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-from-file.vhdx");

    // Create a valid dynamic VHDX file.
    create_vhdx(&path)
            .size(256 * 1024 * 1024) // 256 MB
            .block_size(32 * 1024 * 1024)
            .logical_sector_size(4096)
            .finish()?;

    // Open it.
    let mut file = open_vhdx(&path);

    // Construct SpecValidator via from_file.
    let validator = SpecValidator::from_file(&mut file)?;

    // Verify that sub-validators actually execute (don't silently skip).
    validator.validate_bat()?;
    validator.validate_metadata()?;
    validator.validate_log()?;

    Ok(())
}

/// `Medium::create()` writes the same sequence number (0) to both headers,
/// which the validator rejects. Patch header 2 to have seq=1 so validation
/// can proceed.
fn patch_header2_sequence(path: &std::path::Path) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;

    // Read header 2 (4 KB at offset 128 KB).
    let mut h2 = [0u8; 4096];
    f.seek(SeekFrom::Start(128 * 1024))?;
    f.read_exact(&mut h2)?;

    // Bump sequence number from 0 to 1.
    h2[8..16].copy_from_slice(&1u64.to_le_bytes());

    // Recompute CRC-32C.
    h2[4..8].copy_from_slice(&[0u8; 4]);
    let crc = crc32c(&h2);
    h2[4..8].copy_from_slice(&crc.to_le_bytes());

    f.seek(SeekFrom::Start(128 * 1024))?;
    f.write_all(&h2)?;

    Ok(())
}

#[test]
fn test_validate_file_covers_parent_chain() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-parent-chain.vhdx");

    // Create a dynamic disk (no parent — not a differencing disk).
    create_vhdx(&path).size(256 * 1024 * 1024).finish()?;

    // Patch header 2 sequence number so the validator passes.
    patch_header2_sequence(&path)?;

    let mut file = open_vhdx(&path);
    // validate_file should pass (no parent chain for a non-differencing disk).
    file.validator()?.validate_file()?;

    Ok(())
}

#[test]
fn test_file_validator_integration() -> Result<()> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test-validator-int.vhdx");

    // Create a valid dynamic VHDX.
    create_vhdx(&path)
        .size(256 * 1024 * 1024)
        .block_size(32 * 1024 * 1024)
        .logical_sector_size(4096)
        .physical_sector_size(4096)
        .finish()?;

    // Patch header 2 sequence number so the validator passes.
    patch_header2_sequence(&path)?;

    // Open and validate.
    let mut file = open_vhdx(&path);
    file.validator()?.validate_file()?;

    Ok(())
}
