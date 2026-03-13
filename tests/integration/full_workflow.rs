//! Integration tests for full VHDX workflow
//!
//! Tests complete VHDX lifecycle: create -> write -> read -> verify

use crate::common::{
    assert_data_equal, cleanup_vhdx, create_temp_dynamic_vhdx, create_temp_fixed_vhdx,
    generate_test_data,
};

use linkfs::DiskType;

/// Test complete workflow on dynamic VHDX:
/// 1. Create dynamic VHDX
/// 2. Write data at multiple offsets
/// 3. Read data back
/// 4. Verify data integrity
#[test]
fn test_dynamic_vhdx_full_workflow() {
    let block_size = 1024 * 1024; // 1MB
    let disk_size = 10 * block_size as u64; // 10MB

    let (mut vhdx, path) = create_temp_dynamic_vhdx("dynamic_workflow", disk_size);

    // Ensure cleanup on test completion
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    // Test parameters
    let test_offsets = vec![0u64, block_size as u64, 2 * block_size as u64];
    let data_size = 4096; // 4KB

    // Phase 1: Write data at multiple offsets
    for (i, &offset) in test_offsets.iter().enumerate() {
        let write_data = generate_test_data(i as u8, data_size);

        let bytes_written = vhdx
            .write(offset, &write_data)
            .unwrap_or_else(|e| panic!("Failed to write at offset {}: {:?}", offset, e));

        assert_eq!(
            bytes_written, data_size,
            "Write returned unexpected byte count at offset {}",
            offset
        );
    }

    // Phase 2: Read data back and verify
    for (i, &offset) in test_offsets.iter().enumerate() {
        let mut read_buffer = vec![0u8; data_size];
        let expected_data = generate_test_data(i as u8, data_size);

        let bytes_read = vhdx
            .read(offset, &mut read_buffer)
            .unwrap_or_else(|e| panic!("Failed to read at offset {}: {:?}", offset, e));

        assert_eq!(
            bytes_read, data_size,
            "Read returned unexpected byte count at offset {}",
            offset
        );

        assert_data_equal(&read_buffer, &expected_data, offset);
    }
}

/// Test complete workflow on fixed VHDX:
/// 1. Create fixed VHDX
/// 2. Write data at multiple offsets
/// 3. Read data back
/// 4. Verify data integrity
#[test]
fn test_fixed_vhdx_full_workflow() {
    let block_size = 1024 * 1024; // 1MB
    let disk_size = 10 * block_size as u64; // 10MB

    let (mut vhdx, path) = create_temp_fixed_vhdx("fixed_workflow", disk_size);

    // Ensure cleanup on test completion
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    // Test parameters
    let test_offsets = vec![0u64, block_size as u64, 5 * block_size as u64];
    let data_size = 4096; // 4KB

    // Phase 1: Write data at multiple offsets
    for (i, &offset) in test_offsets.iter().enumerate() {
        let write_data = generate_test_data((i + 10) as u8, data_size);

        let bytes_written = vhdx
            .write(offset, &write_data)
            .unwrap_or_else(|e| panic!("Failed to write at offset {}: {:?}", offset, e));

        assert_eq!(
            bytes_written, data_size,
            "Write returned unexpected byte count at offset {}",
            offset
        );
    }

    // Phase 2: Read data back and verify
    for (i, &offset) in test_offsets.iter().enumerate() {
        let mut read_buffer = vec![0u8; data_size];
        let expected_data = generate_test_data((i + 10) as u8, data_size);

        let bytes_read = vhdx
            .read(offset, &mut read_buffer)
            .unwrap_or_else(|e| panic!("Failed to read at offset {}: {:?}", offset, e));

        assert_eq!(
            bytes_read, data_size,
            "Read returned unexpected byte count at offset {}",
            offset
        );

        assert_data_equal(&read_buffer, &expected_data, offset);
    }
}

/// Test cross-block writes and reads
#[test]
fn test_cross_block_operations() {
    let block_size = 1024 * 1024; // 1MB
    let disk_size = 5 * block_size as u64; // 5MB

    let (mut vhdx, path) = create_temp_dynamic_vhdx("cross_block", disk_size);
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    // Write data that spans two blocks
    let cross_block_offset = (block_size - 256) as u64;
    let cross_block_size = 512; // 256 bytes in first block, 256 in second
    let write_data = generate_test_data(0xAA, cross_block_size);

    let bytes_written = vhdx
        .write(cross_block_offset, &write_data)
        .expect("Failed to write cross-block data");

    assert_eq!(bytes_written, cross_block_size);

    // Read back cross-block data
    let mut read_buffer = vec![0u8; cross_block_size];
    let bytes_read = vhdx
        .read(cross_block_offset, &mut read_buffer)
        .expect("Failed to read cross-block data");

    assert_eq!(bytes_read, cross_block_size);
    assert_data_equal(&read_buffer, &write_data, cross_block_offset);
}

/// Test multiple sequential writes to same location
#[test]
fn test_overwrite_operations() {
    let block_size = 1024 * 1024;
    let disk_size = 5 * block_size as u64;

    let (mut vhdx, path) = create_temp_dynamic_vhdx("overwrite", disk_size);
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    let offset = 0u64;
    let data_size = 1024;

    // First write
    let data1 = generate_test_data(0x11, data_size);
    vhdx.write(offset, &data1).expect("First write failed");

    // Second write (overwrite)
    let data2 = generate_test_data(0x22, data_size);
    vhdx.write(offset, &data2).expect("Second write failed");

    // Third write (overwrite again)
    let data3 = generate_test_data(0x33, data_size);
    vhdx.write(offset, &data3).expect("Third write failed");

    // Read back - should get last written data
    let mut read_buffer = vec![0u8; data_size];
    vhdx.read(offset, &mut read_buffer).expect("Read failed");

    assert_data_equal(&read_buffer, &data3, offset);
}

/// Test large data operations
#[test]
fn test_large_data_operations() {
    let block_size = 1024 * 1024;
    let disk_size = 20 * block_size as u64; // 20MB

    let (mut vhdx, path) = create_temp_dynamic_vhdx("large_data", disk_size);
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    let offset = 10 * block_size as u64; // Middle of disk
    let data_size = 1024 * 1024; // 1MB of data

    let write_data = generate_test_data(0xFF, data_size);

    // Write 1MB
    let bytes_written = vhdx.write(offset, &write_data).expect("Large write failed");

    assert_eq!(bytes_written, data_size);

    // Read back 1MB
    let mut read_buffer = vec![0u8; data_size];
    let bytes_read = vhdx
        .read(offset, &mut read_buffer)
        .expect("Large read failed");

    assert_eq!(bytes_read, data_size);

    // Verify in chunks to avoid huge panic messages
    const CHUNK_SIZE: usize = 4096;
    for i in (0..data_size).step_by(CHUNK_SIZE) {
        let end = (i + CHUNK_SIZE).min(data_size);
        assert_data_equal(&read_buffer[i..end], &write_data[i..end], offset + i as u64);
    }
}

/// Test VHDX metadata after operations
#[test]
fn test_metadata_consistency() {
    let block_size = 1024 * 1024 * 32; // 32MB default
    let disk_size = 10 * block_size as u64;

    let (vhdx, path) = create_temp_dynamic_vhdx("metadata_test", disk_size);
    let _guard = DropGuard::new(|| cleanup_vhdx(&path));

    // Verify metadata is consistent
    assert_eq!(vhdx.virtual_disk_size(), disk_size);
    assert_eq!(vhdx.block_size(), block_size);
    assert_eq!(vhdx.disk_type(), DiskType::Dynamic);
    assert_eq!(vhdx.logical_sector_size(), 512);
    assert_eq!(vhdx.physical_sector_size(), 4096);
}

/// Drop guard to ensure cleanup runs even if test panics
struct DropGuard<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> DropGuard<F> {
    fn new(f: F) -> Self {
        DropGuard(Some(f))
    }
}

impl<F: FnOnce()> Drop for DropGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}
