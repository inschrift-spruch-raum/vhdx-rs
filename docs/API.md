# VHDX Rust 库 API 设计分析

## 基于 MS-VHDX v20240423 规范的导出设计

---

## API 树

```
vhdx::
├── File                                    # 核心 API
│   ├── open(path) -> File::OpenOptions     # 链式打开
│   ├── create(path) -> File::CreateOptions # 链式创建
│   ├── read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>
│   ├── write(&mut self, offset: u64, buf: &[u8]) -> Result<()>
│   ├── flush(&mut self) -> Result<()>
│   ├── sections(&self) -> &Sections        # 获取所有sections
│   └── inner(&self) -> &std::fs::File
│
│   └── OpenOptions                         # 关联类型：打开选项
│       ├── write(self) -> Self             # 启用写权限（RW）
│       └── finish(self) -> Result<File>    # 完成打开
│
│   └── CreateOptions                 # 关联类型：创建选项（原 Builder）
│       ├── size(self, u64) -> Self              # 必需：虚拟磁盘大小
│       ├── disk_type(self, DiskType) -> Self  # 可选：磁盘类型
│       ├── block_size(self, u32) -> Self        # 可选：块大小
│       └── finish(self) -> Result<File>         # 完成创建
│
├── section::                          # Section模块 - 物理文件结构映射
│   ├── Sections                       # 容器，管理所有sections (懒加载)
│   │   ├── header(&self) -> &Header
│   │   ├── bat(&self) -> &Bat
│   │   ├── metadata(&self) -> &Metadata
│   │   ├── log(&self) -> &Log
│   │   └── payload(&self) -> &Payload
│   │
│   ├── Header                         # Header Section (1 MB)
│   │   ├── raw(&self) -> &[u8]        # 完整1MB字节 (FileType + Headers + RegionTables)
│   │   ├── file_type(&self) -> &FileTypeIdentifier
│   │   ├── header_1(&self) -> &HeaderStructure
│   │   ├── header_2(&self) -> &HeaderStructure
│   │   ├── current_header(&self) -> &HeaderStructure
│   │   ├── region_table_1(&self) -> &RegionTable
│   │   └── region_table_2(&self) -> &RegionTable
│   │
│   ├── Bat                            # BAT Section
│   │   ├── raw(&self) -> &[u8]        # 完整BAT区域字节
│   │   ├── entry(&self, index: u64) -> Option<BatEntry>
│   │   ├── entries(&self) -> &[BatEntry]
│   │   └── len(&self) -> usize
│   │
│   ├── Metadata                       # Metadata Section (合并原metadata+raw)
│   │   ├── raw(&self) -> &[u8]        # 完整Metadata区域字节 (Table + Items)
│   │   ├── table(&self) -> &MetadataTable
│   │   ├── entries(&self) -> &[MetadataTableEntry]
│   │   ├── item(&self, guid: Guid) -> Option<&[u8]>
│   │   │
│   │   ├── disk_type(&self) -> DiskType           # 解析后的值
│   │   ├── virtual_size(&self) -> u64
│   │   ├── virtual_disk_id(&self) -> Guid
│   │   ├── logical_sector_size(&self) -> u32
│   │   ├── physical_sector_size(&self) -> u32
│   │   ├── block_size(&self) -> u32
│   │   ├── has_parent(&self) -> bool
│   │   ├── leave_block_allocated(&self) -> bool
│   │   └── parent_locator(&self) -> Option<ParentLocator>
│   │
│   ├── Log                            # Log Section
│   │   ├── raw(&self) -> &[u8]        # 完整Log区域字节
│   │   ├── entries(&self) -> LogEntryIter
│   │   └── is_empty(&self) -> bool
│   │
│   └── Payload                        # Payload Section
│       ├── raw(&self) -> &[u8]        # 完整Payload区域字节 (通常很大)
│       ├── block(&self, index: u64) -> Option<Block>
│       └── block_count(&self) -> u64
│
├── DiskType                           # 磁盘类型枚举
│   ├── Fixed
│   ├── Dynamic
│   └── Differencing
│
├── Guid                               # GUID 类型
│   ├── data1: u32
│   ├── data2: u16
│   ├── data3: u16
│   └── data4: [u8; 8]
│
└── Error                              # 错误类型
    ├── Io(std::io::Error)
    ├── InvalidFile(String)
    ├── CorruptedHeader(String)
    ├── InvalidChecksum { expected: u32, actual: u32 }
    ├── UnsupportedVersion(u16)
    ├── InvalidBlockState(u8)
    ├── ParentNotFound { path: PathBuf }
    ├── ParentMismatch { expected: Guid, actual: Guid }
    ├── LogReplayRequired
    ├── InvalidParameter(String)
    ├── MetadataNotFound { guid: Guid }
    └── ReadOnly
```

---

## 详细 API 设计

### 1. File - 核心 API

```rust
pub struct File;

impl File {
    /// 打开现有 VHDX 文件（只读默认）
    /// 返回 OpenOptions 用于链式配置
    pub fn open(path: impl AsRef<Path>) -> File::OpenOptions;
    
    /// 创建新 VHDX 文件
    /// 返回 CreateOptions 用于链式配置
    pub fn create(path: impl AsRef<Path>) -> File::CreateOptions;
    
    /// 读取数据到缓冲区
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    
    /// 写入数据
    /// 注意：只能写入Payload Blocks，不能修改Section结构
    pub fn write(&mut self, offset: u64, buf: &[u8]) -> Result<()>;
    
    /// 刷新到磁盘
    pub fn flush(&mut self) -> Result<()>;
    
    /// 获取所有Section的容器（懒加载）
    pub fn sections(&self) -> &Sections;
    
    /// 获取底层文件句柄（std::fs::File）
    /// 用户可通过此句柄直接进行底层 IO 操作
    pub fn inner(&self) -> &std::fs::File;
}
```

### 2. File::OpenOptions - 打开选项（关联类型）

```rust
impl File {
    pub struct OpenOptions;
}

impl File::OpenOptions {
    /// 启用写权限（默认为只读）
    pub fn write(self) -> Self;
    
    /// 完成打开操作
    pub fn finish(self) -> Result<File>;
}
```

### 3. File::CreateOptions - 创建选项

```rust
impl File {
    pub struct CreateOptions;
}

impl File::CreateOptions {
    /// 设置虚拟磁盘大小（必需）
    pub fn size(self, virtual_size: u64) -> Self;
    
    /// 设置磁盘类型（可选，默认 Dynamic）
    pub fn disk_type(self, disk_type: DiskType) -> Self;
    
    /// 设置块大小（可选，默认 32MB）
    pub fn block_size(self, size: u32) -> Self;
    
    /// 完成创建操作
    pub fn finish(self) -> Result<File>;
}
```

---


### 4. Section 容器

```rust
/// VHDX文件中的所有Section的容器
/// 
/// 采用懒加载策略：访问具体Section时才从文件读取
pub struct Sections {
    // 内部字段：缓存已加载的sections
}

impl Sections {
    /// 访问Header Section
    /// 懒加载：首次调用时从文件读取1MB Header Section
    pub fn header(&self) -> &Header;
    
    /// 访问BAT Section
    /// 懒加载：首次调用时从文件读取BAT区域
    pub fn bat(&self) -> &Bat;
    
    /// 访问Metadata Section
    /// 懒加载：首次调用时从文件读取Metadata区域
    pub fn metadata(&self) -> &Metadata;
    
    /// 访问Log Section
    /// 懒加载：首次调用时从文件读取Log区域
    pub fn log(&self) -> &Log;
    
    /// 访问Payload Section
    /// 懒加载：首次调用时从文件读取Payload区域元数据（不读取全部数据块）
    pub fn payload(&self) -> &Payload;
}
```

### 5. Header Section

```rust
/// Header Section (1 MB固定大小)
/// 
/// 结构：FileTypeIdentifier(64KB) + Header1(4KB) + Header2(4KB) + RegionTable1(64KB) + RegionTable2(64KB) + Reserved
pub struct Header;

impl Header {
    /// 返回完整的1MB Header Section原始字节
    /// 段序：FileType(64KB) | Header1(4KB) | Header2(4KB) | RegionTable1(64KB) | RegionTable2(64KB) | Reserved
    pub fn raw(&self) -> &[u8];
    
    /// 文件类型标识符
    pub fn file_type(&self) -> &FileTypeIdentifier;
    
    /// 第一个Header (偏移64KB)
    pub fn header_1(&self) -> &HeaderStructure;
    
    /// 第二个Header (偏移128KB)
    pub fn header_2(&self) -> &HeaderStructure;
    
    /// 当前Header（根据SequenceNumber自动选择）
    pub fn current_header(&self) -> &HeaderStructure;
    
    /// 第一个Region Table (偏移192KB)
    pub fn region_table_1(&self) -> &RegionTable;
    
    /// 第二个Region Table (偏移256KB)
    pub fn region_table_2(&self) -> &RegionTable;
}

/// File Type Identifier (64KB)
#[repr(C, packed)]
pub struct FileTypeIdentifier {
    pub signature: [u8; 8],      // "vhdxfile"
    pub creator: [u8; 512],      // UTF-16, null-terminated
}

/// VHDX Header (4KB)
#[repr(C, packed)]
pub struct HeaderStructure {
    pub signature: [u8; 4],      // "head"
    pub checksum: u32,
    pub sequence_number: u64,
    pub file_write_guid: Guid,
    pub data_write_guid: Guid,
    pub log_guid: Guid,
    pub log_version: u16,        // Must be 0
    pub version: u16,            // Must be 1
    pub log_length: u32,
    pub log_offset: u64,
    // ... Reserved填充至4KB
}

/// Region Table (64KB)
pub struct RegionTable {
    pub header: RegionTableHeader,
    pub entries: Vec<RegionTableEntry>,
}

#[repr(C, packed)]
pub struct RegionTableHeader {
    pub signature: [u8; 4],      // "regi"
    pub checksum: u32,
    pub entry_count: u32,
    pub reserved: u32,
}

#[repr(C, packed)]
pub struct RegionTableEntry {
    pub guid: Guid,
    pub file_offset: u64,
    pub length: u32,
    pub required: u32,
}
```

### 6. BAT Section

```rust
/// BAT (Block Allocation Table) Section
/// 
/// 存储虚拟磁盘块到文件偏移的映射
pub struct Bat;

impl Bat {
    /// 返回完整的BAT区域原始字节
    pub fn raw(&self) -> &[u8];
    
    /// 获取指定索引的BAT Entry
    pub fn entry(&self, index: u64) -> Option<BatEntry>;
    
    /// 获取所有BAT Entries
    pub fn entries(&self) -> &[BatEntry];
    
    /// BAT Entry数量
    pub fn len(&self) -> usize;
    
    pub fn is_empty(&self) -> bool;
}

/// BAT Entry (64位)
/// Layout: [State(3bits)|Reserved(17bits)|FileOffsetMB(44bits)]
#[repr(C, packed)]
pub struct BatEntry(pub u64);

impl BatEntry {
    /// 获取块状态
    pub fn state(&self) -> PayloadBlockState;
    
    /// 获取文件偏移（MB为单位）
    pub fn file_offset_mb(&self) -> u64;
    
    /// 获取实际文件偏移（字节）
    pub fn file_offset(&self) -> u64 {
        self.file_offset_mb() * 1024 * 1024
    }
    
    /// 验证Entry有效性
    pub fn is_valid(&self) -> bool;
}

/// Payload Block State
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PayloadBlockState {
    NotPresent = 0,
    Undefined = 1,
    Zero = 2,
    Unmapped = 3,
    FullyPresent = 6,
    PartiallyPresent = 7,
}
```

### 7. Metadata Section (合并设计)

```rust
/// Metadata Section
/// 
/// 结构：MetadataTable(64KB固定) + MetadataItems(可变大小)
/// 合并了原metadata模块(高阶API)和raw模块(低阶结构)
pub struct Metadata {
    // 内部缓存原始字节和解析后的值
}

impl Metadata {
    // ===== Raw 访问 (低阶API) =====
    
    /// 返回完整的Metadata Section原始字节
    /// 段序：MetadataTable(64KB) | MetadataItems(按Entry.Offset排列)
    pub fn raw(&self) -> &[u8];
    
    /// Metadata Table (64KB固定大小)
    pub fn table(&self) -> &MetadataTable;
    
    /// Metadata Table Entries
    pub fn entries(&self) -> &[MetadataTableEntry];
    
    /// 获取指定GUID的原始Metadata Item数据
    pub fn item(&self, guid: Guid) -> Option<&[u8]>;
    
    // ===== Parsed 访问 (高阶API) =====
    
    /// 磁盘类型（从FileParameters派生）
    /// - has_parent = true → Differencing
    /// - leave_block_allocated = true → Fixed
    /// - 否则 → Dynamic
    pub fn disk_type(&self) -> DiskType;
    
    /// 虚拟磁盘大小（字节）
    pub fn virtual_size(&self) -> u64;
    
    /// 虚拟磁盘ID（GUID）
    pub fn virtual_disk_id(&self) -> Guid;
    
    /// 逻辑扇区大小（512 或 4096）
    pub fn logical_sector_size(&self) -> u32;
    
    /// 物理扇区大小（512 或 4096）
    pub fn physical_sector_size(&self) -> u32;
    
    /// 块大小（字节）
    pub fn block_size(&self) -> u32;
    
    /// 是否有父磁盘（差异磁盘）
    pub fn has_parent(&self) -> bool;
    
    /// 是否预分配块（固定磁盘特性）
    pub fn leave_block_allocated(&self) -> bool;
    
    /// 父磁盘定位器（差异磁盘）
    pub fn parent_locator(&self) -> Option<ParentLocator>;
}

/// Metadata Table (64KB固定)
#[repr(C, packed)]
pub struct MetadataTable {
    pub signature: [u8; 8],      // "metadata"
    pub reserved: u16,
    pub entry_count: u16,
    pub reserved2: [u8; 20],
    // 之后紧跟着entry_count个MetadataTableEntry
}

/// Metadata Table Entry (32字节)
#[repr(C, packed)]
pub struct MetadataTableEntry {
    pub item_id: Guid,           // Item GUID
    pub offset: u32,             // 相对于Metadata Section起点的偏移
    pub length: u32,             // Item长度
    pub flags: u32,              // IsUser | IsVirtualDisk | IsRequired
    pub reserved: u32,
}

/// 磁盘类型
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiskType {
    Fixed,        // 固定大小，预分配全部空间
    Dynamic,      // 动态扩展，按需分配
    Differencing, // 差异磁盘，基于父磁盘
}

impl DiskType {
    /// 是否为稀疏磁盘（Dynamic 或 Differencing）
    pub fn is_sparse(&self) -> bool;
    
    /// 是否有父磁盘
    pub fn has_parent(&self) -> bool;
}

/// 父磁盘定位器
pub struct ParentLocator {
    pub locator_type: Guid,
    pub entries: HashMap<String, String>,
}
```

### 8. Log Section

```rust
/// Log Section
/// 
/// 环形缓冲区，用于崩溃恢复
pub struct Log;

impl Log {
    /// 返回完整的Log区域原始字节
    pub fn raw(&self) -> &[u8];
    
    /// 遍历Log Entries
    pub fn entries(&self) -> LogEntryIter;
    
    /// Log是否为空
    pub fn is_empty(&self) -> bool;
    
    /// 是否需要重放
    pub fn replay_required(&self) -> bool;
}

/// Log Entry Header (64字节)
#[repr(C, packed)]
pub struct LogEntryHeader {
    pub signature: [u8; 4],      // "loge"
    pub checksum: u32,
    pub entry_length: u32,
    pub tail: u32,
    pub sequence_number: u64,
    pub descriptor_count: u32,
    pub reserved: u32,
    pub log_guid: Guid,
    pub flushed_file_offset: u64,
    pub last_file_offset: u64,
}

/// Data Descriptor (32字节)
#[repr(C, packed)]
pub struct DataDescriptor {
    pub signature: [u8; 4],      // "desc"
    pub trailing_bytes: u32,
    pub leading_bytes: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
}

/// Zero Descriptor (32字节)
#[repr(C, packed)]
pub struct ZeroDescriptor {
    pub signature: [u8; 4],      // "zero"
    pub reserved: u32,
    pub zero_length: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
}

/// Data Sector (4KB)
#[repr(C, packed)]
pub struct DataSector {
    pub signature: [u8; 4],      // "data"
    pub sequence_high: u32,
    pub data: [u8; 4084],
    pub sequence_low: u32,
}
```

### 9. Payload Section

```rust
/// Payload Section
/// 
/// 实际的虚拟磁盘数据块
pub struct Payload;

impl Payload {
    /// 返回完整的Payload区域原始字节
    /// 注意：对于大文件这会非常大，谨慎使用
    pub fn raw(&self) -> &[u8];
    
    /// 获取指定索引的数据块
    pub fn block(&self, index: u64) -> Option<Block>;
    
    /// 数据块总数
    pub fn block_count(&self) -> u64;
    
    /// 块大小（字节）
    pub fn block_size(&self) -> u32;
}

/// 数据块
pub struct Block;

impl Block {
    /// 块状态
    pub fn state(&self) -> PayloadBlockState;
    
    /// 读取块数据
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    
    /// 是否为Sector Bitmap块
    pub fn is_sector_bitmap(&self) -> bool;
}
```


## 模块结构

```rust
// lib.rs - 公共 API 导出

// 核心类型
pub use error::{Error, Result};
pub use types::Guid;
pub use common::DiskType;

// Section 模块
pub mod section {
    pub use sections::Sections;
    pub use header::{Header, FileTypeIdentifier, HeaderStructure, RegionTable, RegionTableHeader, RegionTableEntry};
    pub use bat::{Bat, BatEntry, PayloadBlockState};
    pub use metadata::{Metadata, MetadataTable, MetadataTableEntry, ParentLocator};
    pub use log::{Log, LogEntryHeader, DataDescriptor, ZeroDescriptor, DataSector};
    pub use payload::{Payload, Block};
}

// 主 API
pub use file::File;

// 内部实现 (私有)
mod error;
mod types;
mod common;
mod file;
mod section {
    mod sections;
    mod header;
    mod bat;
    mod metadata;
    mod log;
    mod payload;
}
```

---

## 使用示例

### 1. 只读打开

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
    println!("Current Header Seq: {}", header.current_header().sequence_number);
    
    // 访问Metadata Section（同时提供raw和parsed访问）
    let metadata = sections.metadata();
    
    // Parsed访问：便捷方法
    println!("Disk Type: {:?}", metadata.disk_type());
    println!("Virtual Size: {} bytes", metadata.virtual_size());
    println!("Block Size: {} bytes", metadata.block_size());
    
    // Raw访问：原始字节
    let raw_metadata = metadata.raw();
    println!("Metadata Section size: {} bytes", raw_metadata.len());
    
    // Raw访问：具体结构
    println!("Metadata Entry count: {}", metadata.table().entry_count);
    
    Ok(())
}
```

### 2. 遍历 BAT

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    let bat = file.sections().bat();
    
    // 遍历前10个BAT Entries
    for i in 0..10.min(bat.len() as u64) {
        if let Some(entry) = bat.entry(i) {
            println!("Block {}: State={:?}, Offset={}MB",
                i, entry.state(), entry.file_offset_mb());
        }
    }
    
    // 获取原始BAT字节
    let raw_bat = bat.raw();
    println!("BAT Region size: {} bytes", raw_bat.len());
    
    Ok(())
}
```

### 3. 创建动态磁盘

```rust
use vhdx::{File, DiskType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 动态磁盘
    let mut file = File::create("disk.vhdx")?
        .size(10 * 1024 * 1024 * 1024)
        .disk_type(DiskType::Dynamic)
        .block_size(32 * 1024 * 1024)  // 32MB块
        .finish()?;
    
    // 写入数据（通过File::write，不是直接操作Sections）
    file.write(0, b"Hello, VHDX!")?;
    file.flush()?;
    
    // 验证创建的Metadata
    let metadata = file.metadata();
    assert_eq!(metadata.disk_type(), DiskType::Dynamic);
    assert_eq!(metadata.block_size(), 32 * 1024 * 1024);
    
    Ok(())
}
```

### 4. 读取原始 Section 数据

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

### 5. 检查差异磁盘

```rust
use vhdx::{File, section::DiskType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("diff.vhdx")?.finish()?;
    let metadata = file.metadata();
    
    if metadata.disk_type() == DiskType::Differencing {
        println!("This is a differencing disk");
        println!("Has parent: {}", metadata.has_parent());
        
        if let Some(locator) = metadata.parent_locator() {
            println!("Parent Locator Type: {}", locator.locator_type);
            for (key, value) in &locator.entries {
                println!("  {}: {}", key, value);
            }
        }
    }
    
    Ok(())
}
```

---

## 文档版本

- **规范**: MS-VHDX v20240423
- **版本**: 3.0
- **更新日期**: 2026
