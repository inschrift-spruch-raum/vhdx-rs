# VHDX Rust 库 API 设计分析

## 基于 MS-VHDX v20240423 规范的导出设计

---

## API 树

```
vhdx::
├── File                              # 核心 API
│   ├── open(path) -> File::OpenOptions           # 链式打开（只读默认）
│   │   └── finish() -> Result<File>
│   ├── create(path) -> File::CreateOptions       # 链式创建
│   │   └── finish() -> Result<File>
│   ├── read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>
│   ├── write(&mut self, offset: u64, buf: &[u8]) -> Result<()>
│   ├── flush(&mut self) -> Result<()>
│   ├── metadata(&self) -> &Metadata              # 获取元数据访问
│   └── inner(&self) -> &std::fs::File           # 获取底层文件句柄
│
│   └── OpenOptions                   # 关联类型：打开选项
│       ├── write(self) -> Self                   # 启用写权限（RW）
│       └── finish(self) -> Result<File>          # 完成打开
│
│   └── CreateOptions                 # 关联类型：创建选项（原 Builder）
│       ├── size(self, u64) -> Self              # 必需：虚拟磁盘大小
│       ├── disk_type(self, Metadata::DiskType) -> Self  # 可选：磁盘类型
│       ├── block_size(self, u32) -> Self        # 可选：块大小
│       └── finish(self) -> Result<File>         # 完成创建
│
├── Metadata                          # 元数据查询（合并 FileParameters）
│   ├── disk_type(&self) -> Metadata::DiskType           # 原 File::disk_type()
│   ├── virtual_size(&self) -> u64                       # 原 File::virtual_size()
│   ├── virtual_disk_id(&self) -> Guid
│   ├── logical_sector_size(&self) -> u32
│   ├── physical_sector_size(&self) -> u32
│   ├── has_parent(&self) -> bool                        # 原 FileParameters
│   ├── leave_block_allocated(&self) -> bool             # 原 FileParameters
│   ├── block_size(&self) -> u32                         # 原 FileParameters
│   ├── parent_locator(&self) -> Option<Metadata::ParentLocator>
│   └── header(&self, Metadata::HeaderSelect) -> raw::Header  # 参数形式
│
│   └── DiskType                      # 关联类型：磁盘类型
│       ├── Fixed
│       ├── Dynamic
│       └── Differencing
│
│   └── HeaderSelect                  # 关联类型：Header 选择（作为参数）
│       ├── Primary
│       ├── Secondary
│       └── Current
│
│   └── ParentLocator                 # 关联类型：父磁盘定位器
│       ├── locator_type(&self) -> Guid
│       └── entries(&self) -> &HashMap<String, String>
│
├── Guid                              # GUID 类型
│   ├── data1: u32
│   ├── data2: u16
│   ├── data3: u16
│   └── data4: [u8; 8]
│
├── Error                             # 错误类型
│   ├── Io(std::io::Error)
│   ├── InvalidFile(String)
│   ├── CorruptedHeader(String)
│   ├── InvalidChecksum { expected: u32, actual: u32 }
│   ├── UnsupportedVersion(u16)
│   ├── InvalidBlockState(u8)
│   ├── ParentNotFound { path: PathBuf }
│   ├── ParentMismatch { expected: Guid, actual: Guid }
│   ├── LogReplayRequired
│   ├── InvalidParameter(String)
│   ├── MetadataNotFound { guid: Guid }
│   └── ReadOnly
│
└── raw::                             # 原始结构（低级 API）
    │                                   // #[repr(C, packed)] - 仅用于二进制文件格式对齐
    ├── FileTypeIdentifier            // 不是 C FFI 兼容！
    ├── Header                        // 用于 MS-VHDX 规范的字节布局
    ├── RegionTableHeader
    ├── RegionTableEntry
    ├── BatEntry                      // (u64 wrapper)
    │   ├── state(&self) -> PayloadBlockState
    │   ├── file_offset_mb(&self) -> u64
    │   └── is_valid(&self) -> bool
    ├── PayloadBlockState             enum
    ├── SectorBitmapState             enum
    ├── MetadataTableHeader
    ├── MetadataTableEntry
    ├── LogEntryHeader
    ├── DataDescriptor
    ├── ZeroDescriptor
    └── DataSector
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
    pub fn write(&mut self, offset: u64, buf: &[u8]) -> Result<()>;
    
    /// 刷新到磁盘
    pub fn flush(&mut self) -> Result<()>;
    
    /// 获取元数据访问
    pub fn metadata(&self) -> &Metadata;
    
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
    /// 调用后打开的文件支持读写操作
    pub fn write(self) -> Self;
    
    /// 完成打开操作，返回 File
    pub fn finish(self) -> Result<File>;
}
```

### 3. File::CreateOptions - 创建选项（关联类型，原 Builder）

```rust
impl File {
    pub struct CreateOptions;
}

impl File::CreateOptions {
    /// 设置虚拟磁盘大小（必需）
    pub fn size(self, virtual_size: u64) -> Self;
    
    /// 设置磁盘类型（可选，默认 Dynamic）
    pub fn disk_type(self, disk_type: Metadata::DiskType) -> Self;
    
    /// 设置块大小（可选，默认 32MB）
    pub fn block_size(self, size: u32) -> Self;
    
    /// 完成创建操作，返回 File
    pub fn finish(self) -> Result<File>;
}
```

### 4. Metadata - 元数据访问

```rust
pub struct Metadata;

impl Metadata {
    /// 获取磁盘类型（从 file_parameters 派生）
    /// - has_parent = true → Differencing
    /// - leave_block_allocated = true → Fixed
    /// - 否则 → Dynamic
    pub fn disk_type(&self) -> Metadata::DiskType;
    
    /// 获取虚拟磁盘大小
    pub fn virtual_size(&self) -> u64;
    
    /// 获取虚拟磁盘 ID（GUID）
    pub fn virtual_disk_id(&self) -> Guid;
    
    /// 获取逻辑扇区大小（512 或 4096）
    pub fn logical_sector_size(&self) -> u32;
    
    /// 获取物理扇区大小（512 或 4096）
    pub fn physical_sector_size(&self) -> u32;
    
    // 原 FileParameters 字段，现为方法
    
    /// 是否有父磁盘（差异磁盘）
    pub fn has_parent(&self) -> bool;
    
    /// 是否预分配块（固定磁盘）
    pub fn leave_block_allocated(&self) -> bool;
    
    /// 获取块大小
    pub fn block_size(&self) -> u32;
    
    /// 获取父磁盘定位器（差异磁盘）
    pub fn parent_locator(&self) -> Option<Metadata::ParentLocator>;
    
    /// 读取指定 Header
    /// select: Primary, Secondary, 或 Current（自动选择序列号较高的）
    pub fn header(&self, select: Metadata::HeaderSelect) -> raw::Header;
}
```

### 5. Metadata::DiskType - 磁盘类型（关联类型）

```rust
impl Metadata {
    pub enum DiskType {
        Fixed,       // 固定大小，预分配全部空间
        Dynamic,     // 动态扩展，按需分配
        Differencing,// 差异磁盘，基于父磁盘
    }
}

impl Metadata::DiskType {
    /// 是否为稀疏磁盘（Dynamic 或 Differencing）
    pub fn is_sparse(&self) -> bool;
    
    /// 是否有父磁盘
    pub fn has_parent(&self) -> bool;
}
```

### 6. Metadata::HeaderSelect - Header 选择（关联类型）

```rust
impl Metadata {
    pub enum HeaderSelect {
        Primary,   // 第一个 Header
        Secondary, // 第二个 Header
        Current,   // 自动选择 SequenceNumber 较高的
    }
}
```

### 7. Metadata::ParentLocator - 父磁盘定位器（关联类型）

```rust
impl Metadata {
    pub struct ParentLocator;
}

impl Metadata::ParentLocator {
    /// 获取定位器类型 GUID
    pub fn locator_type(&self) -> Guid;
    
    /// 获取键值对映射
    /// 包含 parent_linkage, parent_linkage2, relative_path 等
    pub fn entries(&self) -> &HashMap<String, String>;
}
```

### 8. Error - 错误类型

```rust
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidFile(String),
    CorruptedHeader(String),
    InvalidChecksum { expected: u32, actual: u32 },
    UnsupportedVersion(u16),
    InvalidBlockState(u8),
    ParentNotFound { path: PathBuf },
    ParentMismatch { expected: Guid, actual: Guid },
    LogReplayRequired,
    InvalidParameter(String),
    MetadataNotFound { guid: Guid },
    ReadOnly,
}

impl std::fmt::Display for Error { ... }
impl std::error::Error for Error { ... }

pub type Result<T> = std::result::Result<T, Error>;
```

### 9. GUID 类型

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self;
    pub fn parse_str(s: &str) -> Result<Self>;
    pub fn to_string(&self) -> String;
    pub fn generate() -> Self;
}
```

### 10. raw 模块 - 原始结构

**重要文档标注**：

```rust
/// 原始二进制结构模块
/// 
/// 注意：本模块中的所有 `#[repr(C, packed)]` 标注**仅用于二进制文件格式对齐**，
/// 目的是与 MS-VHDX 规范定义的字节布局完全匹配。
/// 
/// **这不是 C FFI 兼容性标注** - 本库是纯 Rust 库，不提供 C 接口。
/// `#[repr(C)]` 在这里仅指定字段顺序和内存布局，`packed` 禁用对齐填充。
pub mod raw {
    use super::Guid;
    
    /// File Type Identifier (8 bytes signature + 512 bytes creator)
    #[repr(C, packed)]
    pub struct FileTypeIdentifier {
        pub signature: [u8; 8],      // "vhdxfile"
        pub creator: [u8; 512],      // UTF-16, null-terminated
    }
    
    /// VHDX Header (4KB structure)
    #[repr(C, packed)]
    pub struct Header {
        pub signature: [u8; 4],      // "head"
        pub checksum: u32,           // CRC-32C
        pub sequence_number: u64,
        pub file_write_guid: Guid,
        pub data_write_guid: Guid,
        pub log_guid: Guid,
        pub log_version: u16,        // Must be 0
        pub version: u16,            // Must be 1
        pub log_length: u32,
        pub log_offset: u64,
        pub reserved: [u8; 4016],
    }
    
    /// Region Table Header
    #[repr(C, packed)]
    pub struct RegionTableHeader {
        pub signature: [u8; 4],      // "regi"
        pub checksum: u32,
        pub entry_count: u32,
        pub reserved: u32,
    }
    
    /// Region Table Entry
    #[repr(C, packed)]
    pub struct RegionTableEntry {
        pub guid: Guid,
        pub file_offset: u64,
        pub length: u32,
        pub required: u32,
    }
    
    /// BAT Entry (64 bits)
    /// Layout: [State(3bits)|Reserved(17bits)|FileOffsetMB(44bits)]
    pub struct BatEntry(pub u64);
    
    impl BatEntry {
        pub fn state(&self) -> PayloadBlockState;
        pub fn file_offset_mb(&self) -> u64;
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
    
    /// Sector Bitmap Block State
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum SectorBitmapState {
        NotPresent = 0,
        Present = 6,
    }
    
    /// Metadata Table Header
    #[repr(C, packed)]
    pub struct MetadataTableHeader {
        pub signature: [u8; 8],      // "metadata"
        pub reserved: u16,
        pub entry_count: u16,
        pub reserved2: [u8; 20],
    }
    
    /// Metadata Table Entry
    #[repr(C, packed)]
    pub struct MetadataTableEntry {
        pub item_id: Guid,
        pub offset: u32,
        pub length: u32,
        pub flags: u32,              // IsUser | IsVirtualDisk | IsRequired
        pub reserved: u32,
    }
    
    /// Log Entry Header
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
    
    /// Data Descriptor
    #[repr(C, packed)]
    pub struct DataDescriptor {
        pub signature: [u8; 4],      // "desc"
        pub trailing_bytes: u32,
        pub leading_bytes: u64,
        pub file_offset: u64,
        pub sequence_number: u64,
    }
    
    /// Zero Descriptor
    #[repr(C, packed)]
    pub struct ZeroDescriptor {
        pub signature: [u8; 4],      // "zero"
        pub reserved: u32,
        pub zero_length: u64,
        pub file_offset: u64,
        pub sequence_number: u64,
    }
    
    /// Data Sector
    #[repr(C, packed)]
    pub struct DataSector {
        pub signature: [u8; 4],      // "data"
        pub sequence_high: u32,
        pub data: [u8; 4084],
        pub sequence_low: u32,
    }
}
```

---

## 模块结构建议

```rust
// lib.rs - 公共 API 导出

// 核心类型
pub use error::{Error, Result};
pub use types::Guid;

// 主 API
pub use file::File;
pub use metadata::Metadata;

// 低级原始访问
pub mod raw;

// 内部实现 (私有)
mod error;
mod types;
mod file;
mod metadata;
mod header;
mod bat;
mod log;
mod block_io;
mod crc32c;
mod utils;
```

---

## 使用示例

### 1. 只读打开

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 只读打开（默认）
    let file = File::open("disk.vhdx")?.finish()?;
    
    // 获取元数据
    let meta = file.metadata();
    println!("Type: {:?}", meta.disk_type());
    println!("Size: {} bytes", meta.virtual_size());
    
    Ok(())
}
```

### 2. 读写打开

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读写打开
    let mut file = File::open("disk.vhdx")?
        .write()           // 启用写权限
        .finish()?;
    
    // 写入数据
    file.write(0, b"Hello, VHDX!")?;
    file.flush()?;
    
    Ok(())
}
```

### 3. 创建动态磁盘

```rust
use vhdx::{File, Metadata};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 动态磁盘
    let mut file = File::create("disk.vhdx")?
        .size(10 * 1024 * 1024 * 1024)  // 10GB
        .disk_type(Metadata::DiskType::Dynamic)
        .finish()?;
    
    // 写入数据
    file.write(0, b"data")?;
    
    Ok(())
}
```

### 4. 创建固定磁盘

```rust
use vhdx::{File, Metadata};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 1GB 固定磁盘
    let file = File::create("fixed.vhdx")?
        .size(1024 * 1024 * 1024)
        .disk_type(Metadata::DiskType::Fixed)
        .block_size(64 * 1024 * 1024)  // 64MB 块
        .finish()?;
    
    Ok(())
}
```

### 5. 读取 Header

```rust
use vhdx::{File, Metadata};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    let meta = file.metadata();
    
    // 读取当前 Header（自动选择序列号较高的）
    let header = meta.header(Metadata::HeaderSelect::Current);
    println!("Sequence: {}", header.sequence_number);
    println!("File Write GUID: {}", header.file_write_guid);
    
    // 显式读取 Primary Header
    let primary = meta.header(Metadata::HeaderSelect::Primary);
    println!("Primary Version: {}", primary.version);
    
    Ok(())
}
```

### 6. 原始文件句柄访问

```rust
use vhdx::File;
use std::io::{Read, Seek, SeekFrom};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    
    // 获取底层 std::fs::File 句柄
    let inner = file.inner();
    
    // 用户自己处理底层 IO
    inner.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 8];
    inner.read(&mut buf)?;
    
    println!("First 8 bytes: {:?}", buf);
    
    Ok(())
}
```

### 7. 检查差异磁盘父信息

```rust
use vhdx::{File, Metadata};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("diff.vhdx")?.finish()?;
    let meta = file.metadata();
    
    if meta.disk_type() == Metadata::DiskType::Differencing {
        println!("This is a differencing disk");
        
        if let Some(locator) = meta.parent_locator() {
            println!("Locator Type: {}", locator.locator_type());
            
            for (key, value) in locator.entries() {
                println!("  {}: {}", key, value);
            }
        }
    }
    
    Ok(())
}
```

---

## 关键设计决策

### 为什么分层设计?

1. **核心 API** (`File`, `Builder`): 简洁的读写操作，满足大多数需求
2. **元数据访问** (`Metadata`): 查询 VHDX 文件属性

### 1. 命名简化原则

**原则**: crate 名已经是 `vhdx`，内部类型不再需要前缀。

```rust
// ❌ 不好 - 冗余前缀
vhdx::VhdxFile
vhdx::VhdxBuilder
vhdx::VhdxError

// ✅ 好 - 简洁清晰
vhdx::File
vhdx::Error
```

### 2. 关联类型设计

将子类型作为关联类型，避免命名冲突：

```rust
// ❌ 可能造成命名冲突
vhdx::DiskType
vhdx::HeaderSelect

// ✅ 关联类型，命名空间隔离
vhdx::Metadata::DiskType
vhdx::Metadata::HeaderSelect
```

### 3. 链式 API

VHDX 创建和打开涉及多个可选参数，链式 API 提供：
- 清晰的默认行为
- 类型安全的配置
- 可扩展性
- 符合 Rust 惯例

### 4. inner() - 获取底层文件句柄

新设计使用 `inner()` 返回 `&std::fs::File`，语义更清晰：
- `inner` 明确表示"内部持有的标准文件对象"
- 符合 Rust 惯例 (Arc::inner, MutexGuard::inner 等)
- 用户获得标准文件句柄
- 可使用 `std::io` 所有功能
- 完全控制底层 IO
- 更灵活，更少限制

### 5. 元数据集中

所有查询方法集中在 `Metadata`：
- 逻辑清晰："查询" vs "操作"
- 避免 `File` 接口膨胀
- 便于缓存和延迟加载
- 符合单一职责原则

### 6. #[repr(C, packed)] 文档

**重要说明**：

```rust
/// 注意：#[repr(C, packed)] 仅用于二进制文件格式对齐，
/// 目的是与 MS-VHDX 规范的字节布局匹配。
/// 这不是 C FFI 兼容性标注 - 本库是纯 Rust 库。
```

理由：
- VHDX 是二进制文件格式
- 需要精确控制内存布局（Little-Endian, 无填充）
- `#[repr(C)]` 指定字段顺序
- `packed` 禁用对齐填充
- 与 C FFI 无关

---

## 总结

这个设计提供了：

1. **简洁命名**: `vhdx::File`, `vhdx::Error` - 符合 Rust 惯例，无冗余前缀
2. **分层 API**: 
   - 核心操作 (`File`)
   - 元数据查询 (`Metadata` 及其关联类型)
   - 原始结构访问 (`raw::*`)
3. **链式 API**: `File::open().write().finish()`, `File::create().size().finish()`
4. **内部句柄访问**: `File::inner()` 返回 `&std::fs::File`，语义清晰，符合 Rust 惯例
5. **类型安全**: 利用 Rust 的类型系统和关联类型
6. **符合规范**: 忠实映射 MS-VHDX 规范的结构和约束
7. **专业定位**: 面向虚拟化/存储开发者，无 "普通用户" 抽象层
8. **CLI 独立**: 命令行工具是独立项目，不在库中导出
9. **文档清晰**: 明确标注 `#[repr(C, packed)]` 用途

### 用户使用示例

```rust
// Cargo.toml: vhdx = "0.1"

use vhdx::{File, Metadata};

// 创建动态磁盘
let mut file = File::create("disk.vhdx")?
    .size(10 * 1024 * 1024 * 1024)
    .disk_type(Metadata::DiskType::Dynamic)
    .finish()?;

// 读写
file.write(0, b"data")?;

// 查询元数据
let meta = file.metadata();
println!("Type: {:?}", meta.disk_type());
println!("Size: {}", meta.virtual_size());

// 获取底层 std::fs::File 句柄
let inner = file.inner();
```

---

## 文档版本

- **规范**: MS-VHDX v20240423
- **版本**: 2.0
- **更新日期**: 2026
