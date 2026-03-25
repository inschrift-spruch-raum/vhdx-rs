//! Integration tests for vhdx-rs

use tempfile::NamedTempFile;

#[test]
fn test_create_and_read_fixed_disk() {
    use vhdx_rs::File;

    // Create a temporary file
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // Create a fixed disk
    let mut file = File::create(path)
        .size(1024 * 1024) // 1 MB
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // Write data
    let test_data = b"Hello, VHDX!";
    let bytes_written = file.write(0, test_data).expect("Failed to write");
    assert_eq!(bytes_written, test_data.len());

    // Flush
    file.flush().expect("Failed to flush");

    // Read back
    let mut buf = vec![0u8; test_data.len()];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, test_data.len());
    assert_eq!(&buf, test_data);
}

#[test]
fn test_create_dynamic_disk() {
    use vhdx_rs::File;

    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // Create a dynamic disk
    let file = File::create(path)
        .size(1024 * 1024) // 1 MB
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    assert!(!file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

#[test]
fn test_read_unallocated_dynamic_block() {
    use vhdx_rs::File;

    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // Create a dynamic disk
    let file = File::create(path)
        .size(1024 * 1024) // 1 MB
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // Read from unallocated area should return zeros
    let mut buf = vec![0u8; 512];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, 512);
    assert_eq!(buf, vec![0u8; 512]); // Should be all zeros
}

#[test]
fn test_write_dynamic_disk_fails() {
    use vhdx_rs::File;

    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();

    // Create a dynamic disk
    let mut file = File::create(path)
        .size(1024 * 1024) // 1 MB
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // Write should fail with a clear error
    let result = file.write(0, b"test");
    assert!(result.is_err());
}
