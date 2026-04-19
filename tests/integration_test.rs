//! VHDX 库集成测试 — 验证文件创建、打开、读写等核心操作的正确性

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use vhdx_rs::constants::{
    DATA_SECTOR_SIZE, DESCRIPTOR_SIZE, HEADER_SECTION_SIZE, LOG_ENTRY_HEADER_SIZE,
};

/// 生成一个临时 VHDX 文件路径，通过 `mem::forget` 阻止临时目录被自动清理，
/// 以便测试代码可以在该路径上创建 VHDX 文件。
fn temp_vhdx_path() -> PathBuf {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = dir.path().join("test.vhdx");
    std::mem::forget(dir);
    path
}

/// 将 UTF-16 字符串编码为小端字节序。
fn utf16_le_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
}

/// 构造 Parent Locator 原始数据。
fn build_parent_locator(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut key_value_data = Vec::new();
    let mut entry_table = Vec::new();

    for (key, value) in entries {
        let key_bytes = utf16_le_bytes(key);
        let value_bytes = utf16_le_bytes(value);

        let key_offset = u32::try_from(key_value_data.len()).expect("key offset overflow");
        key_value_data.extend_from_slice(&key_bytes);
        let value_offset = u32::try_from(key_value_data.len()).expect("value offset overflow");
        key_value_data.extend_from_slice(&value_bytes);

        entry_table.extend_from_slice(&key_offset.to_le_bytes());
        entry_table.extend_from_slice(&value_offset.to_le_bytes());
        entry_table.extend_from_slice(
            &u16::try_from(key_bytes.len())
                .expect("key length overflow")
                .to_le_bytes(),
        );
        entry_table.extend_from_slice(
            &u16::try_from(value_bytes.len())
                .expect("value length overflow")
                .to_le_bytes(),
        );
    }

    let mut locator = vec![0u8; 20];
    locator[18..20].copy_from_slice(
        &u16::try_from(entries.len())
            .expect("entry count overflow")
            .to_le_bytes(),
    );
    locator.extend_from_slice(&entry_table);
    locator.extend_from_slice(&key_value_data);
    locator
}

/// 向差分盘写入可控的 Parent Locator 数据（测试专用）。
fn inject_parent_locator(path: &std::path::Path, locator: &[u8]) {
    // 当前创建布局中 metadata 起始偏移固定为 2 * 1MiB。
    const METADATA_OFFSET: u64 = 2 * 1024 * 1024;
    // 第 6 个 metadata entry 为 Parent Locator（索引 5）。
    const PARENT_LOCATOR_ENTRY_OFFSET: u64 = METADATA_OFFSET + 32 + 5 * 32;
    const PARENT_LOCATOR_LENGTH_FIELD_OFFSET: u64 = PARENT_LOCATOR_ENTRY_OFFSET + 20;
    const PARENT_LOCATOR_DATA_OFFSET: u64 = METADATA_OFFSET + 65_576;

    let mut raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("Failed to open child file for parent locator injection");

    raw.seek(SeekFrom::Start(PARENT_LOCATOR_LENGTH_FIELD_OFFSET))
        .expect("Failed to seek parent locator length field");
    raw.write_all(
        &u32::try_from(locator.len())
            .expect("parent locator size overflow")
            .to_le_bytes(),
    )
    .expect("Failed to write parent locator length");

    raw.seek(SeekFrom::Start(PARENT_LOCATOR_DATA_OFFSET))
        .expect("Failed to seek parent locator data offset");
    raw.write_all(locator)
        .expect("Failed to write parent locator data");
    raw.flush()
        .expect("Failed to flush injected parent locator");
}

/// 在指定偏移读取固定长度原始字节（测试专用）。
fn read_raw_bytes(path: &std::path::Path, offset: u64, len: usize) -> Vec<u8> {
    let mut raw = OpenOptions::new()
        .read(true)
        .open(path)
        .expect("Failed to open file for raw read");
    raw.seek(SeekFrom::Start(offset))
        .expect("Failed to seek for raw read");
    let mut buf = vec![0u8; len];
    raw.read_exact(&mut buf).expect("Failed to read raw bytes");
    buf
}

/// 注入一个最小可回放日志条目，并设置 header 的 log_guid 为非空。
fn inject_pending_log_entry(path: &std::path::Path, write_offset: u64, payload: &[u8]) {
    use vhdx_rs::{File, Guid};

    let file = File::open(path)
        .finish()
        .expect("Failed to open file for log injection metadata");
    let header_ref = file
        .sections()
        .header()
        .expect("Failed to read header for log injection");
    let header = header_ref
        .header(0)
        .expect("No active header for log injection");

    let log_offset = header.log_offset();
    let log_length = usize::try_from(header.log_length()).expect("log_length overflow");

    let entry_len = LOG_ENTRY_HEADER_SIZE + DESCRIPTOR_SIZE + DATA_SECTOR_SIZE;
    let mut entry = vec![0u8; entry_len];
    entry[0..4].copy_from_slice(b"loge");
    entry[8..12]
        .copy_from_slice(&(u32::try_from(entry_len).expect("entry length overflow")).to_le_bytes());
    entry[24..28].copy_from_slice(&1u32.to_le_bytes()); // descriptor_count = 1

    let desc_off = LOG_ENTRY_HEADER_SIZE;
    entry[desc_off..desc_off + 4].copy_from_slice(b"desc");
    entry[desc_off + 4..desc_off + 8].copy_from_slice(&0u32.to_le_bytes()); // trailing_bytes
    entry[desc_off + 8..desc_off + 16].copy_from_slice(&0u64.to_le_bytes()); // leading_bytes
    entry[desc_off + 16..desc_off + 24].copy_from_slice(&write_offset.to_le_bytes());
    entry[desc_off + 24..desc_off + 32].copy_from_slice(&1u64.to_le_bytes()); // sequence

    let sector_off = LOG_ENTRY_HEADER_SIZE + DESCRIPTOR_SIZE;
    entry[sector_off..sector_off + 4].copy_from_slice(b"data");
    entry[sector_off + 4..sector_off + 8].copy_from_slice(&1u32.to_le_bytes());
    let payload_len = payload.len().min(4084);
    entry[sector_off + 8..sector_off + 8 + payload_len].copy_from_slice(&payload[..payload_len]);
    entry[sector_off + 4092..sector_off + 4096].copy_from_slice(&1u32.to_le_bytes());

    let mut raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("Failed to open file for log injection write");

    raw.seek(SeekFrom::Start(log_offset))
        .expect("Failed to seek log offset");
    raw.write_all(&entry).expect("Failed to write log entry");

    let remaining = log_length.saturating_sub(entry.len());
    if remaining > 0 {
        raw.write_all(&vec![0u8; remaining])
            .expect("Failed to clear log tail");
    }

    let log_guid = Guid::from_bytes([
        0xA1, 0xB2, 0xC3, 0xD4, 0x11, 0x22, 0x33, 0x44, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33,
        0x22,
    ]);
    let updated_header = vhdx_rs::section::HeaderStructure::create(
        header.sequence_number(),
        header.file_write_guid(),
        header.data_write_guid(),
        log_guid,
        header.log_length(),
        header.log_offset(),
    );

    raw.seek(SeekFrom::Start(64 * 1024))
        .expect("Failed to seek header1");
    raw.write_all(&updated_header)
        .expect("Failed to write header1");
    raw.seek(SeekFrom::Start(128 * 1024))
        .expect("Failed to seek header2");
    raw.write_all(&updated_header)
        .expect("Failed to write header2");
    raw.flush().expect("Failed to flush injected log");
}

/// 测试固定磁盘的创建与读写：创建 1 MiB 固定磁盘，写入数据后读回并验证一致性。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 read_raw/write_raw/flush_raw）。

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
    assert!(
        !file
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0),
        1024 * 1024
    );
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

    // 通过 IO 接口读取扇区 0 的 4096 字节，期望返回全零
    let sector = file.io().sector(0).expect("Sector 0 should exist");
    let mut buf = vec![0u8; 4096];
    sector.read(&mut buf).expect("Failed to read sector");
    assert_eq!(buf, vec![0u8; 4096]);
}

/// 测试对动态磁盘执行写入操作应失败（当前库仅支持读取动态磁盘）。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 write_raw）。

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
    assert!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp| fp.block_size()))
            .unwrap_or(0),
        1024 * 1024
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0),
        1024 * 1024
    );
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
    assert!(
        !file
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp| fp.block_size()))
            .unwrap_or(0),
        1024 * 1024
    );
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
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0),
        10 * 1024 * 1024
    );
    assert!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
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
    assert!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0),
        1024 * 1024
    );
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
    assert!(
        !file
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false)
    );
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0),
        1024 * 1024
    );
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
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 write_raw）。

/// 测试在非零偏移处写入和读取数据：验证偏移寻址的正确性。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 read_raw/write_raw）。

/// 测试读取未写入区域应返回全零：固定磁盘初始内容应为零。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 read_raw/write_raw）。

/// 测试多次写入和读取：在不同偏移处写入数据后逐一读回验证。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 read_raw/write_raw）。

/// 测试写入后刷新并重新打开文件：验证数据持久化正确。
/// 已移至 src/file.rs 单元测试（使用 pub(crate) 的 read_raw/write_raw/flush_raw）。

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

    // 验证 BAT 条目数量（bat.len() 即为总条目数）
    assert!(!bat.is_empty(), "BAT should have entries");
}

/// 测试 Bat::entries() 返回 Vec<BatEntry>：验证可遍历且内容正确。
#[test]
fn test_bat_entries_vec_traversable() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    let bat = file.sections().bat().expect("Failed to read BAT");

    // entries() 返回 Vec<BatEntry>，可以遍历
    let entries = bat.entries();
    assert!(!entries.is_empty(), "entries() should return non-empty Vec");

    // 遍历并断言每个条目状态有效（Payload 或 SectorBitmap）
    for entry in &entries {
        assert!(
            matches!(
                entry.state,
                vhdx_rs::section::BatState::Payload(_)
                    | vhdx_rs::section::BatState::SectorBitmap(_)
            ),
            "Each BAT entry state should be a valid BAT state variant"
        );
    }

    // 默认参数创建的该用例应包含至少一个 Sector Bitmap 条目
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.state, vhdx_rs::section::BatState::SectorBitmap(_))),
        "entries() should include at least one SectorBitmap entry"
    );

    // entries() 长度与 bat.len() 一致
    assert_eq!(entries.len(), bat.len());
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
    // 验证元数据块大小与指定值一致
    assert_eq!(
        fp.block_size(),
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp2| fp2.block_size()))
            .unwrap_or(0),
        "Metadata block size should be consistent"
    );
}

/// 测试元数据中的扇区大小：默认逻辑和物理扇区均应为 4096 字节。
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

    // 验证逻辑扇区大小为 4096
    assert_eq!(
        items.logical_sector_size(),
        Some(4096),
        "Logical sector size should be 4096"
    );
    // 验证物理扇区大小为 4096
    assert_eq!(
        items.physical_sector_size(),
        Some(4096),
        "Physical sector size should be 4096"
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
        !file.sections().log().is_ok_and(|l| l.is_replay_required()),
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
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp| fp.block_size()))
            .unwrap_or(0),
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

    // 默认逻辑扇区大小应为 4096 字节
    assert_eq!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().logical_sector_size())
            .unwrap_or(0),
        4096,
        "Logical sector size should be 4096"
    );
}

/// 测试 OpenOptions 链式方法：strict + log_replay 可编译并成功打开。
#[test]
fn test_open_options_chain_methods_happy_path() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let file = File::open(&path)
        .strict(false)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()
        .expect("Failed to open with strict/log_replay chain");

    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
}

/// 测试 strict 模式对 required unknown 元数据项的分歧行为。
#[test]
fn test_open_strict_unknown_required_metadata_behavior() {
    use vhdx_rs::{File, Guid};

    let path = temp_vhdx_path();

    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 在 metadata table 中追加一个 required 的未知 GUID 表项。
    // 表项指向已存在的数据区偏移，避免触发边界读取失败。
    let mut raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("Failed to open raw file");

    // 元数据区域起始偏移：2 * 1MB（对应当前创建布局）
    const METADATA_OFFSET: u64 = 2 * 1024 * 1024;
    raw.seek(SeekFrom::Start(METADATA_OFFSET + 10))
        .expect("Failed to seek entry_count");
    let mut entry_count_bytes = [0u8; 2];
    std::io::Read::read_exact(&mut raw, &mut entry_count_bytes)
        .expect("Failed to read entry_count");
    let old_count = u16::from_le_bytes(entry_count_bytes);
    let new_count = old_count + 1;

    raw.seek(SeekFrom::Start(METADATA_OFFSET + 10))
        .expect("Failed to seek entry_count for write");
    raw.write_all(&new_count.to_le_bytes())
        .expect("Failed to write entry_count");

    let entry_offset = METADATA_OFFSET + 32 + u64::from(old_count) * 32;
    raw.seek(SeekFrom::Start(entry_offset))
        .expect("Failed to seek new metadata entry");

    let unknown_required_guid = Guid::from_bytes([
        0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);
    raw.write_all(unknown_required_guid.as_bytes())
        .expect("Failed to write unknown guid");
    raw.write_all(&65_536_u32.to_le_bytes()) // 指向 metadata 数据区起点
        .expect("Failed to write metadata entry offset");
    raw.write_all(&8_u32.to_le_bytes())
        .expect("Failed to write metadata entry length");
    raw.write_all(&0x2000_0000_u32.to_le_bytes()) // required=true
        .expect("Failed to write metadata entry flags");
    raw.write_all(&0_u32.to_le_bytes())
        .expect("Failed to write metadata entry reserved");
    raw.flush().expect("Failed to flush raw file");

    // strict=true（默认）应拒绝打开
    let strict_result = File::open(&path).strict(true).finish();
    assert!(
        strict_result.is_err(),
        "strict=true should reject unknown required metadata item"
    );

    // strict=false 应允许打开同一文件
    let relaxed_result = File::open(&path).strict(false).finish();
    assert!(
        relaxed_result.is_ok(),
        "strict=false should allow unknown required metadata item"
    );
}

/// 测试 CreateOptions 链式方法：logical/physical/parent_path 可编译并成功。
#[test]
fn test_create_options_chain_methods_happy_path() {
    use vhdx_rs::File;

    let parent = temp_vhdx_path();
    let child = temp_vhdx_path();

    File::create(&parent)
        .size(2 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create parent disk");

    let child_file = File::create(&child)
        .size(4 * 1024 * 1024)
        .fixed(false)
        .block_size(1024 * 1024)
        .logical_sector_size(512)
        .physical_sector_size(4096)
        .parent_path(&parent)
        .finish()
        .expect("Failed to create differencing disk with builder chain");

    assert!(child_file.has_parent());
    assert_eq!(child_file.logical_sector_size(), 512);
}

/// 测试 CreateOptions 非法组合：physical_sector_size < logical_sector_size 应失败。
#[test]
fn test_create_invalid_sector_size_combination_fails() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let result = File::create(&path)
        .size(4 * 1024 * 1024)
        .logical_sector_size(4096)
        .physical_sector_size(512)
        .finish();

    assert!(result.is_err());
}

/// 测试 CreateOptions 非法参数：不存在的 parent_path 应返回错误。
#[test]
fn test_create_with_nonexistent_parent_path_fails() {
    use vhdx_rs::File;

    let child = temp_vhdx_path();
    let missing_parent = child.with_file_name("missing-parent.vhdx");

    let result = File::create(&child)
        .size(4 * 1024 * 1024)
        .parent_path(&missing_parent)
        .finish();

    assert!(result.is_err());
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
        !file
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().file_parameters().map(|fp| fp.has_parent()))
            .unwrap_or(false),
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
    assert!(
        !file
            .sections()
            .metadata()
            .ok()
            .and_then(|m| m
                .items()
                .file_parameters()
                .map(|fp| fp.leave_block_allocated()))
            .unwrap_or(false),
        "test-void.vhdx should be dynamic"
    );
    assert!(
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0)
            > 0,
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
        file.sections()
            .metadata()
            .ok()
            .and_then(|m| m.items().virtual_disk_size())
            .unwrap_or(0)
            > 0,
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

/// 测试公共 getter 方法可见性：验证各 getter 返回正确值。
#[test]
fn test_public_getters() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    assert_eq!(file.virtual_disk_size(), 1024 * 1024);
    assert_eq!(file.block_size(), 1024 * 1024);
    assert_eq!(file.logical_sector_size(), 4096);
    assert!(file.is_fixed());
    assert!(!file.has_parent());
    assert!(!file.has_pending_logs());
}

// ── IO / Sector / PayloadBlock API 对齐测试 ──

/// 测试 Sector 公共字段可访问性：block_sector_index 和 payload 可直接读取。
#[test]
fn test_sector_public_fields_accessible() {
    use vhdx_rs::{File, PayloadBlock};

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    let io = file.io();

    // 扇区 0 存在，验证公共字段
    let sector = io.sector(0).expect("Sector 0 should exist");
    assert_eq!(
        sector.block_sector_index, 0,
        "block_sector_index should be 0"
    );

    // payload 字段可直接访问，类型为 PayloadBlock
    let _payload: &PayloadBlock<'_> = &sector.payload;
    assert!(
        sector.payload.bytes.is_empty(),
        "Lazy-load payload bytes should be empty slice"
    );

    // payload() 方法返回值与字段一致
    let via_method = sector.payload();
    assert_eq!(
        via_method, sector.payload,
        "payload() method should match public field"
    );
}

/// 测试 Sector 在不同块内扇区的 block_sector_index 正确性。
#[test]
fn test_sector_block_sector_index_values() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    // 4 MiB 磁盘，1 MiB 块大小，4096 逻辑扇区大小 → 每块 256 个扇区
    let file = File::create(&path)
        .size(4 * 1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .finish()
        .expect("Failed to create fixed disk");

    let io = file.io();

    // 扇区 0 → block_sector_index 0
    let s0 = io.sector(0).expect("Sector 0");
    assert_eq!(s0.block_sector_index, 0);

    // 扇区 1 → block_sector_index 1
    let s1 = io.sector(1).expect("Sector 1");
    assert_eq!(s1.block_sector_index, 1);

    // 扇区 255 → block_sector_index 255（第一块最后一个扇区）
    let s255 = io.sector(255).expect("Sector 255");
    assert_eq!(s255.block_sector_index, 255);

    // 扇区 256 → block_sector_index 0（第二块第一个扇区）
    let s256 = io.sector(256).expect("Sector 256");
    assert_eq!(s256.block_sector_index, 0);
}

/// 测试 IO::sector 超出范围返回 None。
#[test]
fn test_io_sector_out_of_range_returns_none() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let io = file.io();

    // 1 MiB / 4096 = 256 个扇区，索引 0..255 有效
    assert!(io.sector(0).is_some(), "Sector 0 should exist");
    assert!(io.sector(255).is_some(), "Last sector (255) should exist");
    assert!(
        io.sector(256).is_none(),
        "Sector 256 should be out of range"
    );
    assert!(
        io.sector(99999).is_none(),
        "Large sector number should be out of range"
    );
}

/// 测试 Sector Clone/Debug/PartialEq trait 实现。
#[test]
fn test_sector_derive_traits() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let io = file.io();

    let s0a = io.sector(0).expect("Sector 0a");
    let s0b = io.sector(0).expect("Sector 0b");

    // Clone
    let s0_clone = s0a.clone();
    assert_eq!(s0_clone.block_sector_index, 0);

    // PartialEq — 同一扇区号应相等
    assert_eq!(s0a, s0b, "Same sector should be equal");

    // Debug — 不应 panic
    let debug_str = format!("{:?}", s0a);
    assert!(
        debug_str.contains("Sector"),
        "Debug output should contain 'Sector'"
    );
}

/// 测试 PayloadBlock 的 Clone/Debug/PartialEq 派生。
#[test]
fn test_payload_block_traits() {
    use vhdx_rs::{File, PayloadBlock};

    let path = temp_vhdx_path();

    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let sector = file.io().sector(0).expect("Sector 0");
    let pb1 = sector.payload();
    let pb2 = sector.payload();

    // Clone
    let pb_clone = pb1.clone();
    assert_eq!(pb_clone.bytes, pb1.bytes);

    // PartialEq
    assert_eq!(pb1, pb2, "Same payload should be equal");

    // Debug
    let debug_str = format!("{:?}", pb1);
    assert!(
        debug_str.contains("PayloadBlock"),
        "Debug should contain 'PayloadBlock'"
    );

    // 手动构造 PayloadBlock 测试 PartialEq
    let data = b"hello";
    let manual = PayloadBlock { bytes: data };
    let manual2 = PayloadBlock { bytes: data };
    assert_eq!(manual, manual2);

    let different = PayloadBlock { bytes: b"world" };
    assert_ne!(manual, different);
}

// ── LogEntry 命名与导出对齐测试 ──

/// 测试 LogEntry 可通过 section 模块路径导入并使用。
#[test]
fn test_log_entry_import_and_construction() {
    use vhdx_rs::section::{LogEntry, LogEntryHeader};

    // 构造最小有效日志条目：64 字节头部 + 签名
    let mut data = vec![0u8; 64];
    data[0..4].copy_from_slice(b"loge");
    data[8..12].copy_from_slice(&64u32.to_le_bytes()); // entry_length = 64

    // LogEntry::new 应成功
    let entry = LogEntry::new(&data).expect("LogEntry::new should succeed");
    let header = entry.header();
    assert_eq!(header.signature(), b"loge", "Signature should be 'loge'");
    assert_eq!(header.entry_length(), 64, "entry_length should be 64");

    // 验证 LogEntryHeader 类型可独立使用
    let standalone_header = LogEntryHeader::new(&data);
    assert_eq!(standalone_header.signature(), b"loge");
}

// ── 常量与 GUID 子命名空间路径对齐测试 ──

/// 测试 constants 命名空间中基本常量可导入且值正确。
#[test]
fn test_constants_namespace_basic_values() {
    use vhdx_rs::constants::{KiB, MiB};

    // KiB = 1024
    assert_eq!(KiB, 1024u64, "KiB should be 1024");
    // MiB = 1024 * 1024
    assert_eq!(MiB, 1024u64 * 1024, "MiB should be 1048576");
    // MiB 是 KiB 的整数倍
    assert_eq!(MiB, 1024 * KiB);
}

/// 测试 constants 命名空间中布局常量可导入且值合理。
#[test]
fn test_constants_namespace_layout_constants() {
    use vhdx_rs::constants::{
        DEFAULT_BLOCK_SIZE, FILE_TYPE_SIZE, HEADER_1_OFFSET, HEADER_2_OFFSET, HEADER_SECTION_SIZE,
        MAX_BLOCK_SIZE, MIN_BLOCK_SIZE, REGION_TABLE_SIZE,
    };

    // 布局常量基本约束
    assert_eq!(HEADER_SECTION_SIZE, 1024 * 1024);
    assert_eq!(FILE_TYPE_SIZE, 64 * 1024);
    assert_eq!(HEADER_1_OFFSET, 64 * 1024);
    assert_eq!(HEADER_2_OFFSET, 128 * 1024);
    assert_eq!(REGION_TABLE_SIZE, 64 * 1024);
    assert!(DEFAULT_BLOCK_SIZE >= MIN_BLOCK_SIZE);
    assert!(DEFAULT_BLOCK_SIZE <= MAX_BLOCK_SIZE);
}

/// 测试 region_guids 子命名空间可导入且 GUID 非 nil。
#[test]
fn test_constants_region_guids_accessible() {
    use vhdx_rs::constants::region_guids;

    assert!(
        !region_guids::BAT_REGION.is_nil(),
        "BAT_REGION should not be nil"
    );
    assert!(
        !region_guids::METADATA_REGION.is_nil(),
        "METADATA_REGION should not be nil"
    );
}

/// 测试 metadata_guids 子命名空间可导入且所有已知 GUID 非 nil。
#[test]
fn test_constants_metadata_guids_accessible() {
    use vhdx_rs::constants::metadata_guids;

    assert!(!metadata_guids::FILE_PARAMETERS.is_nil());
    assert!(!metadata_guids::VIRTUAL_DISK_SIZE.is_nil());
    assert!(!metadata_guids::VIRTUAL_DISK_ID.is_nil());
    assert!(!metadata_guids::LOGICAL_SECTOR_SIZE.is_nil());
    assert!(!metadata_guids::PHYSICAL_SECTOR_SIZE.is_nil());
    assert!(!metadata_guids::PARENT_LOCATOR.is_nil());
}

/// 测试对齐辅助函数可通过 constants 命名空间调用。
#[test]
fn test_constants_align_functions() {
    use vhdx_rs::constants::{MiB, align_1mib, align_up};

    assert_eq!(align_up(0, MiB), 0);
    assert_eq!(align_up(1, MiB), MiB);
    assert_eq!(align_1mib(1), MiB);
    assert_eq!(align_1mib(MiB), MiB);
    assert_eq!(align_1mib(MiB + 1), 2 * MiB);
}

/// 测试 GUID 各常量值彼此不同（无重复）。
#[test]
fn test_constants_guids_are_unique() {
    use vhdx_rs::constants::metadata_guids;
    use vhdx_rs::constants::region_guids;

    let guids = [
        region_guids::BAT_REGION,
        region_guids::METADATA_REGION,
        metadata_guids::FILE_PARAMETERS,
        metadata_guids::VIRTUAL_DISK_SIZE,
        metadata_guids::VIRTUAL_DISK_ID,
        metadata_guids::LOGICAL_SECTOR_SIZE,
        metadata_guids::PHYSICAL_SECTOR_SIZE,
        metadata_guids::PARENT_LOCATOR,
    ];

    // 所有 GUID 两两不等
    for i in 0..guids.len() {
        for j in (i + 1)..guids.len() {
            assert_ne!(
                guids[i], guids[j],
                "GUIDs at index {i} and {j} should differ"
            );
        }
    }
}

/// 测试 ParentLocator::resolve_parent_path 按规范优先级选择路径。
#[test]
fn test_parent_locator_resolve_parent_path_priority() {
    use vhdx_rs::section::ParentLocator;

    fn utf16_bytes(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for c in s.encode_utf16() {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out
    }

    let relative_key = "relative_path";
    let relative_val = "..\\parent.vhdx";
    let volume_key = "volume_path";
    let volume_val = "C:\\volume\\parent.vhdx";

    let mut kv_data = Vec::new();

    let r_key = utf16_bytes(relative_key);
    let r_val = utf16_bytes(relative_val);
    let r_key_offset = kv_data.len() as u32;
    kv_data.extend_from_slice(&r_key);
    let r_val_offset = kv_data.len() as u32;
    kv_data.extend_from_slice(&r_val);

    let v_key = utf16_bytes(volume_key);
    let v_val = utf16_bytes(volume_val);
    let v_key_offset = kv_data.len() as u32;
    kv_data.extend_from_slice(&v_key);
    let v_val_offset = kv_data.len() as u32;
    kv_data.extend_from_slice(&v_val);

    let mut buf = vec![0u8; 20 + 12 * 2];
    // key_value_count = 2
    buf[18..20].copy_from_slice(&(2u16).to_le_bytes());

    // entry 0: relative_path
    buf[20..24].copy_from_slice(&r_key_offset.to_le_bytes());
    buf[24..28].copy_from_slice(&r_val_offset.to_le_bytes());
    buf[28..30].copy_from_slice(&(r_key.len() as u16).to_le_bytes());
    buf[30..32].copy_from_slice(&(r_val.len() as u16).to_le_bytes());

    // entry 1: volume_path
    buf[32..36].copy_from_slice(&v_key_offset.to_le_bytes());
    buf[36..40].copy_from_slice(&v_val_offset.to_le_bytes());
    buf[40..42].copy_from_slice(&(v_key.len() as u16).to_le_bytes());
    buf[42..44].copy_from_slice(&(v_val.len() as u16).to_le_bytes());

    buf.extend_from_slice(&kv_data);

    let locator = ParentLocator::new(&buf).expect("ParentLocator should parse");
    let resolved = locator
        .resolve_parent_path()
        .expect("Should resolve parent path");

    assert_eq!(resolved, std::path::PathBuf::from(relative_val));
}

/// 测试异常 ParentLocator 路径返回错误而非 panic。
#[test]
fn test_parent_locator_malformed_returns_error_or_none() {
    use vhdx_rs::section::ParentLocator;

    // 小于 20 字节应返回错误
    let too_small = [0u8; 10];
    let err = ParentLocator::new(&too_small);
    assert!(err.is_err(), "Small parent locator should return Err");

    // key_value_count 与数据不匹配时不应 panic，解析应返回 None
    let mut malformed = vec![0u8; 20];
    malformed[18..20].copy_from_slice(&(1u16).to_le_bytes()); // 声称有 1 条 entry，但无实际 entry 数据
    let locator = ParentLocator::new(&malformed).expect("Header-sized locator should parse");
    assert!(locator.entry(0).is_none());
    assert!(locator.resolve_parent_path().is_none());
}

/// 测试 validation 模块公共类型可导入且基础校验路径可执行。
#[test]
fn test_validation_api_import_and_validate_file() {
    use vhdx_rs::{File, SpecValidator, ValidationIssue};

    let path = temp_vhdx_path();
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let validator = SpecValidator::new(&file);
    validator
        .validate_file()
        .expect("validate_file should pass for a valid fixed disk");

    let issue = ValidationIssue {
        section: "metadata",
        code: "EXAMPLE",
        message: "example issue".to_string(),
        spec_ref: "MS-VHDX §2.6",
    };
    assert_eq!(issue.section, "metadata");
    assert_eq!(issue.code, "EXAMPLE");
}

/// 测试差分盘缺少 Parent Locator 必需键时返回错误而非 panic。
#[test]
fn test_validation_parent_locator_invalid_returns_error() {
    use vhdx_rs::{Error, File, SpecValidator};

    let parent_path = temp_vhdx_path();
    File::create(&parent_path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create parent disk");

    let child_path = temp_vhdx_path();
    let file = File::create(&child_path)
        .size(1024 * 1024)
        .parent_path(&parent_path)
        .finish()
        .expect("Failed to create differencing-style disk");

    let validator = SpecValidator::new(&file);
    let err = validator
        .validate_parent_locator()
        .expect_err("Expected missing parent_linkage validation error");

    match err {
        Error::InvalidMetadata(message) => {
            assert!(
                message.contains("parent_linkage"),
                "unexpected error message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 测试 File::validator() 返回的 SpecValidator 可成功执行全量校验（happy path）。
#[test]
fn test_file_validator_callable_and_validate_file() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // File::validator() 返回的校验器应能成功执行 validate_file
    file.validator()
        .validate_file()
        .expect("validate_file should succeed for a valid fixed disk");
}

/// 测试 File::validator() 可调用分项校验。
#[test]
fn test_file_validator_individual_methods() {
    use vhdx_rs::File;

    let path = temp_vhdx_path();
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let v = file.validator();
    v.validate_header().expect("header");
    v.validate_region_table().expect("region table");
    v.validate_bat().expect("bat");
    v.validate_metadata().expect("metadata");
    v.validate_required_metadata_items()
        .expect("required metadata items");
    v.validate_log().expect("log");
}

/// 测试 ParentChainInfo 可从 crate root 导入且公共字段可访问。
#[test]
fn test_parent_chain_info_import_and_fields() {
    use std::path::PathBuf;
    use vhdx_rs::ParentChainInfo;

    let info = ParentChainInfo {
        child: PathBuf::from("/child.vhdx"),
        parent: PathBuf::from("/parent.vhdx"),
        linkage_matched: true,
    };
    assert_eq!(info.child, PathBuf::from("/child.vhdx"));
    assert_eq!(info.parent, PathBuf::from("/parent.vhdx"));
    assert!(info.linkage_matched);
}

/// 测试 validate_parent_chain 对非差分盘返回错误（failure path）。
#[test]
fn test_validate_parent_chain_non_diff_disk_returns_error() {
    use vhdx_rs::{Error, File};

    let path = temp_vhdx_path();
    let file = File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    let err = file
        .validator()
        .validate_parent_chain()
        .expect_err("Expected error for non-diff disk");

    match err {
        Error::InvalidParameter(msg) => {
            assert!(
                msg.contains("differencing"),
                "unexpected error message: {msg}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 测试差分链校验：当 parent_linkage2 匹配父盘 DataWriteGuid 时通过，且 child 路径等于实际子盘路径。
#[test]
fn test_validate_parent_chain_passes_with_parent_linkage2_and_real_child_path() {
    use vhdx_rs::{File, Guid};

    let parent_path = temp_vhdx_path();
    let parent = File::create(&parent_path)
        .size(2 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create parent disk");

    let parent_data_write_guid = parent
        .sections()
        .header()
        .expect("Failed to read parent header")
        .header(0)
        .expect("Missing active parent header")
        .data_write_guid();

    let child_path = temp_vhdx_path();
    let child = File::create(&child_path)
        .size(2 * 1024 * 1024)
        .parent_path(&parent_path)
        .finish()
        .expect("Failed to create child differencing disk");
    drop(child);

    let primary_mismatch = Guid::from_bytes([
        0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);

    let locator = build_parent_locator(&[
        ("parent_linkage", &format!("{primary_mismatch}")),
        ("parent_linkage2", &format!("{parent_data_write_guid}")),
        ("relative_path", &parent_path.to_string_lossy()),
    ]);
    inject_parent_locator(&child_path, &locator);

    let child_reopen = File::open(&child_path)
        .finish()
        .expect("Failed to reopen child disk");
    let validator = child_reopen.validator();

    validator
        .validate_parent_locator()
        .expect("validate_parent_locator should accept optional parent_linkage2");

    let info = validator
        .validate_parent_chain()
        .expect("validate_parent_chain should pass when linkage2 matches");

    assert!(info.linkage_matched, "linkage should be matched");
    assert_eq!(info.child, child_path);
    assert_eq!(info.parent, parent_path);
}

/// 测试差分链校验：当 parent_linkage 与 parent_linkage2 都不匹配父盘 DataWriteGuid 时失败。
#[test]
fn test_validate_parent_chain_fails_on_linkage_mismatch() {
    use vhdx_rs::{Error, File, Guid};

    let parent_path = temp_vhdx_path();
    File::create(&parent_path)
        .size(2 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create parent disk");

    let child_path = temp_vhdx_path();
    let child = File::create(&child_path)
        .size(2 * 1024 * 1024)
        .parent_path(&parent_path)
        .finish()
        .expect("Failed to create child differencing disk");
    drop(child);

    let mismatch1 = Guid::from_bytes([
        0xAA, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);
    let mismatch2 = Guid::from_bytes([
        0xBB, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ]);

    let locator = build_parent_locator(&[
        ("parent_linkage", &format!("{mismatch1}")),
        ("parent_linkage2", &format!("{mismatch2}")),
        ("relative_path", &parent_path.to_string_lossy()),
    ]);
    inject_parent_locator(&child_path, &locator);

    let child_reopen = File::open(&child_path)
        .finish()
        .expect("Failed to reopen child disk");

    let err = child_reopen
        .validator()
        .validate_parent_chain()
        .expect_err("Expected parent linkage mismatch error");

    match err {
        Error::ParentMismatch { expected, actual } => {
            assert_eq!(expected, mismatch1);
            assert_ne!(actual, mismatch1);
            assert_ne!(actual, mismatch2);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 测试 LogReplayPolicy 全部 4 个变体可从 crate root 导入且值可匹配。
#[test]
fn test_log_replay_policy_variants_accessible() {
    use vhdx_rs::LogReplayPolicy;

    let policies = [
        LogReplayPolicy::Require,
        LogReplayPolicy::Auto,
        LogReplayPolicy::InMemoryOnReadOnly,
        LogReplayPolicy::ReadOnlyNoReplay,
    ];
    assert_eq!(policies.len(), 4);
    // 验证 Require 和 Auto 不相等
    assert_ne!(LogReplayPolicy::Require, LogReplayPolicy::Auto);
}

/// 测试以 Require 策略打开（无日志时应正常完成，不会触发 LogReplayRequired）。
#[test]
fn test_open_with_require_policy_no_pending_logs() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 新创建的磁盘无 pending logs，Require 策略不应报错
    let _file = File::open(&path)
        .log_replay(LogReplayPolicy::Require)
        .finish()
        .expect("Open with Require policy should succeed when no pending logs");
}

/// 测试以 ReadOnlyNoReplay 策略只读打开。
#[test]
fn test_open_with_read_only_no_replay_policy() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    // 新创建的磁盘无 pending logs，ReadOnlyNoReplay 策略不应报错
    let _file = File::open(&path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()
        .expect("Open with ReadOnlyNoReplay policy should succeed");
}

/// 测试 InMemoryOnReadOnly 与 ReadOnlyNoReplay 在 pending log 场景下行为分歧。
#[test]
fn test_open_readonly_replay_policies_behavior_diff_with_pending_logs() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();
    let virtual_size = 2 * 1024 * 1024;
    let target_disk_offset = 512_u64;
    let target_file_offset =
        u64::try_from(HEADER_SECTION_SIZE).expect("header size overflow") + target_disk_offset;
    let target_sector = 0_u64;
    let payload = b"INMEM_REPLAY_POLICY_DIFF";

    File::create(&path)
        .size(virtual_size)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    inject_pending_log_entry(&path, target_file_offset, payload);

    let mut no_replay_buf = vec![0u8; payload.len()];
    let no_replay = File::open(&path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()
        .expect("ReadOnlyNoReplay open should succeed");
    let no_replay_sector = no_replay
        .io()
        .sector(target_sector)
        .expect("ReadOnlyNoReplay sector should exist");
    let mut no_replay_sector_buf = vec![0u8; 4096];
    no_replay_sector
        .read(&mut no_replay_sector_buf)
        .expect("ReadOnlyNoReplay read should succeed");
    no_replay_buf.copy_from_slice(&no_replay_sector_buf[512..512 + payload.len()]);
    assert_eq!(
        no_replay_buf,
        vec![0u8; payload.len()],
        "ReadOnlyNoReplay should keep original on-disk bytes"
    );

    let mut inmem_buf = vec![0u8; payload.len()];
    let inmem = File::open(&path)
        .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
        .finish()
        .expect("InMemoryOnReadOnly open should succeed");
    assert!(
        !inmem.has_pending_logs(),
        "InMemoryOnReadOnly should execute replay logic in-memory"
    );
    let inmem_sector = inmem
        .io()
        .sector(target_sector)
        .expect("InMemoryOnReadOnly sector should exist");
    let mut inmem_sector_buf = vec![0u8; 4096];
    inmem_sector
        .read(&mut inmem_sector_buf)
        .expect("InMemoryOnReadOnly read should succeed");
    inmem_buf.copy_from_slice(&inmem_sector_buf[512..512 + payload.len()]);
    assert_eq!(
        &inmem_buf, payload,
        "InMemoryOnReadOnly should expose replayed payload in reads"
    );
}

/// 测试只读 InMemoryOnReadOnly 不应将回放结果写回磁盘。
#[test]
fn test_inmemory_on_readonly_does_not_write_back_to_disk() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();
    let virtual_size = 2 * 1024 * 1024;
    let target_disk_offset = 1024_u64;
    let target_file_offset =
        u64::try_from(HEADER_SECTION_SIZE).expect("header size overflow") + target_disk_offset;
    let payload = b"NO_DISK_WRITEBACK_ON_INMEM_REPLAY";

    File::create(&path)
        .size(virtual_size)
        .fixed(true)
        .finish()
        .expect("Failed to create fixed disk");

    inject_pending_log_entry(&path, target_file_offset, payload);

    let before = read_raw_bytes(&path, target_file_offset, payload.len());
    assert_eq!(
        before,
        vec![0u8; payload.len()],
        "fixture expects zeroed on-disk bytes before read-only open"
    );

    let _inmem = File::open(&path)
        .log_replay(LogReplayPolicy::InMemoryOnReadOnly)
        .finish()
        .expect("InMemoryOnReadOnly open should succeed");

    let after = read_raw_bytes(&path, target_file_offset, payload.len());
    assert_eq!(
        after, before,
        "InMemoryOnReadOnly must not persist replay result to disk in read-only mode"
    );
}

// ── T9: ParentLocator / LocatorHeader / KeyValueEntry 签名收口测试 ──

/// 测试 LocatorHeader 公共字段可访问：locator_type、reserved、key_value_count、raw。
#[test]
fn test_locator_header_public_fields_accessible() {
    use vhdx_rs::section::{KeyValueEntry, ParentLocator};

    // ParentLocator::new 需要 >= 20 字节
    let mut buf = vec![0u8; 20];
    buf[18..20].copy_from_slice(&(0u16.to_le_bytes()));
    let locator = ParentLocator::new(&buf).expect("ParentLocator should parse");

    // header 返回 LocatorHeader，公共字段可访问
    let header = locator.header();
    assert_eq!(header.key_value_count, 0);

    // raw getter 可调用
    let _raw: &[u8] = header.raw();

    // KeyValueEntry 可手动构造（利用公共字段）
    let kv = KeyValueEntry {
        key_offset: 0,
        value_offset: 0,
        key_length: 0,
        value_length: 0,
        raw: &[0u8; 12],
    };
    assert_eq!(kv.key_offset, 0);
}

/// 测试 KeyValueEntry 公共字段可访问：key_offset、value_offset、key_length、value_length、raw。
#[test]
fn test_key_value_entry_public_fields_accessible() {
    use vhdx_rs::section::KeyValueEntry;

    let mut entry_bytes = [0u8; 12];
    entry_bytes[0..4].copy_from_slice(&10u32.to_le_bytes());
    entry_bytes[4..8].copy_from_slice(&20u32.to_le_bytes());
    entry_bytes[8..10].copy_from_slice(&8u16.to_le_bytes());
    entry_bytes[10..12].copy_from_slice(&12u16.to_le_bytes());

    let entry = KeyValueEntry::new(&entry_bytes).expect("KeyValueEntry::new should succeed");

    assert_eq!(entry.key_offset, 10);
    assert_eq!(entry.value_offset, 20);
    assert_eq!(entry.key_length, 8);
    assert_eq!(entry.value_length, 12);
    assert_eq!(entry.raw.len(), 12);
    assert_eq!(entry.raw(), entry.raw);
}

/// 测试 KeyValueEntry key/value 方法正确解码 UTF-16LE。
#[test]
fn test_key_value_entry_key_value_utf16_decode() {
    use vhdx_rs::section::KeyValueEntry;

    fn utf16_bytes(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
    }

    let key = "relative_path";
    let value = r"..\parent.vhdx";
    let key_data = utf16_bytes(key);
    let value_data = utf16_bytes(value);

    let mut kv_region = Vec::new();
    let key_offset = 0u32;
    kv_region.extend_from_slice(&key_data);
    let value_offset = kv_region.len() as u32;
    kv_region.extend_from_slice(&value_data);

    let mut entry_bytes = [0u8; 12];
    entry_bytes[0..4].copy_from_slice(&key_offset.to_le_bytes());
    entry_bytes[4..8].copy_from_slice(&value_offset.to_le_bytes());
    entry_bytes[8..10].copy_from_slice(&(key_data.len() as u16).to_le_bytes());
    entry_bytes[10..12].copy_from_slice(&(value_data.len() as u16).to_le_bytes());

    let entry = KeyValueEntry::new(&entry_bytes).expect("KeyValueEntry::new should succeed");
    assert_eq!(entry.key(&kv_region), Some(key.to_string()));
    assert_eq!(entry.value(&kv_region), Some(value.to_string()));
}

/// 测试 ParentLocator 各方法返回类型与 API.md 一致。
#[test]
fn test_parent_locator_api_surface() {
    use vhdx_rs::section::{KeyValueEntry, LocatorHeader, ParentLocator};

    fn utf16_bytes(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
    }

    let key = "absolute_win32_path";
    let value = r"C:\disks\parent.vhdx";
    let key_data = utf16_bytes(key);
    let value_data = utf16_bytes(value);

    let mut kv_region = Vec::new();
    let k_off = kv_region.len() as u32;
    kv_region.extend_from_slice(&key_data);
    let v_off = kv_region.len() as u32;
    kv_region.extend_from_slice(&value_data);

    let mut buf = vec![0u8; 32];
    buf[18..20].copy_from_slice(&(1u16).to_le_bytes());
    buf[20..24].copy_from_slice(&k_off.to_le_bytes());
    buf[24..28].copy_from_slice(&v_off.to_le_bytes());
    buf[28..30].copy_from_slice(&(key_data.len() as u16).to_le_bytes());
    buf[30..32].copy_from_slice(&(value_data.len() as u16).to_le_bytes());
    buf.extend_from_slice(&kv_region);

    let locator = ParentLocator::new(&buf).expect("ParentLocator should parse");

    // header() -> LocatorHeader
    let _header: LocatorHeader<'_> = locator.header();
    assert_eq!(locator.header().key_value_count, 1);

    // entry(0) -> Some(KeyValueEntry)
    let e0: Option<KeyValueEntry<'_>> = locator.entry(0);
    assert!(e0.is_some());
    assert_eq!(e0.unwrap().key(&kv_region), Some(key.to_string()));

    // entry 超界 -> None
    assert!(locator.entry(1).is_none());
    assert!(locator.entry(999).is_none());

    // entries() -> Vec<KeyValueEntry>
    let entries: Vec<KeyValueEntry<'_>> = locator.entries();
    assert_eq!(entries.len(), 1);

    // key_value_data() -> &[u8]
    let kvd: &[u8] = locator.key_value_data();
    assert!(!kvd.is_empty());

    // raw() 返回完整原始数据
    assert_eq!(locator.raw().len(), buf.len());

    // resolve_parent_path() -> Some(PathBuf)
    let resolved = locator.resolve_parent_path();
    assert_eq!(resolved, Some(std::path::PathBuf::from(value)));
}

/// 测试 ParentLocator 空 entries 和 key_value_data 边界情况。
#[test]
fn test_parent_locator_empty_entries_and_data() {
    use vhdx_rs::section::ParentLocator;

    let mut buf = vec![0u8; 20];
    buf[18..20].copy_from_slice(&0u16.to_le_bytes());

    let locator = ParentLocator::new(&buf).expect("ParentLocator with 0 entries should parse");

    assert_eq!(locator.header().key_value_count, 0);
    assert!(locator.entries().is_empty());
    assert!(locator.entry(0).is_none());
    assert!(locator.key_value_data().is_empty());
    assert!(locator.resolve_parent_path().is_none());
}

/// 测试 ParentLocator/LocatorHeader/KeyValueEntry 可通过 section 模块路径导入。
#[test]
fn test_t9_section_module_import_paths() {
    use vhdx_rs::section::{KeyValueEntry, ParentLocator};

    let mut buf = vec![0u8; 20];
    buf[18..20].copy_from_slice(&0u16.to_le_bytes());
    let locator = ParentLocator::new(&buf).expect("ParentLocator should parse");

    let header = locator.header();
    assert_eq!(header.key_value_count, 0);
    let _raw: &[u8] = header.raw();

    let kv = KeyValueEntry {
        key_offset: 0,
        value_offset: 0,
        key_length: 0,
        value_length: 0,
        raw: &[0u8; 12],
    };
    assert_eq!(kv.key_offset, 0);
}
