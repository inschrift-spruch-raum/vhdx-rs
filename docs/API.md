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
│   └── CreateOptions                          # 关联类型：创建选项
│       ├── size(self, u64) -> Self            # 必需：虚拟磁盘大小
│       ├── fixed(self, bool) -> Self          # 可选：固定磁盘
│       ├── has_parent(self, bool) -> Self     # 可选：差分磁盘
│       ├── block_size(self, u32) -> Self      # 可选：块大小
│       └── finish(self) -> Result<File>       # 完成创建
│
├── section::                               # Section模块 - 物理文件结构映射
│   ├── Sections                            # 容器，管理所有sections (懒加载)
│   │   ├── header(&self) -> &Header
│   │   ├── bat(&self) -> &Bat
│   │   ├── metadata(&self) -> &Metadata
│   │   ├── log(&self) -> &Log
│   │   └── blocks(&self) -> &Blocks
│   │
│   ├── Header                              # Header Section (1 MB)
│   │   ├── raw(&self) -> &[u8]             # 完整1MB字节 (FileType + Headers + RegionTables)
│   │   ├── file_type(&self) -> &FileTypeIdentifier
│   │   ├── header(&self, index: usize) -> Option<&HeaderStructure>  # 0=current, 1=header1, 2=header2
│   │   └── region_table(&self, index: usize) -> Option<&RegionTable>  # 0=current, 1=rt1, 2=rt2
│   │
│   │   └── FileTypeIdentifier              # 文件类型标识符
│   │
│   │   └── HeaderStructure                 # VHDX Header
│   │
│   │   └── RegionTable                     # Region Table
│   │       ├── header: RegionTableHeader
│   │       └── entries: Vec<RegionTableEntry>
│   │       │
│   │       └── RegionTableHeader           # Region Table Header
│   │       └── RegionTableEntry            # Region Table Entry
│   │
│   ├── Bat                                 # BAT Section
│   │   ├── raw(&self) -> &[u8]
│   │   ├── entry(&self, index: u64) -> Option<&BatEntry>
│   │   ├── entries(&self) -> &[BatEntry]
│   │   └── len(&self) -> usize
│   │
│   │   └── BatEntry                        # BAT Entry 枚举
│   │       ├── Payload(PayloadEntry)       # Payload Block Entry 变体
│   │       │   └── state(&self) -> PayloadBlockState
│   │       │
│   │       ├── SectorBitmap(SectorBitmapEntry)  # Sector Bitmap Block Entry 变体
│   │       │   └── state(&self) -> SectorBitmapState
│   │       │
│   │       └── file_offset_mb(&self) -> u64   # 通用方法
│   │
│   │       └── PayloadBlockState           # Payload Block 状态枚举
│   │           ├── NotPresent
│   │           ├── Undefined
│   │           ├── Zero
│   │           ├── Unmapped
│   │           ├── FullyPresent
│   │           └── PartiallyPresent
│   │
│   │       └── SectorBitmapState           # Sector Bitmap Block 状态枚举 (差异磁盘)
│   │           ├── NotPresent
│   │           └── Present
│   │
│   ├── Metadata                            # Metadata Section
│   │   ├── raw(&self) -> &[u8]
│   │   ├── table(&self) -> &MetadataTable
│   │   └── items(&self) -> &MetadataItems
│   │
│   │   └── MetadataTable
│   │       ├── header(&self) -> &TableHeader
│   │       ├── entry(&self, item_id: &Guid) -> Option<&TableEntry>
│   │       └── entries(&self) -> &[TableEntry]
│   │
│   │       └── TableHeader
│   │
│   │       └── TableEntry
│   │           └── flags(&self) -> &EntryFlags
│   │
│   │           └── EntryFlags
│   │               ├── is_user(&self) -> bool
│   │               ├── is_virtual_disk(&self) -> bool
│   │               └── is_required(&self) -> bool
│   │
│   │   └── MetadataItems
│   │       ├── file_parameters(&self) -> Option<&FileParameters>
│   │       ├── virtual_disk_size(&self) -> Option<u64>
│   │       ├── virtual_disk_id(&self) -> Option<&Guid>
│   │       ├── logical_sector_size(&self) -> Option<u32>
│   │       ├── physical_sector_size(&self) -> Option<u32>
│   │       └── parent_locator(&self) -> Option<&ParentLocator>
│   │
│   │       └── FileParameters
│   │           ├── block_size(&self) -> u32
│   │           ├── leave_block_allocated(&self) -> bool
│   │           └── has_parent(&self) -> bool
│   │
│   │       └── ParentLocator
│   │           ├── header(&self) -> &LocatorHeader
│   │           ├── entry(&self, index: usize) -> Option<&KeyValueEntry>
│   │           ├── entries(&self) -> &[KeyValueEntry]
│   │           └── key_value_data(&self) -> &[u8]
│   │
│   │           └── LocatorHeader
│   │
│   │           └── KeyValueEntry
│   │               ├── key(&self, data: &[u8]) -> Option<String>
│   │               └── value(&self, data: &[u8]) -> Option<String>
│   │
│   ├── Log                                 # Log Section
│   │   ├── raw(&self) -> &[u8]
│   │   ├── entry(&self, index: usize) -> Option<&Entry>
│   │   └── entries(&self) -> &[Entry]
│   │
│   │   └── Entry                           # Log Entry
│   │       ├── header(&self) -> &LogEntryHeader
│   │       ├── descriptor(&self, index: usize) -> Option<&Descriptor>
│   │       ├── descriptors(&self) -> &[Descriptor]
│   │       └── data(&self) -> &[DataSector]
│   │
│   │       └── Descriptor                  # Descriptor 枚举
│   │           ├── Data(DataDescriptor)    # Data Descriptor 变体
│   │           │
│   │           └── Zero(ZeroDescriptor)    # Zero Descriptor 变体
│   │
│   │           └── DataDescriptor          # Data Descriptor
│   │
│   │           └── ZeroDescriptor          # Zero Descriptor
│   │
│   │       └── LogEntryHeader              # Log Entry Header
│   │
│   │       └── DataSector                  # Data Sector
│   │
│   └── Blocks                              # Blocks Section (Payload & Sector Bitmap)
│       ├── raw(&self) -> &[u8]
│       ├── block(&self, index: u64) -> Option<&Block>
│       └── blocks(&self) -> &[Block]
│       │
│       └── Block                           # Block 枚举
│           ├── Payload(PayloadBlock)       # Payload Block 变体
│           │
│           └── SectorBitmap(SectorBitmapBlock)  # Sector Bitmap Block 变体
│           │
│           └── read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>  # 通用方法
│           │
│           └── PayloadBlock                # Payload Block
│           │
│           └── SectorBitmapBlock           # Sector Bitmap Block
│
├── Guid                                    # GUID 类型
│
└── Error                                   # 错误类型
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
    
    /// 设置是否为固定磁盘（可选，默认 Dynamic）
    pub fn fixed(self, fixed: bool) -> Self;
    
    /// 设置是否为差分磁盘（可选，默认 false）
    pub fn has_parent(self, has_parent: bool) -> Self;
    
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
    
    /// 访问Blocks Section
    /// 懒加载：首次调用时从文件读取Blocks区域元数据（不读取全部数据块）
    pub fn blocks(&self) -> &Blocks;
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
    
    /// 获取Header
    /// - index = 0: 返回 current header（根据 sequence_number 自动选择）
    /// - index = 1: 返回 header 1（物理第一个，偏移 64KB）
    /// - index = 2: 返回 header 2（物理第二个，偏移 128KB）
    /// - index > 2: 返回 None
    pub fn header(&self, index: usize) -> Option<&HeaderStructure>;
    
    /// 获取Region Table
    /// - index = 0: 返回 current header 对应的 region table
    /// - index = 1: 返回 region table 1（偏移 192KB）
    /// - index = 2: 返回 region table 2（偏移 256KB）
    /// - index > 2: 返回 None
    pub fn region_table(&self, index: usize) -> Option<&RegionTable>;
}

/// File Type Identifier (8 bytes signature + 512 bytes creator) (64KB)
#[repr(C, packed)]
pub struct FileTypeIdentifier {
    pub signature: [u8; 8],      // "vhdxfile"
    pub creator: [u8; 512],      // UTF-16, null-terminated
}

/// VHDX Header (4KB)
#[repr(C, packed)]
pub struct HeaderStructure {
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

/// BAT Entry 枚举
/// 区分 Payload Block Entry 和 Sector Bitmap Block Entry
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BatEntry {
    Payload(PayloadEntry),
    SectorBitmap(SectorBitmapEntry),
}

/// Payload Block Entry (64位)
///
/// **注意**：此结构体直接映射BAT Entry的原始64位值（小端序）。
/// Layout: [State(3bits)|Reserved(17bits)|FileOffsetMB(44bits)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct PayloadEntry {
    /// 原始64位BAT Entry值
    pub raw: u64,
}

impl PayloadEntry {
    /// 获取 Payload Block 状态
    pub fn state(&self) -> PayloadBlockState;
}

/// Sector Bitmap Block Entry (64位)
///
/// **注意**：此结构体直接映射BAT Entry的原始64位值（小端序）。
/// Layout: [State(3bits)|Reserved(17bits)|FileOffsetMB(44bits)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct SectorBitmapEntry {
    /// 原始64位BAT Entry值
    pub raw: u64,
}

impl SectorBitmapEntry {
    /// 获取 Sector Bitmap Block 状态
    pub fn state(&self) -> SectorBitmapState;
}

impl BatEntry {
    /// 获取文件偏移（MB为单位）- 通用方法
    pub fn file_offset_mb(&self) -> u64 {
        match self {
            BatEntry::Payload(e) => (e.raw >> 20) & 0xFFFFFFFFFFF,  // 44 bits
            BatEntry::SectorBitmap(e) => (e.raw >> 20) & 0xFFFFFFFFFFF,
        }
    }
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

/// Sector Bitmap Block State (用于差异磁盘)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SectorBitmapState {
    NotPresent = 0,  // 块未分配
    Present = 6,     // 块存在
}
```

### 7. Metadata Section

```rust
/// Metadata Section
/// 
/// 结构：MetadataTable(64KB固定) + MetadataItems(可变大小)
pub struct Metadata;

impl Metadata {
    /// 返回完整的Metadata Section原始字节
    pub fn raw(&self) -> &[u8];
    
    /// 访问Metadata Table
    pub fn table(&self) -> &MetadataTable;
    
    /// 访问Metadata Items
    pub fn items(&self) -> &MetadataItems;
}

/// Metadata Table (64KB固定大小)
pub struct MetadataTable;

impl MetadataTable {
    /// 访问Table Header
    pub fn header(&self) -> &TableHeader;
    
    /// 根据Item ID查找Entry
    pub fn entry(&self, item_id: &Guid) -> Option<&TableEntry>;
    
    /// 获取所有Entries
    pub fn entries(&self) -> &[TableEntry];
}

/// Table Header (32字节)
#[repr(C, packed)]
pub struct TableHeader {
    pub signature: [u8; 8],      // "metadata"
    pub reserved: [u8; 2],
    pub entry_count: u16,
    pub reserved2: [u8; 20],
}

/// Table Entry (32字节)
#[repr(C, packed)]
pub struct TableEntry {
    pub item_id: Guid,
    pub offset: u32,
    pub length: u32,
    pub flags: u32,
    pub reserved: u32,
}

impl TableEntry {
    /// 获取Entry Flags
    pub fn flags(&self) -> &EntryFlags;
}

/// Entry Flags (TableEntry.flags的包装)
pub struct EntryFlags(pub u32);

impl EntryFlags {
    /// 是否为用户元数据 (Bit 31)
    pub fn is_user(&self) -> bool;
    
    /// 是否为虚拟磁盘元数据 (Bit 30)
    pub fn is_virtual_disk(&self) -> bool;
    
    /// 是否为必需项 (Bit 29)
    pub fn is_required(&self) -> bool;
}

/// Metadata Items (64KB之后，变长)
pub struct MetadataItems;

impl MetadataItems {
    /// 获取File Parameters
    pub fn file_parameters(&self) -> Option<&FileParameters>;
    
    /// 获取虚拟磁盘大小
    pub fn virtual_disk_size(&self) -> Option<u64>;
    
    /// 获取虚拟磁盘ID
    pub fn virtual_disk_id(&self) -> Option<&Guid>;
    
    /// 获取逻辑扇区大小
    pub fn logical_sector_size(&self) -> Option<u32>;
    
    /// 获取物理扇区大小
    pub fn physical_sector_size(&self) -> Option<u32>;
    
    /// 获取父定位器（差分磁盘）
    pub fn parent_locator(&self) -> Option<&ParentLocator>;
}

/// File Parameters (8字节)
#[repr(C, packed)]
pub struct FileParameters {
    pub block_size: u32,
    pub flags: u32,
}

impl FileParameters {
    /// 块大小（1MB-256MB，2的幂）
    pub fn block_size(&self) -> u32;
    
    /// 是否保留块分配（固定磁盘）
    pub fn leave_block_allocated(&self) -> bool;
    
    /// 是否有父磁盘（差分磁盘）
    pub fn has_parent(&self) -> bool;
}

/// Parent Locator（差分磁盘，变长结构）
pub struct ParentLocator;

impl ParentLocator {
    /// 访问Locator Header
    pub fn header(&self) -> &LocatorHeader;
    
    /// 根据索引获取Key-Value Entry
    pub fn entry(&self, index: usize) -> Option<&KeyValueEntry>;
    
    /// 获取所有Key-Value Entries
    pub fn entries(&self) -> &[KeyValueEntry];
    
    /// 获取Key-Value数据区域
    pub fn key_value_data(&self) -> &[u8];
}

/// Locator Header (20字节)
#[repr(C, packed)]
pub struct LocatorHeader {
    pub locator_type: Guid,
    pub reserved: u16,
    pub key_value_count: u16,
}

/// Key-Value Entry (12字节)
#[repr(C, packed)]
pub struct KeyValueEntry {
    pub key_offset: u32,
    pub value_offset: u32,
    pub key_length: u16,
    pub value_length: u16,
}

impl KeyValueEntry {
    /// 从key_value_data中获取Key字符串（UTF-16LE解码）
    pub fn key(&self, data: &[u8]) -> Option<String>;
    
    /// 从key_value_data中获取Value字符串（UTF-16LE解码）
    pub fn value(&self, data: &[u8]) -> Option<String>;
}

/// 标准Metadata Item GUID常量
pub mod StandardItems {
    pub const FILE_PARAMETERS: Guid = Guid::from_bytes([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D,
        0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7, 0x6B
    ]); // CAA16737-FA36-4D43-B3B6-33F0AA44E76B
    
    pub const VIRTUAL_DISK_SIZE: Guid = Guid::from_bytes([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48,
        0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4, 0xB8
    ]); // 2FA54224-CD1B-4876-B211-5DBED83BF4B8
    
    pub const VIRTUAL_DISK_ID: Guid = Guid::from_bytes([
        0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45,
        0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7, 0x46
    ]); // BECA12AB-B2E6-4523-93EF-C309E000C746
    
    pub const LOGICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47,
        0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB, 0x5F
    ]); // 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
    
    pub const PHYSICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44,
        0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5, 0x56
    ]); // CDA348C7-445D-4471-9CC9-E9885251C556
    
    pub const PARENT_LOCATOR: Guid = Guid::from_bytes([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45,
        0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB, 0x0C
    ]); // A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
    
    /// VHDX Parent Locator Type GUID
    pub const LOCATOR_TYPE_VHDX: Guid = Guid::from_bytes([
        0xB7, 0xEF, 0x4A, 0xB0, 0x9E, 0xD1, 0x81, 0x4A,
        0xB7, 0x89, 0x25, 0xB8, 0xE9, 0x44, 0x59, 0x13
    ]); // B04AEFB7-D19E-4A81-B789-25B8E9445913
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
    
    /// 根据索引获取Entry
    pub fn entry(&self, index: usize) -> Option<&Entry>;
    
    /// 获取所有Entries
    pub fn entries(&self) -> &[Entry];
}

/// Log Entry（组合结构，包含header、descriptors和sectors）
pub struct Entry;

impl Entry {
    /// 获取Log Entry Header
    pub fn header(&self) -> &LogEntryHeader;
    
    /// 根据索引获取单个Descriptor
    pub fn descriptor(&self, index: usize) -> Option<&Descriptor>;
    
    /// 获取所有Descriptors（按原始顺序）
    pub fn descriptors(&self) -> &[Descriptor];
    
    /// 获取Data Sectors
    pub fn data(&self) -> &[DataSector];
}

/// Descriptor 枚举
pub enum Descriptor {
    Data(DataDescriptor),
    Zero(ZeroDescriptor),
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

/// Data Sector (4KB)
#[repr(C, packed)]
pub struct DataSector {
    pub signature: [u8; 4],      // "data"
    pub sequence_high: u32,
    pub data: [u8; 4084],
    pub sequence_low: u32,
}
```

### 9. Blocks Section

```rust
/// Blocks Section
/// 
/// 包含 Payload Blocks 和 Sector Bitmap Blocks 的数据区域
pub struct Blocks;

impl Blocks {
    /// 返回完整的Blocks区域原始字节
    /// 注意：对于大文件这会非常大，谨慎使用
    pub fn raw(&self) -> &[u8];
    
    /// 获取指定索引的Block
    pub fn block(&self, index: u64) -> Option<&Block>;
    
    /// 获取所有Blocks
    pub fn blocks(&self) -> &[Block];
}

/// Block 枚举
/// 区分 Payload Block 和 Sector Bitmap Block
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Payload(PayloadBlock),
    SectorBitmap(SectorBitmapBlock),
}

/// Payload Block
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadBlock;

/// Sector Bitmap Block（用于差异磁盘）
#[derive(Clone, Debug, PartialEq)]
pub struct SectorBitmapBlock;

impl SectorBitmapBlock {
    /// 检查指定扇位是否存在于当前文件（差异磁盘）
    /// offset: 扇区在虚拟磁盘中的偏移（字节）
    pub fn is_sector_present(&self, sector_index: u64) -> bool;
}

impl Block {
    /// 读取块数据 - 通用方法
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
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

/// Sector Bitmap Block State（用于差异磁盘）
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SectorBitmapState {
    NotPresent = 0,  // 块未分配
    Present = 6,     // 块存在
}
```


## 模块结构

```rust
// lib.rs - 公共 API 导出

// 核心类型
pub use error::{Error, Result};
pub use types::Guid;

// Section 模块
pub mod section {
    pub use sections::Sections;
    pub use header::{Header, FileTypeIdentifier, HeaderStructure, RegionTable, RegionTableHeader, RegionTableEntry};
    pub use bat::{Bat, BatEntry, PayloadEntry, SectorBitmapEntry};
    pub use metadata::Metadata;
    pub use log::{Log, Entry, LogEntryHeader, DataDescriptor, ZeroDescriptor, DataSector};
    pub use blocks::{Blocks, Block, PayloadBlock, SectorBitmapBlock, PayloadBlockState, SectorBitmapState};
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
    mod blocks;
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
    println!("Current Header Seq: {}", header.header(0).unwrap().sequence_number);
    
    // 访问Metadata Section（同时提供raw和parsed访问）
    let metadata = sections.metadata();
    
    // 从 FileParameters 获取磁盘类型和块大小
    if let Some(fp) = metadata.file_parameters() {
        println!("Block Size: {} bytes", fp.block_size);
        println!("Has Parent: {}", fp.has_parent());
        println!("Leave Blocks Allocated: {}", fp.leave_blocks_allocated());
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

### 2. 遍历 BAT

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx")?.finish()?;
    let bat = file.sections().bat();
    
    // 遍历前10个BAT Entries
    for i in 0..10.min(bat.len() as u64) {
        if let Some(entry) = bat.entry(i) {
            match entry {
                BatEntry::Payload(e) => {
                    println!("Block {}: Payload State={:?}, Offset={}MB",
                        i, e.state(), entry.file_offset_mb());
                }
                BatEntry::SectorBitmap(e) => {
                    println!("Block {}: SectorBitmap State={:?}, Offset={}MB",
                        i, e.state(), entry.file_offset_mb());
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

### 3. 创建动态磁盘

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
    if let Some(fp) = metadata.file_parameters() {
        assert_eq!(fp.block_size(), 32 * 1024 * 1024);
        assert!(!fp.has_parent());
        assert!(!fp.leave_block_allocated());  // 动态磁盘
    }
    
    Ok(())
}
```

### 3a. 创建固定磁盘

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
    if let Some(fp) = metadata.file_parameters() {
        assert!(fp.leave_block_allocated());  // 固定磁盘
        assert!(!fp.has_parent());
    }
    
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

### 5. 检查磁盘类型

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("diff.vhdx")?.finish()?;
    let sections = file.sections();
    let metadata = sections.metadata();
    
    if let Some(fp) = metadata.file_parameters() {
        if fp.has_parent() {
            println!("This is a differencing disk");
            println!("Block size: {}", fp.block_size());
            
            if let Some(locator) = metadata.parent_locator() {
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

---

## 文档版本

- **规范**: MS-VHDX v20240423
- **版本**: 3.0
- **更新日期**: 2026
