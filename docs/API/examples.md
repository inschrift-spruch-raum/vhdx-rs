# Usage Examples

[Back to API Documentation](../API.md)

## Overview

Practical code examples demonstrating API usage.

### 1. Read-Only Open

Open an existing VHDX file in read-only mode and inspect its metadata.

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 只读打开（默认）
    let file = File::open("disk.vhdx")?.finish()?;
    
    // 获取sections容器
    let sections = file.sections();
    
    // 访问Header Section
    let header = sections.header();
    println!("File Type: {:?}", header.file_type().signature);
    println!("Current Header Seq: {}", header.header(0).unwrap().sequence_number);
    
    // 访问Metadata Section（同时提供raw和parsed访问）
    let metadata = sections.metadata();
    
    // 从 FileParameters 获取磁盘类型和块大小
    if let Some(fp) = metadata.items().file_parameters() {
        println!("Block Size: {} bytes", fp.block_size());
        println!("Has Parent: {}", fp.has_parent());
        println!("Leave Blocks Allocated: {}", fp.leave_block_allocated());
    }
    println!("Virtual Size: {} bytes", metadata.virtual_size());
    
    // Raw访问：原始字节
    let raw_metadata = metadata.raw();
    println!("Metadata Section size: {} bytes", raw_metadata.len());
    
    // Raw访问：具体结构
    println!("Metadata Entry count: {}", metadata.table().entry_count);
    
    Ok(())
}
```

### 2. Iterate BAT

Traverse the Block Allocation Table entries.

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    let bat = file.sections().bat();
    
    // 遍历前10个BAT Entries
    for i in 0..10.min(bat.len() as u64) {
        if let Some(entry) = bat.entry(i) {
            match entry.state {
                BatState::Payload(state) => {
                    println!("Block {}: Payload State={:?}, Offset={}MB",
                        i, state, entry.file_offset_mb);
                }
                BatState::SectorBitmap(state) => {
                    println!("Block {}: SectorBitmap State={:?}, Offset={}MB",
                        i, state, entry.file_offset_mb);
                }
            }
        }
    }
    
    // 获取原始BAT字节
    let raw_bat = bat.raw();
    println!("BAT Region size: {} bytes", raw_bat.len());
    
    Ok(())
}
```

### 3. Create Dynamic Disk

Create a new dynamic (sparse) VHDX file.

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 动态磁盘（默认：非固定、无父磁盘）
    let mut file = File::create("disk.vhdx")?
        .size(10 * 1024 * 1024 * 1024)
        .block_size(32 * 1024 * 1024)  // 32MB块
        .finish()?;
    
    // 写入数据（通过File::write，不是直接操作Sections）
    file.write(0, b"Hello, VHDX!")?;
    file.flush()?;
    
    // 验证创建的Metadata
    let metadata = file.sections().metadata();
    if let Some(fp) = metadata.items().file_parameters() {
        assert_eq!(fp.block_size(), 32 * 1024 * 1024);
        assert!(!fp.has_parent());
        assert!(!fp.leave_block_allocated());  // 动态磁盘
    }
    
    Ok(())
}
```

### 4. Create Fixed Disk

Create a new fixed-size VHDX file.

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 固定磁盘
    let mut file = File::create("disk.vhdx")?
        .size(10 * 1024 * 1024 * 1024)
        .fixed(true)  // 固定磁盘
        .block_size(32 * 1024 * 1024)
        .finish()?;
    
    // 验证
    let metadata = file.sections().metadata();
    if let Some(fp) = metadata.items().file_parameters() {
        assert!(fp.leave_block_allocated());  // 固定磁盘
        assert!(!fp.has_parent());
    }
    
    Ok(())
}
```

### 5. Read Raw Section Data

Export raw binary data from VHDX sections.

```rust
use vhdx::File;
use std::fs::File as StdFile;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    let sections = file.sections();
    
    // 导出Header Section原始数据
    let header_raw = sections.header().raw();
    let mut header_file = StdFile::create("header_section.bin")?;
    header_file.write_all(header_raw)?;
    
    // 导出Metadata Section原始数据
    let metadata_raw = sections.metadata().raw();
    let mut metadata_file = StdFile::create("metadata_section.bin")?;
    metadata_file.write_all(metadata_raw)?;
    
    println!("Header Section: {} bytes", header_raw.len());      // 1 MB
    println!("Metadata Section: {} bytes", metadata_raw.len());  // 可变
    
    Ok(())
}
```

### 6. Check Disk Type

Determine if a VHDX file is dynamic, fixed, or differencing.

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("diff.vhdx")?.finish()?;
    let sections = file.sections();
    let metadata = sections.metadata();
    
    if let Some(fp) = metadata.items().file_parameters() {
        if fp.has_parent() {
            println!("This is a differencing disk");
            println!("Block size: {}", fp.block_size());
            
            if let Some(locator) = metadata.items().parent_locator() {
                println!("Parent Locator Entries: {}", locator.header().key_value_count);
                for (i, entry) in locator.entries().iter().enumerate() {
                    let key = entry.key(locator.key_value_data()).unwrap_or_default();
                    let value = entry.value(locator.key_value_data()).unwrap_or_default();
                    println!("  [{}] {}: {}", i, key, value);
                }
            }
        } else if fp.leave_block_allocated() {
            println!("This is a fixed disk");
        } else {
            println!("This is a dynamic disk");
        }
    }
    
    Ok(())
}
```
