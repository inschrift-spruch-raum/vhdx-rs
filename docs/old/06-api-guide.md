# API 使用指南

本文档详细介绍如何在 Rust 代码中使用 vhdx-rs 库，包括基本用法、高级特性和最佳实践。

---

## 1. 添加依赖

在 `Cargo.toml` 中添加：

```toml
[dependencies]
vhdx-rs = { path = "path/to/vhdx-rs" }
```

或使用 crates.io（发布后）：

```toml
[dependencies]
vhdx-rs = "0.1"
```

---

## 2. 基本使用

### 2.1 导入模块

```rust
use vhdx_rs::{
    VhdxFile,
    VhdxBuilder,
    DiskType,
    VhdxError,
};
use std::path::Path;
```

### 2.2 打开现有 VHDX

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 以只读模式打开
    let vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;
    
    println!("Virtual disk size: {} bytes", vhdx.virtual_disk_size());
    println!("Block size: {} bytes", vhdx.block_size());
    println!("Disk type: {:?}", vhdx.disk_type());
    
    Ok(())
}
```

### 2.3 创建新 VHDX

```rust
use vhdx_rs::{VhdxBuilder, DiskType};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 动态磁盘
    let vhdx = VhdxBuilder::new(10 * 1024 * 1024 * 1024)
        .disk_type(DiskType::Dynamic)
        .create(Path::new("new_disk.vhdx"))?;
    
    println!("Created VHDX: {}", vhdx.virtual_disk_id());
    
    Ok(())
}
```

---

## 3. 读写操作

### 3.1 读取数据

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn read_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;
    
    // 读取前 4096 字节
    let mut buffer = vec![0u8; 4096];
    let bytes_read = vhdx.read(0, &mut buffer)?;
    
    println!("Read {} bytes", bytes_read);
    
    // 处理数据...
    
    Ok(())
}
```

### 3.2 写入数据

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn write_example() -> Result<(), Box<dyn std::error::Error>> {
    // 以读写模式打开
    let mut vhdx = VhdxFile::open(Path::new("disk.vhdx"), false)?;
    
    // 准备数据
    let data = b"Hello, VHDX World!";
    
    // 写入到偏移 1024
    let bytes_written = vhdx.write(1024, data)?;
    
    println!("Wrote {} bytes", bytes_written);
    
    Ok(())
}
```

### 3.3 分块读写

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn chunked_io_example() -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(Path::new("disk.vhdx"), false)?;
    
    let chunk_size = 64 * 1024; // 64KB
    let data = vec![0xABu8; chunk_size];
    
    // 分块写入 10MB 数据
    for i in 0..160 {
        let offset = i as u64 * chunk_size as u64;
        vhdx.write(offset, &data)?;
    }
    
    // 分块读取
    let mut buffer = vec![0u8; chunk_size];
    for i in 0..160 {
        let offset = i as u64 * chunk_size as u64;
        let bytes_read = vhdx.read(offset, &mut buffer)?;
        assert_eq!(bytes_read, chunk_size);
    }
    
    Ok(())
}
```

---

## 4. Builder 模式详解

### 4.1 链式配置

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

let vhdx = VhdxBuilder::new(50 * 1024 * 1024 * 1024)  // 50GB
    .disk_type(DiskType::Fixed)
    .block_size(32 * 1024 * 1024)  // 32MB
    .sector_sizes(512, 4096)
    .create(Path::new("custom.vhdx"))?;
```

### 4.2 可选配置

| 方法 | 参数 | 说明 | 默认值 |
|------|------|------|--------|
| `new()` | `size: u64` | 必需：虚拟磁盘大小 | - |
| `disk_type()` | `DiskType` | 磁盘类型 | `Dynamic` |
| `block_size()` | `u32` | 块大小（1MB-256MB） | 32MB |
| `sector_sizes()` | `(u32, u32)` | 逻辑/物理扇区 | `(512, 4096)` |
| `parent_path()` | `String` | 父磁盘路径 | `None` |
| `creator()` | `String` | 创建者标识 | `"Rust VHDX Library"` |

### 4.3 配置验证

```rust
use vhdx_rs::{VhdxBuilder, DiskType, VhdxError};

fn create_with_validation() -> Result<(), VhdxError> {
    // 无效：块大小不是 1MB 的倍数
    let result = VhdxBuilder::new(10 * 1024 * 1024 * 1024)
        .block_size(1.5 * 1024 * 1024 as u32)  // 1.5MB
        .create(Path::new("test.vhdx"));
    
    assert!(result.is_err()); // InvalidMetadata
    
    // 无效：扇区大小不是 512 或 4096
    let result = VhdxBuilder::new(10 * 1024 * 1024 * 1024)
        .sector_sizes(1024, 4096)  // 逻辑扇区 1024 无效
        .create(Path::new("test.vhdx"));
    
    assert!(result.is_err());
    
    Ok(())
}
```

---

## 5. 磁盘类型

### 5.1 固定磁盘 (Fixed)

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

// 创建固定磁盘
let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)  // 100GB
    .disk_type(DiskType::Fixed)
    .create(Path::new("fixed.vhdx"))?;

// 特点：
// - 文件大小 = 虚拟磁盘大小 + 元数据（约3MB）
// - 最佳读写性能
// - 创建时间较长
// - 无碎片问题
```

### 5.2 动态磁盘 (Dynamic)

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

// 创建动态磁盘
let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)  // 100GB
    .disk_type(DiskType::Dynamic)
    .create(Path::new("dynamic.vhdx"))?;

// 特点：
// - 文件大小按需增长
// - 创建速度快
// - 适合稀疏数据
// - 可能有碎片
```

### 5.3 差异磁盘 (Differencing)

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

// 基于父磁盘创建差异磁盘
let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)
    .disk_type(DiskType::Differencing)
    .parent_path("parent.vhdx")
    .create(Path::new("snapshot.vhdx"))?;

// 特点：
// - 仅存储与父磁盘的差异
// - 支持快照链
// - 读取可能查询父链
// - 首次写入有 CoW 开销
```

### 5.4 检测磁盘类型

```rust
use vhdx_rs::{VhdxFile, DiskType};

let vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;

match vhdx.disk_type() {
    DiskType::Fixed => println!("Fixed disk"),
    DiskType::Dynamic => println!("Dynamic disk"),
    DiskType::Differencing => {
        println!("Differencing disk");
        if vhdx.has_parent() {
            println!("Has parent disk");
        }
    }
}
```

---

## 6. 元数据访问

### 6.1 获取磁盘信息

```rust
use vhdx_rs::VhdxFile;

fn print_disk_info(vhdx: &VhdxFile) {
    println!("Virtual Disk ID: {}", vhdx.virtual_disk_id());
    println!("Virtual Disk Size: {} bytes", vhdx.virtual_disk_size());
    println!("Block Size: {} bytes", vhdx.block_size());
    println!("Logical Sector Size: {} bytes", vhdx.logical_sector_size());
    println!("Physical Sector Size: {} bytes", vhdx.physical_sector_size());
    println!("Disk Type: {:?}", vhdx.disk_type());
    
    if let Some(creator) = vhdx.creator() {
        println!("Creator: {}", creator);
    }
}
```

### 6.2 检查父磁盘

```rust
use vhdx_rs::VhdxFile;

fn check_parent(vhdx: &VhdxFile) {
    if vhdx.has_parent() {
        println!("This is a differencing disk");
        // 父磁盘在打开时自动加载
        // 注意：VhdxFile 结构中的 parent 字段未在公共 API 暴露
    }
}
```

---

## 7. 错误处理

### 7.1 错误类型

```rust
use vhdx_rs::VhdxError;

fn error_handling_example() {
    match VhdxFile::open(Path::new("disk.vhdx"), true) {
        Ok(vhdx) => {
            // 使用 vhdx
        }
        Err(VhdxError::InvalidSignature { expected, got }) => {
            eprintln!("Invalid file: expected {}, got {}", expected, got);
        }
        Err(VhdxError::NoValidHeader) => {
            eprintln!("File is corrupted: no valid header found");
        }
        Err(VhdxError::InvalidChecksum) => {
            eprintln!("File is corrupted: checksum mismatch");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
```

### 7.2 结果传播

```rust
use vhdx_rs::{VhdxFile, VhdxError};

fn process_vhdx(path: &Path) -> Result<Vec<u8>, VhdxError> {
    let mut vhdx = VhdxFile::open(path, true)?;
    
    let mut buffer = vec![0u8; 4096];
    vhdx.read(0, &mut buffer)?;
    
    Ok(buffer)
}
```

### 7.3 与标准错误集成

```rust
use vhdx_rs::VhdxError;

fn integrated_error() -> Result<(), Box<dyn std::error::Error>> {
    let vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;
    
    // VhdxError 可以转换为 Box<dyn Error>
    // 也可以与 io::Error 等混合使用
    
    let mut file = std::fs::File::open("data.bin")?;  // io::Error
    let mut vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;  // VhdxError
    
    Ok(())
}
```

---

## 8. 高级用法

### 8.1 批量创建

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

fn create_multiple_disks() -> Result<(), Box<dyn std::error::Error>> {
    let sizes = vec![
        ("small.vhdx", 1 * 1024 * 1024 * 1024u64),   // 1GB
        ("medium.vhdx", 10 * 1024 * 1024 * 1024u64),  // 10GB
        ("large.vhdx", 100 * 1024 * 1024 * 1024u64),   // 100GB
    ];
    
    for (name, size) in sizes {
        VhdxBuilder::new(size)
            .disk_type(DiskType::Dynamic)
            .create(Path::new(name))?;
        println!("Created: {}", name);
    }
    
    Ok(())
}
```

### 8.2 数据复制

```rust
use vhdx_rs::VhdxFile;

fn copy_between_vhdx(
    src_path: &Path,
    dst_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut src = VhdxFile::open(src_path, true)?;
    let mut dst = VhdxFile::open(dst_path, false)?;
    
    let block_size = src.block_size() as usize;
    let total_size = src.virtual_disk_size();
    
    let mut buffer = vec![0u8; block_size];
    let mut offset = 0u64;
    
    while offset < total_size {
        let to_read = (total_size - offset).min(block_size as u64) as usize;
        let bytes_read = src.read(offset, &mut buffer[..to_read])?;
        
        if bytes_read == 0 {
            break;
        }
        
        dst.write(offset, &buffer[..bytes_read])?;
        offset += bytes_read as u64;
    }
    
    println!("Copied {} bytes", offset);
    Ok(())
}
```

### 8.3 自定义块大小计算

```rust
use vhdx_rs::VhdxBuilder;

fn calculate_optimal_block_size(
    virtual_size: u64,
    typical_io_size: u32,
) -> u32 {
    // 对于小随机 I/O：使用较小块（1-8MB）
    // 对于大顺序 I/O：使用较大块（32-256MB）
    
    match typical_io_size {
        0..=65536 => 1 * 1024 * 1024,      // 1MB
        65537..=1048576 => 8 * 1024 * 1024,  // 8MB
        _ => 32 * 1024 * 1024,             // 32MB
    }
}

fn create_optimized() -> Result<(), Box<dyn std::error::Error>> {
    let block_size = calculate_optimal_block_size(
        100 * 1024 * 1024 * 1024,
        4096,  // 假设 4KB 典型 I/O
    );
    
    let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)
        .block_size(block_size)
        .create(Path::new("optimized.vhdx"))?;
    
    Ok(())
}
```

---

## 9. 最佳实践

### 9.1 资源管理

```rust
use vhdx_rs::VhdxFile;

// 使用 RAII 模式
fn process_disk(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // VhdxFile 在作用域结束时自动关闭
    let mut vhdx = VhdxFile::open(path, true)?;
    
    // 处理...
    let mut buffer = vec![0u8; 4096];
    vhdx.read(0, &mut buffer)?;
    
    // 文件自动关闭
    Ok(())
}

// 或使用显式关闭
fn explicit_close(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(path, false)?;
    
    // 写入数据
    vhdx.write(0, b"data")?;
    
    // 确保数据落盘（VhdxFile 在 Drop 时会 flush）
    // 如需更严格控制，可以实现 close 方法
    
    Ok(())
}
```

### 9.2 只读 vs 读写

```rust
use vhdx_rs::VhdxFile;

// 分析场景：使用只读模式
fn analyze_disk(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let vhdx = VhdxFile::open(path, true)?;  // 只读
    
    // 安全：不会意外修改文件
    println!("Size: {}", vhdx.virtual_disk_size());
    
    Ok(())
}

// 修改场景：使用读写模式
fn modify_disk(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(path, false)?;  // 读写
    
    // 可以写入
    vhdx.write(0, b"modified")?;
    
    Ok(())
}
```

### 9.3 缓冲区管理

```rust
use vhdx_rs::VhdxFile;

fn efficient_io(vhdx: &mut VhdxFile) -> Result<(), Box<dyn std::error::Error>> {
    // 重用缓冲区而非重复分配
    let block_size = vhdx.block_size() as usize;
    let mut buffer = vec![0u8; block_size];
    
    for i in 0..100 {
        let offset = i as u64 * block_size as u64;
        vhdx.read(offset, &mut buffer)?;
        // 处理 buffer...
    }
    
    Ok(())
}

// 避免：频繁分配
fn inefficient_io(vhdx: &mut VhdxFile) -> Result<(), Box<dyn std::error::Error>> {
    for i in 0..100 {
        let offset = i as u64 * 4096;
        let mut buffer = vec![0u8; 4096];  // 每次循环都分配
        vhdx.read(offset, &mut buffer)?;
    }
    
    Ok(())
}
```

### 9.4 错误恢复

```rust
use vhdx_rs::{VhdxFile, VhdxError};

fn robust_read(path: &Path, offset: u64, buf: &mut [u8]) -> Result<usize, VhdxError> {
    let mut vhdx = VhdxFile::open(path, true)?;
    
    match vhdx.read(offset, buf) {
        Ok(n) => Ok(n),
        Err(VhdxError::InvalidOffset(_)) => {
            // 偏移越界，调整读取范围
            let size = vhdx.virtual_disk_size();
            if offset >= size {
                Ok(0)  // EOF
            } else {
                let remaining = (size - offset) as usize;
                let to_read = buf.len().min(remaining);
                vhdx.read(offset, &mut buf[..to_read])
            }
        }
        Err(e) => Err(e),
    }
}
```

---

## 10. 完整示例

### 10.1 磁盘分析工具

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn analyze_vhdx(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Analyzing: {}", path.display());
    println!("{:=<50}", "");
    
    let vhdx = VhdxFile::open(path, true)?;
    
    // 基本信息
    println!("Virtual Disk Size: {:.2} GB", 
        vhdx.virtual_disk_size() as f64 / (1024.0 * 1024.0 * 1024.0));
    println!("Block Size: {:.2} MB", 
        vhdx.block_size() as f64 / (1024.0 * 1024.0));
    println!("Disk Type: {:?}", vhdx.disk_type());
    println!("Virtual Disk ID: {}", vhdx.virtual_disk_id());
    
    // 扇区信息
    println!("Logical Sector Size: {} bytes", vhdx.logical_sector_size());
    println!("Physical Sector Size: {} bytes", vhdx.physical_sector_size());
    
    // 创建者
    if let Some(creator) = vhdx.creator() {
        println!("Creator: {}", creator);
    }
    
    // 父磁盘
    if vhdx.has_parent() {
        println!("Parent Disk: Yes");
    }
    
    Ok(())
}

fn main() {
    if let Err(e) = analyze_vhdx(Path::new("disk.vhdx")) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
```

### 10.2 数据验证工具

```rust
use vhdx_rs::VhdxFile;
use std::path::Path;

fn verify_vhdx(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let mut vhdx = VhdxFile::open(path, true)?;
    
    let block_size = vhdx.block_size() as usize;
    let total_blocks = (vhdx.virtual_disk_size() + block_size as u64 - 1) 
        / block_size as u64;
    
    let mut buffer = vec![0u8; block_size];
    let mut errors = 0;
    
    println!("Verifying {} blocks...", total_blocks);
    
    for block_idx in 0..total_blocks {
        let offset = block_idx * block_size as u64;
        match vhdx.read(offset, &mut buffer) {
            Ok(_) => {
                if block_idx % 100 == 0 {
                    print!("\rProgress: {}/{} blocks", block_idx, total_blocks);
                }
            }
            Err(e) => {
                eprintln!("\nError at block {}: {}", block_idx, e);
                errors += 1;
            }
        }
    }
    
    println!("\nVerification complete: {} errors", errors);
    Ok(errors == 0)
}
```

---

## 11. 参考文档

- [01-architecture-overview.md](./01-architecture-overview.md) - 架构概述
- [04-file-operations.md](./04-file-operations.md) - 文件操作详情
- [05-cli-tool.md](./05-cli-tool.md) - CLI 工具使用
- [07-error-handling.md](./07-error-handling.md) - 错误处理
