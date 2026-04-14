use std::path::PathBuf;

fn temp_vhdx_path() -> PathBuf {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = dir.path().join("test.vhdx");
    std::mem::forget(dir);
    path
}

#[test]
fn test_create_and_read_fixed_disk() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let test_data = b"Hello, VHDX!";
    let bytes_written = file.write(0, test_data).expect("Failed to write");
    assert_eq!(bytes_written, test_data.len());

    file.flush().expect("Failed to flush");

    let mut buf = vec![0u8; test_data.len()];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, test_data.len());
    assert_eq!(&buf, test_data);
}

#[test]
fn test_create_dynamic_disk() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    assert!(!file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

#[test]
fn test_read_unallocated_dynamic_block() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    let mut buf = vec![0u8; 512];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, 512);
    assert_eq!(buf, vec![0u8; 512]);
}

#[test]
fn test_write_dynamic_disk_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    let result = file.write(0, b"test");
    assert!(result.is_err());
}

#[test]
fn test_create_fixed_disk_with_custom_block_size() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk with custom block size");

    assert!(file.is_fixed());
    assert_eq!(file.block_size(), 1024 * 1024);
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

#[test]
fn test_create_dynamic_disk_with_custom_block_size() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(4 * 1024 * 1024)
        .fixed(false)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create dynamic disk with custom block size");

    assert!(!file.is_fixed());
    assert_eq!(file.block_size(), 1024 * 1024);
}

#[test]
fn test_create_zero_size_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let result = File::create(&path).size(0).fixed(true).finish();
    assert!(result.is_err(), "Zero-size creation should fail");
}

#[test]
fn test_create_non_power_of_two_block_size_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let result = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(3 * 1024 * 1024)
        .finish();
    assert!(result.is_err(), "Non-power-of-2 block size should fail");
}

#[test]
fn test_create_file_already_exists_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("First creation should succeed");

    let result = File::create(&path).size(1024 * 1024).fixed(true).finish();
    assert!(result.is_err(), "Creating over existing file should fail");
}

#[test]
fn test_create_fixed_disk_10mb() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(10 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create 10 MB fixed disk");

    assert_eq!(file.virtual_disk_size(), 10 * 1024 * 1024);
    assert!(file.is_fixed());
}

#[test]
fn test_open_fixed_disk_read_only() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let file = File::open(&path)
        .finish()
        .expect("Failed to open existing file");
    assert!(file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

#[test]
fn test_open_dynamic_disk_read_only() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    let file = File::open(&path)
        .finish()
        .expect("Failed to open existing file");
    assert!(!file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

#[test]
fn test_open_nonexistent_file_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();
    let result = File::open(&path).finish();
    assert!(result.is_err(), "Opening non-existent file should fail");
}

#[test]
fn test_open_with_write_access() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let mut file = File::open(&path)
        .write()
        .finish()
        .expect("Failed to open with write access");

    let written = file.write(0, b"test data").expect("Failed to write");
    assert_eq!(written, 9);
}

#[test]
fn test_write_and_read_at_offset() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let data = b"offset data";
    file.write(512, data).expect("Failed to write at offset");

    let mut buf = vec![0u8; data.len()];
    file.read(512, &mut buf).expect("Failed to read at offset");
    assert_eq!(&buf, data);
}

#[test]
fn test_read_unwritten_area_returns_zeros() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    file.write(0, b"some data").expect("Failed to write");

    let mut buf = vec![0u8; 512];
    file.read(4096, &mut buf).expect("Failed to read");
    assert_eq!(buf, vec![0u8; 512], "Unwritten area should be zeros");
}

#[test]
fn test_multiple_writes_and_reads() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    file.write(0, b"block0").expect("Failed to write block0");
    file.write(1024, b"block1").expect("Failed to write block1");
    file.write(2048, b"block2").expect("Failed to write block2");

    let mut buf0 = vec![0u8; 6];
    let mut buf1 = vec![0u8; 6];
    let mut buf2 = vec![0u8; 6];

    file.read(0, &mut buf0).expect("Failed to read block0");
    file.read(1024, &mut buf1).expect("Failed to read block1");
    file.read(2048, &mut buf2).expect("Failed to read block2");

    assert_eq!(&buf0, b"block0");
    assert_eq!(&buf1, b"block1");
    assert_eq!(&buf2, b"block2");
}

#[test]
fn test_flush_after_write() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    file.write(0, b"flush test").expect("Failed to write");
    file.flush().expect("Failed to flush");

    let file = File::open(&path).finish().expect("Failed to reopen");

    let mut buf = vec![0u8; 10];
    file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(&buf, b"flush test");
}

#[test]
fn test_header_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let header = file.sections().header().expect("Failed to read header");

    let hdr = header.header(0).expect("No header structure found");
    assert_eq!(hdr.version(), 1, "VHDX version should be 1");
    assert_eq!(hdr.log_version(), 0, "Log version should be 0");
}

#[test]
fn test_bat_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    let bat = file.sections().bat().expect("Failed to read BAT");

    let expected = vhdx_rs::Bat::calculate_total_entries(
        file.virtual_disk_size(),
        file.block_size(),
        file.logical_sector_size(),
    );
    assert_eq!(bat.len() as u64, expected, "BAT entry count mismatch");
}

#[test]
fn test_metadata_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(10 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let metadata = file.sections().metadata().expect("Failed to read metadata");
    let items = metadata.items();

    assert_eq!(
        items.virtual_disk_size(),
        Some(10 * 1024 * 1024),
        "Virtual disk size should match"
    );

    let fp = items.file_parameters().expect("Missing file parameters");
    assert!(!fp.has_parent(), "Should not have parent");
}

#[test]
fn test_metadata_block_size_matches() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    let metadata = file.sections().metadata().expect("Failed to read metadata");
    let fp = metadata
        .items()
        .file_parameters()
        .expect("Missing file parameters");

    assert_eq!(fp.block_size(), 1024 * 1024, "Block size should match");
    assert_eq!(
        fp.block_size(),
        file.block_size(),
        "Metadata block size should match File::block_size()"
    );
}

#[test]
fn test_metadata_sector_sizes() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let metadata = file.sections().metadata().expect("Failed to read metadata");
    let items = metadata.items();

    assert_eq!(
        items.logical_sector_size(),
        Some(512),
        "Logical sector size should be 512"
    );
    assert_eq!(
        items.physical_sector_size(),
        Some(512),
        "Physical sector size should be 512"
    );
}

#[test]
fn test_log_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let log = file.sections().log().expect("Failed to read log");
    assert!(
        !log.is_replay_required(),
        "New file should not require log replay"
    );
}

#[test]
fn test_has_pending_logs_false_for_new_file() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    assert!(
        !file.has_pending_logs(),
        "New file should not have pending logs"
    );
}

#[test]
fn test_default_block_size_is_32mb() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    assert_eq!(
        file.block_size(),
        32 * 1024 * 1024,
        "Default block size should be 32 MB"
    );
}

#[test]
fn test_logical_sector_size_is_512() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    assert_eq!(
        file.logical_sector_size(),
        512,
        "Logical sector size should be 512"
    );
}

#[test]
fn test_has_parent_false_for_non_differencing() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    assert!(
        !file.has_parent(),
        "Non-differencing disk should not have parent"
    );
}

#[test]
fn test_metadata_virtual_disk_id_present() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let metadata = file.sections().metadata().expect("Failed to read metadata");
    let disk_id = metadata.items().virtual_disk_id();
    assert!(disk_id.is_some(), "Virtual disk ID should be present");
    assert!(
        !disk_id.unwrap().is_nil(),
        "Virtual disk ID should not be nil"
    );
}

#[test]
fn test_open_test_void_vhdx() {
    use vhdx_rs::File;

    let path = std::path::Path::new("misc/test-void.vhdx");
    if !path.exists() {
        eprintln!("Skipping: misc/test-void.vhdx not found");
        return;
    }

    let file = File::open(path)
        .finish()
        .expect("Failed to open test-void.vhdx");

    assert!(!file.is_fixed(), "test-void.vhdx should be dynamic");
    assert!(
        file.virtual_disk_size() > 0,
        "test-void.vhdx should have a virtual size"
    );

    let _header = file.sections().header().expect("Header should be readable");
    let _metadata = file
        .sections()
        .metadata()
        .expect("Metadata should be readable");
}

#[test]
fn test_open_test_fs_vhdx() {
    use vhdx_rs::File;

    let path = std::path::Path::new("misc/test-fs.vhdx");
    if !path.exists() {
        eprintln!("Skipping: misc/test-fs.vhdx not found");
        return;
    }

    let file = File::open(path)
        .finish()
        .expect("Failed to open test-fs.vhdx");

    assert!(
        file.virtual_disk_size() > 0,
        "test-fs.vhdx should have non-zero size"
    );

    let _header = file.sections().header().expect("Header should be readable");
    let _bat = file.sections().bat().expect("BAT should be readable");
    let _metadata = file
        .sections()
        .metadata()
        .expect("Metadata should be readable");
}
