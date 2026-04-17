//! VHDX 库集成测试 — 验证文件创建、打开、读写等核心操作的正确性

use std::path::PathBuf;

/// 生成一个临时 VHDX 文件路径，通过 `mem::forget` 阻止临时目录被自动清理，
/// 以便测试代码可以在该路径上创建 VHDX 文件。
fn temp_vhdx_path() -> PathBuf {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = dir.path().join("test.vhdx");
    std::mem::forget(dir);
    path
}

/// 测试固定磁盘的创建与读写：创建 1 MiB 固定磁盘，写入数据后读回并验证一致性。
#[test]
fn test_create_and_read_fixed_disk() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 创建 1 MiB 固定类型 VHDX 文件
    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 写入测试数据
    let test_data = b"Hello, VHDX!";
    let bytes_written = file.write(0, test_data).expect("Failed to write");
    assert_eq!(bytes_written, test_data.len());

    // 刷新确保数据落盘
    file.flush().expect("Failed to flush");

    // 从偏移 0 读回数据并验证与写入一致
    let mut buf = vec![0u8; test_data.len()];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, test_data.len());
    assert_eq!(&buf, test_data);
}

/// 测试动态磁盘的创建：验证动态磁盘类型标志和虚拟磁盘大小正确。
#[test]
fn test_create_dynamic_disk() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 创建 1 MiB 动态类型 VHDX 文件
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // 确认为动态磁盘，且虚拟大小为 1 MiB
    assert!(!file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

/// 测试读取动态磁盘未分配的数据块：未写入的数据应返回全零。
#[test]
fn test_read_unallocated_dynamic_block() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 创建动态磁盘（不写入任何数据）
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // 读取 512 字节，期望返回全零
    let mut buf = vec![0u8; 512];
    let bytes_read = file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(bytes_read, 512);
    assert_eq!(buf, vec![0u8; 512]);
}

/// 测试对动态磁盘执行写入操作应失败（当前库仅支持读取动态磁盘）。
#[test]
fn test_write_dynamic_disk_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // 写入操作应返回错误
    let result = file.write(0, b"test");
    assert!(result.is_err());
}

/// 测试以自定义块大小创建固定磁盘：验证块大小和虚拟磁盘大小均正确。
#[test]
fn test_create_fixed_disk_with_custom_block_size() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 使用 1 MiB 自定义块大小创建固定磁盘
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk with custom block size");

    // 验证块大小、类型和虚拟磁盘大小
    assert!(file.is_fixed());
    assert_eq!(file.block_size(), 1024 * 1024);
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

/// 测试以自定义块大小创建动态磁盘：验证块大小设置生效。
#[test]
fn test_create_dynamic_disk_with_custom_block_size() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 创建 4 MiB 动态磁盘，块大小为 1 MiB
    let file = File::create(&path)
        .size(4 * 1024 * 1024)
        .fixed(false)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create dynamic disk with custom block size");

    // 确认为动态类型且块大小为 1 MiB
    assert!(!file.is_fixed());
    assert_eq!(file.block_size(), 1024 * 1024);
}

/// 测试创建零大小磁盘应失败。
#[test]
fn test_create_zero_size_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let result = File::create(&path).size(0).fixed(true).finish();
    assert!(result.is_err(), "Zero-size creation should fail");
}

/// 测试使用非 2 的幂的块大小创建磁盘应失败。
#[test]
fn test_create_non_power_of_two_block_size_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 块大小 3 MiB 不是 2 的幂
    let result = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(3 * 1024 * 1024)
        .finish();
    assert!(result.is_err(), "Non-power-of-2 block size should fail");
}

/// 测试在已有文件上重复创建应失败。
#[test]
fn test_create_file_already_exists_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 首次创建应成功
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("First creation should succeed");

    // 再次创建同一路径应失败
    let result = File::create(&path).size(1024 * 1024).fixed(true).finish();
    assert!(result.is_err(), "Creating over existing file should fail");
}

/// 测试创建 10 MiB 固定磁盘：验证大容量磁盘的虚拟大小和类型正确。
#[test]
fn test_create_fixed_disk_10mb() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(10 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create 10 MB fixed disk");

    // 验证虚拟大小为 10 MiB 且为固定类型
    assert_eq!(file.virtual_disk_size(), 10 * 1024 * 1024);
    assert!(file.is_fixed());
}

/// 测试以只读模式打开固定磁盘：验证能正确读取磁盘元信息。
#[test]
fn test_open_fixed_disk_read_only() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 先创建一个固定磁盘
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 以只读方式打开并验证属性
    let file = File::open(&path)
        .finish()
        .expect("Failed to open existing file");
    assert!(file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

/// 测试以只读模式打开动态磁盘：验证类型和大小信息正确。
#[test]
fn test_open_dynamic_disk_read_only() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 先创建动态磁盘
    File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // 以只读方式打开并验证
    let file = File::open(&path)
        .finish()
        .expect("Failed to open existing file");
    assert!(!file.is_fixed());
    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

/// 测试打开不存在的文件应失败。
#[test]
fn test_open_nonexistent_file_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();
    let result = File::open(&path).finish();
    assert!(result.is_err(), "Opening non-existent file should fail");
}

/// 测试以写入模式打开已有文件并写入数据。
#[test]
fn test_open_with_write_access() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 先创建固定磁盘
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 以写入模式打开并写入数据
    let mut file = File::open(&path)
        .write()
        .finish()
        .expect("Failed to open with write access");

    let written = file.write(0, b"test data").expect("Failed to write");
    assert_eq!(written, 9);
}

/// 测试在非零偏移处写入和读取数据：验证偏移寻址的正确性。
#[test]
fn test_write_and_read_at_offset() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 在偏移 512 处写入数据
    let data = b"offset data";
    file.write(512, data).expect("Failed to write at offset");

    // 从同一偏移处读回并验证
    let mut buf = vec![0u8; data.len()];
    file.read(512, &mut buf).expect("Failed to read at offset");
    assert_eq!(&buf, data);
}

/// 测试读取未写入区域应返回全零：固定磁盘初始内容应为零。
#[test]
fn test_read_unwritten_area_returns_zeros() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 在偏移 0 写入少量数据
    file.write(0, b"some data").expect("Failed to write");

    // 读取偏移 4096 处（未写入）的 512 字节，应为全零
    let mut buf = vec![0u8; 512];
    file.read(4096, &mut buf).expect("Failed to read");
    assert_eq!(buf, vec![0u8; 512], "Unwritten area should be zeros");
}

/// 测试多次写入和读取：在不同偏移处写入数据后逐一读回验证。
#[test]
fn test_multiple_writes_and_reads() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 在三个不同偏移处分别写入数据
    file.write(0, b"block0").expect("Failed to write block0");
    file.write(1024, b"block1").expect("Failed to write block1");
    file.write(2048, b"block2").expect("Failed to write block2");

    let mut buf0 = vec![0u8; 6];
    let mut buf1 = vec![0u8; 6];
    let mut buf2 = vec![0u8; 6];

    // 从各偏移处读回数据
    file.read(0, &mut buf0).expect("Failed to read block0");
    file.read(1024, &mut buf1).expect("Failed to read block1");
    file.read(2048, &mut buf2).expect("Failed to read block2");

    // 验证各块数据与写入一致
    assert_eq!(&buf0, b"block0");
    assert_eq!(&buf1, b"block1");
    assert_eq!(&buf2, b"block2");
}

/// 测试写入后刷新并重新打开文件：验证数据持久化正确。
#[test]
fn test_flush_after_write() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let mut file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 写入数据并刷新
    file.write(0, b"flush test").expect("Failed to write");
    file.flush().expect("Failed to flush");

    // 重新打开文件并读取，验证数据持久化
    let file = File::open(&path).finish().expect("Failed to reopen");

    let mut buf = vec![0u8; 10];
    file.read(0, &mut buf).expect("Failed to read");
    assert_eq!(&buf, b"flush test");
}

/// 测试创建后头部区域（Header Section）的正确性：验证版本号等字段。
#[test]
fn test_header_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 读取头部区域
    let header = file.sections().header().expect("Failed to read header");

    // 验证 VHDX 版本号和日志版本号
    let hdr = header.header(0).expect("No header structure found");
    assert_eq!(hdr.version(), 1, "VHDX version should be 1");
    assert_eq!(hdr.log_version(), 0, "Log version should be 0");
}

/// 测试创建后 BAT（块分配表）区域的条目数量是否正确。
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

    // 读取 BAT 区域
    let bat = file.sections().bat().expect("Failed to read BAT");

    // 验证 BAT 条目数量与理论计算值一致
    let expected = vhdx_rs::Bat::calculate_total_entries(
        file.virtual_disk_size(),
        file.block_size(),
        file.logical_sector_size(),
    );
    assert_eq!(bat.len() as u64, expected, "BAT entry count mismatch");
}

/// 测试创建后元数据区域的正确性：验证虚拟磁盘大小和文件参数。
#[test]
fn test_metadata_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(10 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 读取元数据区域
    let metadata = file.sections().metadata().expect("Failed to read metadata");
    let items = metadata.items();

    // 验证虚拟磁盘大小
    assert_eq!(
        items.virtual_disk_size(),
        Some(10 * 1024 * 1024),
        "Virtual disk size should match"
    );

    // 验证文件参数中无父磁盘
    let fp = items.file_parameters().expect("Missing file parameters");
    assert!(!fp.has_parent(), "Should not have parent");
}

/// 测试元数据中的块大小与创建时指定的块大小一致。
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

    // 验证元数据记录的块大小与指定值一致
    assert_eq!(fp.block_size(), 1024 * 1024, "Block size should match");
    // 验证元数据块大小与 File API 返回的块大小一致
    assert_eq!(
        fp.block_size(),
        file.block_size(),
        "Metadata block size should match File::block_size()"
    );
}

/// 测试元数据中的扇区大小：逻辑扇区和物理扇区均应为 512 字节。
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

    // 验证逻辑扇区大小为 512
    assert_eq!(
        items.logical_sector_size(),
        Some(512),
        "Logical sector size should be 512"
    );
    // 验证物理扇区大小为 512
    assert_eq!(
        items.physical_sector_size(),
        Some(512),
        "Physical sector size should be 512"
    );
}

/// 测试创建后日志区域状态：新文件不应需要日志重放。
#[test]
fn test_log_section_after_create() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 新创建的文件不应需要日志重放
    let log = file.sections().log().expect("Failed to read log");
    assert!(
        !log.is_replay_required(),
        "New file should not require log replay"
    );
}

/// 测试新创建文件无待处理日志。
#[test]
fn test_has_pending_logs_false_for_new_file() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 新文件不应有待处理的日志
    assert!(
        !file.has_pending_logs(),
        "New file should not have pending logs"
    );
}

/// 测试默认块大小为 32 MiB。
#[test]
fn test_default_block_size_is_32mb() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 未指定块大小时默认应为 32 MiB
    assert_eq!(
        file.block_size(),
        32 * 1024 * 1024,
        "Default block size should be 32 MB"
    );
}

/// 测试逻辑扇区大小为 512 字节。
#[test]
fn test_logical_sector_size_is_512() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 逻辑扇区大小应为 512 字节
    assert_eq!(
        file.logical_sector_size(),
        512,
        "Logical sector size should be 512"
    );
}

/// 测试非差分磁盘不应有父磁盘。
#[test]
fn test_has_parent_false_for_non_differencing() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(false)
        .finish()
        .expect("Failed to create dynamic disk");

    // 非差分磁盘不应有父磁盘
    assert!(
        !file.has_parent(),
        "Non-differencing disk should not have parent"
    );
}

/// 测试元数据中虚拟磁盘 ID 存在且非空。
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
    // 虚拟磁盘 ID 应存在
    assert!(disk_id.is_some(), "Virtual disk ID should be present");
    // 虚拟磁盘 ID 不应为 nil UUID
    assert!(
        !disk_id.unwrap().is_nil(),
        "Virtual disk ID should not be nil"
    );
}

/// 测试打开 misc/test-void.vhdx 样本文件：验证能正确读取动态磁盘的各区域。
#[test]
fn test_open_test_void_vhdx() {
    use vhdx_rs::File;

    let path = std::path::Path::new("misc/test-void.vhdx");
    // 如果样本文件不存在则跳过
    if !path.exists() {
        eprintln!("Skipping: misc/test-void.vhdx not found");
        return;
    }

    let file = File::open(path)
        .finish()
        .expect("Failed to open test-void.vhdx");

    // 验证为动态类型且虚拟大小大于 0
    assert!(!file.is_fixed(), "test-void.vhdx should be dynamic");
    assert!(
        file.virtual_disk_size() > 0,
        "test-void.vhdx should have a virtual size"
    );

    // 验证头部和元数据区域可读
    let _header = file.sections().header().expect("Header should be readable");
    let _metadata = file
        .sections()
        .metadata()
        .expect("Metadata should be readable");
}

/// 测试打开 misc/test-fs.vhdx 样本文件：验证能正确读取含文件系统数据的磁盘。
#[test]
fn test_open_test_fs_vhdx() {
    use vhdx_rs::File;

    let path = std::path::Path::new("misc/test-fs.vhdx");
    // 如果样本文件不存在则跳过
    if !path.exists() {
        eprintln!("Skipping: misc/test-fs.vhdx not found");
        return;
    }

    let file = File::open(path)
        .finish()
        .expect("Failed to open test-fs.vhdx");

    // 验证虚拟大小大于 0
    assert!(
        file.virtual_disk_size() > 0,
        "test-fs.vhdx should have non-zero size"
    );

    // 验证头部、BAT 和元数据区域均可读
    let _header = file.sections().header().expect("Header should be readable");
    let _bat = file.sections().bat().expect("BAT should be readable");
    let _metadata = file
        .sections()
        .metadata()
        .expect("Metadata should be readable");
}
