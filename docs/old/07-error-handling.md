# 错误处理与测试

本文档详细介绍 vhdx-rs 旧版的错误处理机制和测试策略。

---

## 1. 错误类型系统

### 1.1 VhdxError 枚举

**文件**: `src/error.rs`

```rust
#[derive(Error, Debug)]
pub enum VhdxError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid signature: expected {expected:?}, got {got:?}")]
    InvalidSignature { expected: String, got: String },

    #[error("Invalid checksum")]
    InvalidChecksum,

    #[error("Invalid file type identifier")]
    InvalidFileType,

    #[error("No valid header found")]
    NoValidHeader,

    #[error("Corrupt VHDX file: {0}")]
    Corrupt(String),


    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),

    #[error("Invalid region: {0}")]
    InvalidRegion(String),

    #[error("Required region not found: {0}")]
    RequiredRegionNotFound(String),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Invalid BAT entry")]
    InvalidBatEntry,

    #[error("Block not present")]
    BlockNotPresent,

    #[error("Log replay failed: {0}")]
    LogReplayFailed(String),

    #[error("Invalid log entry")]
    InvalidLogEntry,

    #[error("Parent disk not found: {0}")]
    ParentNotFound(String),

    #[error("Parent GUID mismatch")]
    ParentGuidMismatch,

    #[error("Invalid sector bitmap")]
    InvalidSectorBitmap,

    #[error("Invalid virtual offset: {0}")]
    InvalidOffset(u64),

    #[error("File too small")]
    FileTooSmall,

    #[error("Alignment error: {0} is not aligned to {1}")]
    Alignment(u64, u64),
}

pub type Result<T> = std::result::Result<T, VhdxError>;
```

### 1.2 错误分类

| 类别 | 错误类型 | 说明 |
|------|----------|------|
| **I/O 错误** | `Io` | 底层文件系统错误（权限、磁盘满等） |
| **格式错误** | `InvalidSignature`, `InvalidFileType` | 文件不是有效的 VHDX |
| **完整性错误** | `InvalidChecksum`, `Corrupt` | 文件已损坏 |
| **版本错误** | `UnsupportedVersion` | 不支持的 VHDX 版本 |
| **结构错误** | `NoValidHeader`, `RequiredRegionNotFound` | 文件结构不完整 |
| **元数据错误** | `InvalidMetadata`, `InvalidRegion` | 元数据无效 |
| **BAT 错误** | `InvalidBatEntry`, `BlockNotPresent` | 块分配表问题 |
| **日志错误** | `LogReplayFailed`, `InvalidLogEntry` | 日志系统错误 |
| **差异磁盘错误** | `ParentNotFound`, `ParentGuidMismatch` | 父子链问题 |
| **数据错误** | `InvalidOffset`, `FileTooSmall` | 数据访问问题 |
| **对齐错误** | `Alignment` | 对齐要求不满足 |

---

## 2. 错误处理策略

### 2.1 传播错误

```rust
use vhdx_rs::VhdxError;

// 使用 ? 操作符自动传播
fn open_and_read(path: &Path) -> Result<Vec<u8>, VhdxError> {
    let mut vhdx = VhdxFile::open(path, true)?;  // 传播打开错误
    
    let mut buffer = vec![0u8; 4096];
    vhdx.read(0, &mut buffer)?;  // 传播读取错误
    
    Ok(buffer)
}
```

#### 2.2 转换错误类型

```rust
use vhdx_rs::VhdxError;

fn read_with_context(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(path, true)
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    
    let mut buffer = vec![0u8; 4096];
    vhdx.read(0, &mut buffer)
        .map_err(|e| format!("Failed to read from {}: {}", path.display(), e))?;
    
    Ok(buffer)
}
```

### 2.3 恢复策略

```rust
use vhdx_rs::{VhdxFile, VhdxError};

fn robust_open(path: &Path) -> Result<VhdxFile, VhdxError> {
    match VhdxFile::open(path, true) {
        Ok(vhdx) => Ok(vhdx),
        Err(VhdxError::NoValidHeader) => {
            // 尝试恢复：检查文件是否至少有一个有效 Header
            eprintln!("Warning: Attempting recovery...");
            // 实现恢复逻辑...
            Err(VhdxError::NoValidHeader)
        }
        Err(e) => Err(e),
    }
}
```

### 2.4 用户友好的错误信息

```rust
use vhdx_rs::VhdxError;

fn user_friendly_error(e: &VhdxError) -> String {
    match e {
        VhdxError::InvalidSignature { expected, got } => format!(
            "This doesn't appear to be a valid VHDX file. \
             Expected signature '{}', but found '{}'.",
            expected, got
        ),
        VhdxError::NoValidHeader => format!(
            "The VHDX file appears to be corrupted. \
             Neither of the two headers is valid."
        ),
        VhdxError::ParentNotFound(path) => format!(
            "Cannot find the parent disk '{}'. \
             Please ensure the parent disk exists and is accessible.",
            path
        ),
        VhdxError::UnsupportedVersion(v) => format!(
            "This VHDX file uses version {}, which is not supported. \
             Please upgrade to a newer version of the library.",
            v
        ),
        _ => format!("An error occurred: {}", e),
    }
}
```

---

## 3. 单元测试

### 3.1 Header 测试

**文件**: `src/header/header.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::{ByteOrder, LittleEndian};

    #[test]
    fn test_vhdx_header() {
        let header = VhdxHeader::new(1);
        let mut bytes = header.to_bytes();

        // 更新校验和
        let checksum = crc32c_with_zero_field(&bytes, 4, 4);
        LittleEndian::write_u32(&mut bytes[4..8], checksum);

        // 解析回结构
        let header2 = VhdxHeader::from_bytes(&bytes).unwrap();
        
        // 验证
        assert!(header2.is_valid(&bytes));
        assert_eq!(header.sequence_number, header2.sequence_number);
        assert_eq!(header.version, header2.version);
    }

    #[test]
    fn test_header_signature() {
        let header = VhdxHeader::new(1);
        assert_eq!(&header.signature, b"head");
    }

    #[test]
    fn test_header_version() {
        let header = VhdxHeader::new(1);
        assert_eq!(header.version, 1);
        assert!(header.check_version().is_ok());
    }

    #[test]
    fn test_invalid_version() {
        let mut header = VhdxHeader::new(1);
        header.version = 2;  // 不支持的版本
        
        assert!(matches!(
            header.check_version(),
            Err(VhdxError::UnsupportedVersion(2))
        ));
    }
}
```

### 3.2 BAT 测试

**文件**: `src/bat/table.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_calculation() {
        let block_size = 1024 * 1024;  // 1MB
        let logical_sector_size = 512;

        let chunk_ratio = Bat::calculate_chunk_ratio(block_size, logical_sector_size);
        let chunk_size = (1u64 << 23) * logical_sector_size as u64;

        assert_eq!(chunk_size, 4 * 1024 * 1024 * 1024);  // 4GB
        assert_eq!(chunk_ratio, 4096);  // 4096 blocks per chunk
    }

    #[test]
    fn test_bat_index_calculation() {
        let bat = Bat {
            entries: vec![],
            virtual_disk_size: 100 * 1024 * 1024 * 1024,  // 100GB
            block_size: 1024 * 1024,  // 1MB
            logical_sector_size: 512,
            num_payload_blocks: 100 * 1024,
            num_sector_bitmap_blocks: 25,
            chunk_ratio: 4096,
            chunk_size: 4 * 1024 * 1024 * 1024,
            bat_file_offset: 1024 * 1024,
        };

        // Block 0 -> Index 0
        assert_eq!(bat.payload_bat_index(0), Some(0));

        // Block 4095 -> Index 4095
        assert_eq!(bat.payload_bat_index(4095), Some(4095));

        // Block 4096 -> Index 4097 (after sector bitmap)
        assert_eq!(bat.payload_bat_index(4096), Some(4097));

        // Sector bitmap 0 -> Index 4096
        assert_eq!(bat.sector_bitmap_bat_index(0), Some(4096));
    }

    #[test]
    fn test_bat_translate() {
        // 创建包含一个 FullyPresent 块的 BAT
        let mut entries = vec![];
        entries.push(BatEntry::new(PayloadBlockState::FullyPresent, 3)); // 3MB
        
        let bat = Bat {
            entries,
            virtual_disk_size: 1024 * 1024,
            block_size: 1024 * 1024,
            logical_sector_size: 512,
            num_payload_blocks: 1,
            num_sector_bitmap_blocks: 1,
            chunk_ratio: 4096,
            chunk_size: 4 * 1024 * 1024 * 1024,
            bat_file_offset: 0,
        };

        // 虚拟偏移 0 应该映射到文件偏移 3MB
        let result = bat.translate(0).unwrap();
        assert_eq!(result, Some(3 * 1024 * 1024));
    }
}
```

### 3.3 Log 测试

**文件**: `src/log/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::crc32c::crc32c_with_zero_field;
    use byteorder::{ByteOrder, LittleEndian};

    #[test]
    fn test_log_entry_header() {
        let mut data = vec![0u8; 4096];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        LittleEndian::write_u32(&mut data[8..12], 4096);  // entry_length
        LittleEndian::write_u32(&mut data[12..16], 0);   // tail
        LittleEndian::write_u64(&mut data[16..24], 1); // sequence_number
        LittleEndian::write_u32(&mut data[24..28], 0);   // descriptor_count

        // 计算并写入校验和
        let checksum = crc32c_with_zero_field(&data, 4, 4);
        LittleEndian::write_u32(&mut data[4..8], checksum);

        let header = LogEntryHeader::from_bytes(&data).unwrap();
        assert!(header.verify_checksum(&data));
        assert_eq!(header.sequence_number, 1);
        assert_eq!(header.entry_length, 4096);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        LittleEndian::write_u64(&mut data[8..16], 4096);  // zero_length
        LittleEndian::write_u64(&mut data[16..24], 4096);   // file_offset
        LittleEndian::write_u64(&mut data[24..32], 1);      // sequence_number

        let desc = ZeroDescriptor::from_bytes(&data).unwrap();
        assert_eq!(desc.zero_length, 4096);
        assert_eq!(desc.file_offset, 4096);
        assert!(desc.verify_sequence(1));
    }
}
```

### 3.4 GUID 测试

**文件**: `src/common/guid.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_nil() {
        let nil_guid = Guid::nil();
        assert!(nil_guid.is_nil());
        assert_eq!(nil_guid.to_bytes(), [0u8; 16]);
    }

    #[test]
    fn test_guid_roundtrip() {
        let bytes: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42,
            0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
        ];
        let guid = Guid::from_bytes(bytes);
        let roundtrip = guid.to_bytes();
        assert_eq!(bytes, roundtrip);
    }

    #[test]
    fn test_guid_display() {
        let bytes: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42,
            0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
        ];
        let guid = Guid::from_bytes(bytes);
        let s = format!("{}", guid);
        assert_eq!(s, "2dc27766-f623-4200-9d64-115e9bfd4a08");
    }
}
```

---

## 4. 集成测试

### 4.1 完整工作流测试

**文件**: `tests/integration/full_workflow.rs`

```rust
use vhdx_rs::{VhdxBuilder, VhdxFile, DiskType};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_create_and_open() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.vhdx");
    
    // 创建
    let vhdx = VhdxBuilder::new(1024 * 1024 * 100)  // 100MB
        .disk_type(DiskType::Dynamic)
        .create(&path)
        .unwrap();
    
    assert_eq!(vhdx.virtual_disk_size(), 1024 * 1024 * 100);
    assert_eq!(vhdx.disk_type(), DiskType::Dynamic);
    
    // 重新打开
    let vhdx2 = VhdxFile::open(&path, true).unwrap();
    assert_eq!(vhdx2.virtual_disk_size(), vhdx.virtual_disk_size());
}

#[test]
fn test_read_write() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.vhdx");
    
    // 创建并写入
    {
        let mut vhdx = VhdxBuilder::new(1024 * 1024 * 10)
            .disk_type(DiskType::Dynamic)
            .create(&path)
            .unwrap();
        
        let data = b"Hello, VHDX!";
        vhdx.write(0, data).unwrap();
    }
    
    // 重新打开并读取
    {
        let mut vhdx = VhdxFile::open(&path, true).unwrap();
        let mut buffer = vec![0u8; 12];
        let bytes_read = vhdx.read(0, &mut buffer).unwrap();
        
        assert_eq!(bytes_read, 12);
        assert_eq!(&buffer, b"Hello, VHDX!");
    }
}

#[test]
fn test_fixed_disk() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("fixed.vhdx");
    
    let vhdx = VhdxBuilder::new(1024 * 1024 * 10)  // 10MB
        .disk_type(DiskType::Fixed)
        .create(&path)
        .unwrap();
    
    assert_eq!(vhdx.disk_type(), DiskType::Fixed);
    
    // 验证文件大小
    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 1024 * 1024 * 10);  // 至少 10MB
}
```

### 4.2 错误场景测试

```rust
#[test]
fn test_invalid_file() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("not_vhdx.txt");
    
    // 创建非 VHDX 文件
    std::fs::write(&path, "This is not a VHDX file").unwrap();
    
    // 应该返回 InvalidSignature 错误
    let result = VhdxFile::open(&path, true);
    assert!(matches!(result, Err(VhdxError::InvalidSignature { .. })));
}

#[test]
fn test_readonly_write() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("readonly.vhdx");
    
    // 创建
    VhdxBuilder::new(1024 * 1024 * 10)
        .create(&path)
        .unwrap();
    
    // 以只读模式打开
    let mut vhdx = VhdxFile::open(&path, true).unwrap();
    
    // 尝试写入应该失败
    let result = vhdx.write(0, b"test");
    assert!(result.is_err());
}

#[test]
fn test_out_of_bounds() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.vhdx");
    
    let mut vhdx = VhdxBuilder::new(1024)  // 1KB
        .create(&path)
        .unwrap();
    
    // 读取超出范围
    let mut buffer = vec![0u8; 100];
    let result = vhdx.read(1024, &mut buffer);  // 偏移 1KB
    assert!(matches!(result, Err(VhdxError::InvalidOffset(1024))));
}
```

---

## 5. 测试工具与辅助函数

### 5.1 测试辅助模块

```rust
// tests/common/mod.rs

use vhdx_rs::{VhdxBuilder, VhdxFile, DiskType};
use std::path::Path;
use tempfile::TempDir;

pub fn create_test_vhdx(size: u64, disk_type: DiskType) -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.vhdx");
    
    VhdxBuilder::new(size)
        .disk_type(disk_type)
        .create(&path)
        .unwrap();
    
    (temp_dir, path)
}

pub fn verify_pattern(vhdx: &mut VhdxFile, offset: u64, expected: &[u8]) {
    let mut buffer = vec![0u8; expected.len()];
    let bytes_read = vhdx.read(offset, &mut buffer).unwrap();
    assert_eq!(bytes_read, expected.len());
    assert_eq!(&buffer, expected);
}

pub fn write_pattern(vhdx: &mut VhdxFile, offset: u64, pattern: &[u8]) {
    vhdx.write(offset, pattern).unwrap();
}
```

### 5.2 使用辅助函数

```rust
use tests::common::*;

#[test]
fn test_pattern_verification() {
    let (temp_dir, path) = create_test_vhdx(1024 * 1024, DiskType::Dynamic);
    
    let mut vhdx = VhdxFile::open(&path, false).unwrap();
    
    // 写入模式
    write_pattern(&mut vhdx, 0, b"PATTERN_1");
    write_pattern(&mut vhdx, 4096, b"PATTERN_2");
    
    // 验证模式
    verify_pattern(&mut vhdx, 0, b"PATTERN_1");
    verify_pattern(&mut vhdx, 4096, b"PATTERN_2");
}
```

---

## 6. 性能测试

### 6.1 基准测试

```rust
// benches/vhdx_bench.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vhdx_rs::{VhdxBuilder, VhdxFile, DiskType};
use tempfile::TempDir;

fn bench_read(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("bench.vhdx");
    
    // 创建并填充
    let mut vhdx = VhdxBuilder::new(1024 * 1024 * 100)  // 100MB
        .disk_type(DiskType::Fixed)
        .create(&path)
        .unwrap();
    
    let data = vec![0xABu8; 4096];
    for i in 0..(100 * 1024 * 1024 / 4096) {
        vhdx.write(i as u64 * 4096, &data).unwrap();
    }
    
    c.bench_function("read_4k", |b| {
        let mut vhdx = VhdxFile::open(&path, true).unwrap();
        let mut buffer = vec![0u8; 4096];
        let mut offset = 0u64;
        
        b.iter(|| {
            vhdx.read(offset, &mut buffer).unwrap();
            offset = (offset + 4096) % (100 * 1024 * 1024);
        });
    });
}

fn bench_write(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("bench.vhdx");
    
    c.bench_function("write_4k", |b| {
        let mut vhdx = VhdxBuilder::new(1024 * 1024 * 100)
            .disk_type(DiskType::Dynamic)
            .create(&path)
            .unwrap();
        
        let data = vec![0xABu8; 4096];
        let mut offset = 0u64;
        
        b.iter(|| {
            vhdx.write(offset, black_box(&data)).unwrap();
            offset = (offset + 4096) % (100 * 1024 * 1024);
        });
    });
}

criterion_group!(benches, bench_read, bench_write);
criterion_main!(benches);
```

---

## 7. 持续集成

### 7.1 GitHub Actions 配置

```yaml
# .github/workflows/ci.yml

name: CI

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
        rust: [stable, beta]

    steps:
    - uses: actions/checkout@v2
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: ${{ matrix.rust }}
        override: true
    
    - name: Build
      run: cargo build --verbose
    
    - name: Run tests
      run: cargo test --verbose
    
    - name: Run clippy
      run: cargo clippy -- -D warnings
    
    - name: Check formatting
      run: cargo fmt -- --check
```

---

## 8. 调试技巧

### 8.1 启用日志

```rust
// 在测试中添加日志输出
#[test]
fn test_with_logging() {
    env_logger::init();
    
    log::debug!("Starting test...");
    
    // 测试代码...
}
```

### 8.2 失败测试调试

```rust
#[test]
fn test_debug_on_failure() {
    let result = some_operation();
    
    if let Err(ref e) = result {
        eprintln!("Operation failed: {:?}", e);
        eprintln!("Backtrace: {}", std::backtrace::Backtrace::capture());
    }
    
    assert!(result.is_ok());
}
```

### 8.3 临时文件检查

```rust
#[test]
fn test_keep_temp() {
    use std::env;
    
    // 设置环境变量保留临时目录
    env::set_var("VHDX_KEEP_TEMP", "1");
    
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("debug.vhdx");
    
    // 测试代码...
    
    println!("Temp dir: {}", temp_dir.path().display());
    // 手动检查生成的文件
}
```

---

## 9. 测试覆盖率

### 9.1 生成覆盖率报告

```bash
# 使用 tarpaulin
cargo install cargo-tarpaulin
cargo tarpaulin --out Html

# 或使用 grcov
# 见 grcov 文档
```

### 9.2 覆盖率目标

| 模块 | 目标覆盖率 | 当前状态 |
|------|-----------|----------|
| common/ | 90% | - |
| header/ | 95% | - |
| bat/ | 90% | - |
| log/ | 85% | - |
| metadata/ | 90% | - |
| block_io/ | 80% | - |
| file/ | 85% | - |

---

## 10. 参考文档

- [Rust Testing Book](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Crate: tempfile](https://docs.rs/tempfile)
- [Crate: criterion](https://docs.rs/criterion)
- [01-architecture-overview.md](./01-architecture-overview.md) - 架构概述
