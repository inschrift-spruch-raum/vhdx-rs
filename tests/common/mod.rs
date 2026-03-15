//! Test utilities and helper functions for integration tests

use std::fs;
use std::path::{Path, PathBuf};
use vhdx_rs::{Builder, DiskType, File};

/// Get a unique temporary file path for tests
pub fn temp_vhdx_path(name: &str) -> PathBuf {
    let test_dir = PathBuf::from("test_output");
    fs::create_dir_all(&test_dir).expect("Failed to create test output directory");
    test_dir.join(format!("{}_{}.vhdx", name, std::process::id()))
}

/// Clean up a temporary VHDX file
pub fn cleanup_vhdx(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Create a temporary dynamic VHDX file for testing
pub fn create_temp_dynamic_vhdx(name: &str, size: u64) -> (File, PathBuf) {
    let path = temp_vhdx_path(name);
    let _vhdx = Builder::new(size)
        .disk_type(DiskType::Dynamic)
        .create(&path)
        .expect("Failed to create dynamic VHDX");
    // Reopen with write access for tests that need to write
    let vhdx = File::open(&path, true).expect("Failed to reopen dynamic VHDX with write access");
    (vhdx, path)
}

/// Create a temporary fixed VHDX file for testing
pub fn create_temp_fixed_vhdx(name: &str, size: u64) -> (File, PathBuf) {
    let path = temp_vhdx_path(name);
    let _vhdx = Builder::new(size)
        .disk_type(DiskType::Fixed)
        .create(&path)
        .expect("Failed to create fixed VHDX");
    // Reopen with write access for tests that need to write
    let vhdx = File::open(&path, true).expect("Failed to reopen fixed VHDX with write access");
    (vhdx, path)
}

/// Generate test data pattern
pub fn generate_test_data(seed: u8, size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((seed as usize + i) % 256) as u8)
        .collect()
}

/// Verify that two byte slices are equal, panic with detailed message if not
pub fn assert_data_equal(actual: &[u8], expected: &[u8], offset: u64) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "Data length mismatch at offset {}: expected {}, got {}",
        offset,
        expected.len(),
        actual.len()
    );

    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e {
            panic!(
                "Data mismatch at offset {} (byte {}): expected 0x{:02x}, got 0x{:02x}",
                offset + i as u64,
                i,
                e,
                a
            );
        }
    }
}
