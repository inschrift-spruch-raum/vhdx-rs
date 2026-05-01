# VHDX Rust 库 API 设计分析

## 基于 MS-VHDX v20240423 规范的导出设计

---

## API 树

```
vhdx::
├── File                                    # 核心 API
│   ├── open(path) -> File::OpenOptions     # 链式打开
│   ├── create(path) -> File::CreateOptions # 链式创建
│   ├── sections(&self) -> &Sections<'_>    # 获取所有sections
│   ├── io(&self) -> IO<'_>                 # 获取IO模块
│   ├── validator(&self) -> validation::SpecValidator<'_>  # 获取规范校验器
│   └── inner(&self) -> &std::fs::File
│
│   └── OpenOptions                         # 关联类型：打开选项
│       ├── write(self) -> Self             # 启用写权限（RW）
│       ├── strict(self, bool) -> Self      # 是否启用严格模式（默认 true，required unknown 始终失败）
│       ├── log_replay(self, LogReplayPolicy) -> Self # 日志回放策略
│       └── finish(self) -> Result<File>    # 完成打开
│
│   └── CreateOptions                          # 关联类型：创建选项
│       ├── size(self, u64) -> Self            # 必需：虚拟磁盘大小
│       ├── fixed(self, bool) -> Self          # 可选：固定磁盘
│       ├── block_size(self, u32) -> Self      # 可选：块大小
│       ├── logical_sector_size(self, u32) -> Self   # 可选：逻辑扇区大小(512/4096)
│       ├── physical_sector_size(self, u32) -> Self  # 可选：物理扇区大小(512/4096)
│       ├── parent_path(self, impl AsRef<Path>) -> Self # 差分盘父路径
│       └── finish(self) -> Result<File>       # 完成创建
│
├── validation::                             # 规范一致性校验模块（只读）
│   ├── SpecValidator<'a>                    # 规范校验器
│   │   ├── validate_file(&self) -> Result<()> # 总入口（Header/Region/BAT/Metadata/Log）
│   │   ├── validate_header(&self) -> Result<()>
│   │   ├── validate_region_table(&self) -> Result<()>
│   │   ├── validate_bat(&self) -> Result<()>
│   │   ├── validate_metadata(&self) -> Result<()>
│   │   ├── validate_required_metadata_items(&self) -> Result<()>
│   │   ├── validate_log(&self) -> Result<()>
│   │   ├── validate_parent_locator(&self) -> Result<()>
│   │   └── validate_parent_chain(&self) -> Result<ParentChainInfo> # 差分链校验
│   └── ValidationIssue                      # 可选：结构化校验问题（用于报告）
│
├── section::                               # Section模块 - 物理文件结构映射
│   ├── Sections<'a>                        # 容器，管理所有sections (懒加载)
│   │   ├── header(&self) -> Result<std::cell::Ref<'_, Header<'a>>>
│   │   ├── bat(&self) -> Result<std::cell::Ref<'_, Bat<'a>>>
│   │   ├── metadata(&self) -> Result<std::cell::Ref<'_, Metadata<'a>>>
│   │   └── log(&self) -> Result<std::cell::Ref<'_, Log<'a>>>
│   │
│   ├── Header<'a>                          # Header Section (1 MB)
│   │   ├── file_type(&self) -> FileTypeIdentifier<'_>
│   │   ├── header(&self, index: usize) -> Option<HeaderStructure<'_>>  # 0=current, 1=header1, 2=header2
│   │   └── region_table(&self, index: usize) -> Option<RegionTable<'_>>  # 0=current, 1=rt1, 2=rt2
│   │
│   │   └── FileTypeIdentifier<'a>          # 文件类型标识符视图
│   │       ├── signature: [u8; 8]
│   │       └── creator: &'a [u8]
│   │
│   │   └── HeaderStructure<'a>             # VHDX Header 视图
│   │       ├── signature: [u8; 4]
│   │       ├── checksum: u32
│   │       ├── sequence_number: u64
│   │       ├── file_write_guid: Guid
│   │       ├── data_write_guid: Guid
│   │       ├── log_guid: Guid
│   │       ├── log_version: u16
│   │       ├── version: u16
│   │       ├── log_length: u32
│   │       └── log_offset: u64
│   │
│   │   └── RegionTable<'a>                 # Region Table 视图
│   │       └── RegionTableHeader<'a>       # Region Table Header 视图
│   │           ├── signature: [u8; 4]
│   │           ├── checksum: u32
│   │           ├── entry_count: u32
│   │           └── reserved: u32
│   │       └── RegionTableEntry<'a>        # Region Table Entry 视图
│   │           ├── guid: Guid
│   │           ├── file_offset: u64
│   │           ├── length: u32
│   │           └── required: u32
│   │
│   ├── Bat<'a>                             # BAT Section
│   │   ├── entry(&self, index: u64) -> Option<BatEntry>
│   │   ├── entries(&self) -> Vec<BatEntry>
│   │   └── len(&self) -> usize
│   │
│   │   └── BatEntry                        # BAT Entry 结构体
│   │       ├── state: BatState
│   │       ├── file_offset_mb: u64
│   │
│   │       └──BatState 枚举:                  # Entry 类型枚举
│   │          ├── Payload(PayloadBlockState)
│   │          └── SectorBitmap(SectorBitmapState)
│   │
│   │          └── PayloadBlockState           # Payload Block 状态枚举
│   │              ├── NotPresent
│   │              ├── Undefined
│   │              ├── Zero
│   │              ├── Unmapped
│   │              ├── FullyPresent
│   │              └── PartiallyPresent
│   │
│   │          └── SectorBitmapState           # Sector Bitmap Block 状态枚举 (差异磁盘)
│   │              ├── NotPresent
│   │              └── Present
│   │
│   ├── Metadata<'a>                        # Metadata Section
│   │   ├── table(&self) -> MetadataTable<'_>
│   │   └── items(&self) -> MetadataItems<'_>
│   │
│   │   └── MetadataTable<'a>
│   │       ├── header(&self) -> TableHeader<'_>
│   │       ├── entry(&self, item_id: &Guid) -> Option<TableEntry<'_>>
│   │       └── entries(&self) -> Vec<TableEntry<'_>>
│   │
│   │       └── TableHeader<'a>
│   │           ├── signature: [u8; 8]
│   │           ├── reserved: [u8; 2]
│   │           ├── entry_count: u16
│   │           └── reserved2: [u8; 20]
│   │
│   │       └── TableEntry<'a>
│   │           ├── item_id: Guid
│   │           ├── offset: u32
│   │           ├── length: u32
│   │           ├── flags: u32
│   │           └── reserved: u32
│   │           └── flags(&self) -> EntryFlags
│   │
│   │           └── EntryFlags
│   │               ├── is_user(&self) -> bool
│   │               ├── is_virtual_disk(&self) -> bool
│   │               └── is_required(&self) -> bool
│   │
│   │   └── MetadataItems<'a>
│   │       ├── file_parameters(&self) -> Option<FileParameters<'_>>
│   │       ├── virtual_disk_size(&self) -> Option<u64>
│   │       ├── virtual_disk_id(&self) -> Option<Guid>
│   │       ├── logical_sector_size(&self) -> Option<u32>
│   │       ├── physical_sector_size(&self) -> Option<u32>
│   │       └── parent_locator(&self) -> Option<ParentLocator<'_>>
│   │
│   │       └── FileParameters<'a>
│   │           ├── block_size(&self) -> u32
│   │           ├── leave_block_allocated(&self) -> bool
│   │           └── has_parent(&self) -> bool
│   │
│   │       └── ParentLocator<'a>
│   │           ├── header(&self) -> LocatorHeader<'_>
│   │           ├── entry(&self, index: usize) -> Option<KeyValueEntry<'_>>
│   │           ├── entries(&self) -> Vec<KeyValueEntry<'_>>
│   │           └── key_value_data(&self) -> &[u8]
│   │           └── resolve_parent_path(&self) -> Option<PathBuf> # 按 relative_path->volume_path->absolute_win32_path 顺序解析
│   │
│   │           └── LocatorHeader<'a>
│   │               ├── locator_type: Guid
│   │               ├── reserved: u16
│   │               └── key_value_count: u16
│   │
│   │           └── KeyValueEntry<'a>
│   │               ├── key_offset: u32
│   │               ├── value_offset: u32
│   │               ├── key_length: u16
│   │               ├── value_length: u16
│   │               ├── key(&self, data: &[u8]) -> Option<String>
│   │               └── value(&self, data: &[u8]) -> Option<String>
│   │
│   └── Log<'a>                             # Log Section
│       ├── entry(&self, index: usize) -> Option<Entry<'_>>
│       └── entries(&self) -> Vec<Entry<'_>>
│    
│       └── Entry<'a>                       # Log Entry
│           ├── header(&self) -> LogEntryHeader<'_>
│           ├── descriptor(&self, index: usize) -> Option<Descriptor<'_>>
│           ├── descriptors(&self) -> Vec<Descriptor<'_>>
│           └── data(&self) -> Vec<DataSector<'_>>
│    
│           └── Descriptor<'a>              # Descriptor 枚举
│               ├── Data(DataDescriptor<'a>)    # Data Descriptor 变体
│               │
│               └── Zero(ZeroDescriptor<'a>)    # Zero Descriptor 变体
│    
│               └── DataDescriptor<'a>      # Data Descriptor
│                   ├── signature: [u8; 4]
│                   ├── trailing_bytes: u32
│                   ├── leading_bytes: u64
│                   ├── file_offset: u64
│                   └── sequence_number: u64
│    
│               └── ZeroDescriptor<'a>      # Zero Descriptor
│                   ├── signature: [u8; 4]
│                   ├── reserved: u32
│                   ├── zero_length: u64
│                   ├── file_offset: u64
│                   └── sequence_number: u64
│    
│           └── LogEntryHeader<'a>          # Log Entry Header
│               ├── signature: [u8; 4]
│               ├── checksum: u32
│               ├── entry_length: u32
│               ├── tail: u32
│               ├── sequence_number: u64
│               ├── descriptor_count: u32
│               ├── reserved: u32
│               ├── log_guid: Guid
│               ├── flushed_file_offset: u64
│               └── last_file_offset: u64
│    
│           └── DataSector<'a>              # Data Sector
│               ├── signature: [u8; 4]
│               ├── sequence_high: u32
│               ├── data: &'a [u8]
│               └── sequence_low: u32
│    
├── IO<'a>                                  # IO模块 (扇区级操作)
│   └── sector(&self, sector: u64) -> Option<Sector<'_>>  # 输入: 全局扇区号
│   │
│   └── Sector<'a>                          # 扇区级定位与操作
│       ├── payload(&self) -> PayloadBlock<'_>
│       ├── read(&self, buf: &mut [u8]) -> Result<usize>
│       └── write(&self, data: &[u8]) -> Result<()>
│
│   └── PayloadBlock<'a>                    # Payload Block 视图
│
├── Guid                                    # GUID 类型
├── LogReplayPolicy                         # 日志回放策略
│   ├── Require                             # 若存在日志则返回 LogReplayRequired
│   ├── Auto                                # 打开阶段自动回放日志
│   ├── InMemoryOnReadOnly                  # 只读场景以内存方式回放
│   └── ReadOnlyNoReplay                    # 只读打开且不回放日志（允许带未回放日志读取元数据）
├── ParentChainInfo                         # 差分链校验结果
│   ├── child: PathBuf                      # 当前子盘路径
│   ├── parent: PathBuf                     # 解析出的父盘路径
│   └── linkage_matched: bool               # 是否匹配 parent_linkage / parent_linkage2
│
└── Error                                   # 错误类型
    ├── Io(std::io::Error)                  # 底层 IO 错误
    ├── FileLocked                          # 文件被其他进程锁定（Windows）
    ├── InvalidFile(String)                 # 无效的 VHDX 文件
    ├── InvalidSignature { expected, found }# 签名不匹配
    ├── CorruptedHeader(String)             # 头部损坏
    ├── InvalidChecksum { expected: u32, actual: u32 }  # CRC32C 校验和不匹配
    ├── InvalidBlockState(u8)               # 无效的 BAT 块状态值
    ├── InvalidRegionTable(String)          # 区域表格式错误
    ├── InvalidMetadata(String)             # 元数据格式错误
    ├── MetadataNotFound { guid: Guid }     # 元数据项未找到
    ├── LogReplayRequired                   # 需要日志回放
    ├── LogEntryCorrupted(String)           # 日志条目损坏
    ├── BatEntryNotFound { index: u64 }     # BAT 条目未找到
    ├── BlockNotPresent { block_idx: u64, state: String }  # 数据块未分配
    ├── SectorOutOfBounds { sector: u64, max: u64 }  # 扇区索引越界
    ├── ParentNotFound { path: PathBuf }    # 父磁盘未找到
    ├── ParentMismatch { expected: Guid, actual: Guid }  # 父磁盘 GUID 不匹配
    ├── InvalidParameter(String)            # 参数无效
    └── ReadOnly                            # 只读模式
```

### CLI 工具树

```
vhdx-tool::
├── info [file]                             # 查看VHDX文件信息
│   └── --format <json|text>                # 输出格式 (默认: text)
│
├── create <path>                           # 创建VHDX文件
│   ├── --size <size>                       # 虚拟磁盘大小 (必需)
│   ├── --type <dynamic|fixed|differencing> # 磁盘类型 (默认: dynamic)
│   ├── --block-size <size>                 # 块大小 (默认: 32MB)
│   ├── --parent <path>                     # 父磁盘路径 (差分磁盘必需)
│   └── --force                             # 覆盖已存在文件
│
├── check [file]                            # 检查文件完整性
│   ├── --repair                            # 尝试修复
│   └── --log-replay                        # 重放日志
│
├── sections [file]                         # 查看内部Sections
│   ├── header                              # 查看Header Section
│   ├── bat                                 # 查看BAT Entries
│   ├── metadata                            # 查看Metadata
│   └── log                                 # 查看Log Entries
│
└── diff [file]                             # 差分磁盘操作
    ├── parent                              # 显示父磁盘路径
    └── chain                               # 显示磁盘链
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
    
    /// 获取所有Section的容器（懒加载）
    pub fn sections(&self) -> &Sections<'_>;
    
    /// 获取IO模块（用于扇区级读写）
    /// 懒加载：内部Sector缓存按需从文件读取
    /// 前置条件：文件无待回放日志，或已按策略完成日志回放
    pub fn io(&self) -> IO<'_>;

    /// 获取规范校验器（只读）
    ///
    /// 说明：校验逻辑被独立到 validation 模块，避免与 File 的打开/创建职责耦合。
    pub fn validator(&self) -> validation::SpecValidator<'_>;
    
    /// 获取底层文件句柄（std::fs::File）
    /// 可用于诊断或结构导出；不得用于虚拟磁盘 payload 数据面读写。
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

    /// 设置严格模式
    ///
    /// strict=true 时启用严格校验。
    /// strict=false 仅放宽 optional unknown，
    /// required unknown（region / metadata）仍必须失败。
    pub fn strict(self, strict: bool) -> Self;

    /// 设置日志回放策略（默认 `LogReplayPolicy::Require`）
    pub fn log_replay(self, policy: LogReplayPolicy) -> Self;
    
    /// 完成打开操作
    ///
    /// 规范约束：
    /// - 若日志非空且策略为 `Require`，必须拒绝打开并返回 `Error::LogReplayRequired`。
    /// - 若策略为 `ReadOnlyNoReplay`，允许只读打开但不回放日志；
    ///   此时仅保证结构读取（Header/Region/Metadata 等），不保证 payload 数据面一致性。
    pub fn finish(self) -> Result<File>;
}
```

```rust
/// 日志回放策略
pub enum LogReplayPolicy {
    /// 若存在日志则返回 LogReplayRequired
    Require,
    /// 打开阶段自动回放日志
    Auto,
    /// 只读场景允许以内存方式回放
    InMemoryOnReadOnly,
    /// 只读打开且不回放日志
    ///
    /// 约束：仅允许结构读取（Header/Region/Metadata 等），
    /// 不保证 payload 数据面的一致性读取。
    ReadOnlyNoReplay,
}

// 默认行为说明：
// File::open(path).finish() 在未显式调用 .log_replay(...) 时，
// 等价于使用 LogReplayPolicy::Require。

/// 差分链校验结果
pub struct ParentChainInfo {
    /// 当前子盘路径
    pub child: PathBuf,
    /// 解析出的父盘路径
    pub parent: PathBuf,
    /// 是否匹配 parent_linkage / parent_linkage2
    pub linkage_matched: bool,
}
```

### 3. File::CreateOptions - 创建选项

```rust
impl File {
    pub struct CreateOptions;
}

impl File::CreateOptions {
    /// 设置虚拟磁盘大小（必需）
    ///
    /// 约束：必须是 logical_sector_size 的整数倍，且 <= 64TB。
    pub fn size(self, virtual_size: u64) -> Self;
    
    /// 设置是否为固定磁盘（可选，默认 Dynamic）
    pub fn fixed(self, fixed: bool) -> Self;
    
    /// 设置块大小（可选，默认 32MB）
    ///
    /// 约束：必须在 [1MB, 256MB] 且为 2 的幂。
    pub fn block_size(self, size: u32) -> Self;

    /// 设置逻辑扇区大小（可选，默认 4096）
    ///
    /// 约束：只能为 512 或 4096。
    pub fn logical_sector_size(self, size: u32) -> Self;

    /// 设置物理扇区大小（可选，默认 4096）
    ///
    /// 约束：只能为 512 或 4096，且必须 >= logical_sector_size。
    pub fn physical_sector_size(self, size: u32) -> Self;

    /// 设置父磁盘路径（设置后即创建差分盘）
    pub fn parent_path(self, path: impl AsRef<Path>) -> Self;
    
    /// 完成创建操作
    ///
    /// 失败条件示例：
    /// - 参数违反规范约束 -> Error::InvalidParameter
    /// - 指定 parent_path 但 Parent Locator 约束不满足 -> Error::ParentNotFound / Error::InvalidFile
    pub fn finish(self) -> Result<File>;
}
```

---

### 3a. validation - 规范一致性校验模块（独立）

```rust
pub mod validation {
    use crate::error::Result;

    /// 规范一致性校验器（只读）
    ///
    /// 职责：将 `validate_spec_compliance` 的规则独立在单一模块中，
    /// 便于按 MS-VHDX 章节维护与测试。
    pub struct SpecValidator<'a>;

    impl<'a> SpecValidator<'a> {
        /// 总入口：执行全部结构校验
        ///
        /// 对应 MS-VHDX 规范章节：
        /// - Layout: §2.1（对齐/非重叠）
        /// - Header/Region: §2.2
        /// - Log: §2.3
        /// - BAT: §2.5
        /// - Metadata: §2.6
        /// - Differencing: 在 has_parent=true 时覆盖 Parent Locator + Parent Chain
        pub fn validate_file(&self) -> Result<()>;

        /// Header Section 校验（签名/CRC/current header/version/log 对齐）
        pub fn validate_header(&self) -> Result<()>;

        /// Region Table 校验（regi/CRC/entry 约束/required unknown 拒绝加载）
        pub fn validate_region_table(&self) -> Result<()>;

        /// BAT 校验（entry 状态合法性与磁盘类型匹配）
        pub fn validate_bat(&self) -> Result<()>;

        /// Metadata 校验（table/entry/已知项约束，不含 required 完整性）
        pub fn validate_metadata(&self) -> Result<()>;

        /// 仅校验 Metadata required item 约束
        ///
        /// 对于 IsRequired=true 但未知/缺失的项，返回错误。
        pub fn validate_required_metadata_items(&self) -> Result<()>;

        /// Log 校验（entry/descriptor/data sector/active sequence/replay 前置）
        pub fn validate_log(&self) -> Result<()>;

        /// 校验 Parent Locator 键约束
        ///
        /// - parent_linkage 必须存在；parent_linkage2 的处理遵循 MS-VHDX §2.6.2.6.3
        /// - relative_path / volume_path / absolute_win32_path 至少存在一个
        pub fn validate_parent_locator(&self) -> Result<()>;

        /// 差分链校验
        ///
        /// 校验 parent_linkage / parent_linkage2 与父盘 DataWriteGuid 的一致性。
        pub fn validate_parent_chain(&self) -> Result<ParentChainInfo>;
    }

    /// 可选：结构化校验问题（用于诊断/报告）
    pub struct ValidationIssue {
        pub section: &'static str,
        pub code: &'static str,
        pub message: String,
        pub spec_ref: &'static str,
    }
}
```

---


### 4. Section 容器

```rust
/// VHDX文件中的所有Section的容器
/// 
/// 采用懒加载策略：访问具体Section时才从文件读取
pub struct Sections<'a> {
    // 内部字段：缓存已加载的sections
}

impl<'a> Sections<'a> {
    /// 访问Header Section
    /// 懒加载：首次调用时从文件读取1MB Header Section
    pub fn header(&self) -> Result<std::cell::Ref<'_, Header<'a>>>;
    
    /// 访问BAT Section
    /// 懒加载：首次调用时从文件读取BAT区域
    pub fn bat(&self) -> Result<std::cell::Ref<'_, Bat<'a>>>;
    
    /// 访问Metadata Section
    /// 懒加载：首次调用时从文件读取Metadata区域
    pub fn metadata(&self) -> Result<std::cell::Ref<'_, Metadata<'a>>>;
    
    /// 访问Log Section
    /// 懒加载：首次调用时从文件读取Log区域
    pub fn log(&self) -> Result<std::cell::Ref<'_, Log<'a>>>;
}
```

### 5. Header Section

```rust
/// Header Section (1 MB固定大小)
/// 
/// 结构：FileTypeIdentifier(64KB) + Header1(4KB) + Header2(4KB) + RegionTable1(64KB) + RegionTable2(64KB) + Reserved
pub struct Header<'a>;

impl<'a> Header<'a> {
    /// 文件类型标识符
    pub fn file_type(&self) -> FileTypeIdentifier<'_>;
    
    /// 获取Header
    /// - index = 0: 返回 current header（根据 sequence_number 自动选择）
    /// - index = 1: 返回 header 1（物理第一个，偏移 64KB）
    /// - index = 2: 返回 header 2（物理第二个，偏移 128KB）
    /// - index > 2: 返回 None
    pub fn header(&self, index: usize) -> Option<HeaderStructure<'_>>;
    
    /// 获取Region Table
    /// - index = 0: 返回 current header 对应的 region table
    /// - index = 1: 返回 region table 1（偏移 192KB）
    /// - index = 2: 返回 region table 2（偏移 256KB）
    /// - index > 2: 返回 None
    pub fn region_table(&self, index: usize) -> Option<RegionTable<'_>>;
}

/// File Type Identifier (8 bytes signature + 512 bytes creator) (64KB)
pub struct FileTypeIdentifier<'a> {
    pub signature: [u8; 8],
    pub creator: &'a [u8],
}

/// VHDX Header 视图（4KB）
pub struct HeaderStructure<'a> {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub sequence_number: u64,
    pub file_write_guid: Guid,
    pub data_write_guid: Guid,
    pub log_guid: Guid,
    pub log_version: u16,
    pub version: u16,
    pub log_length: u32,
    pub log_offset: u64,
    pub raw: &'a [u8],
}

/// Region Table 视图（64KB）
pub struct RegionTable<'a> {
    pub header: RegionTableHeader<'a>,
    pub entries: Vec<RegionTableEntry<'a>>,
}

pub struct RegionTableHeader<'a> {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub entry_count: u32,
    pub reserved: u32,
    pub raw: &'a [u8],
}

pub struct RegionTableEntry<'a> {
    pub guid: Guid,
    pub file_offset: u64,
    pub length: u32,
    pub required: u32,
    pub raw: &'a [u8],
}
```

### 6. BAT Section

```rust
/// BAT (Block Allocation Table) Section
/// 
/// 存储虚拟磁盘块到文件偏移的映射
pub struct Bat<'a>;

impl<'a> Bat<'a> {
    /// 获取指定索引的BAT Entry
    pub fn entry(&self, index: u64) -> Option<BatEntry>;
    
    /// 获取所有BAT Entries（按需解析为视图列表）
    pub fn entries(&self) -> Vec<BatEntry>;
    
    /// BAT Entry数量
    pub fn len(&self) -> usize;
    
    pub fn is_empty(&self) -> bool;
}

/// BAT Entry 结构体
/// 
/// 存储 Payload Block 或 Sector Bitmap Block 的元数据
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatEntry {
    /// Entry 类型和状态
    pub state: BatState,
    /// 文件偏移（MB为单位）
    pub file_offset_mb: u64,
}

/// BAT Entry 类型枚举
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BatState {
    /// Payload Block 状态
    Payload(PayloadBlockState),
    /// Sector Bitmap Block 状态
    SectorBitmap(SectorBitmapState),
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
pub struct Metadata<'a>;

impl<'a> Metadata<'a> {
    /// 访问Metadata Table
    pub fn table(&self) -> MetadataTable<'_>;
    
    /// 访问Metadata Items
    pub fn items(&self) -> MetadataItems<'_>;
}

/// Metadata Table (64KB固定大小)
pub struct MetadataTable<'a>;

impl<'a> MetadataTable<'a> {
    /// 访问Table Header
    pub fn header(&self) -> TableHeader<'_>;
    
    /// 根据Item ID查找Entry
    pub fn entry(&self, item_id: &Guid) -> Option<TableEntry<'_>>;
    
    /// 获取所有Entries（按需解析为视图列表）
    pub fn entries(&self) -> Vec<TableEntry<'_>>;
}

/// Table Header (32字节)
pub struct TableHeader<'a> {
    pub signature: [u8; 8],
    pub reserved: [u8; 2],
    pub entry_count: u16,
    pub reserved2: [u8; 20],
    pub raw: &'a [u8],
}

/// Table Entry (32字节)
pub struct TableEntry<'a> {
    pub item_id: Guid,
    pub offset: u32,
    pub length: u32,
    pub flags: u32,
    pub reserved: u32,
    pub raw: &'a [u8],
}

impl<'a> TableEntry<'a> {
    /// 获取Entry Flags
    pub fn flags(&self) -> EntryFlags;
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
pub struct MetadataItems<'a>;

impl<'a> MetadataItems<'a> {
    /// 获取File Parameters
    pub fn file_parameters(&self) -> Option<FileParameters<'_>>;
    
    /// 获取虚拟磁盘大小
    pub fn virtual_disk_size(&self) -> Option<u64>;
    
    /// 获取虚拟磁盘ID
    pub fn virtual_disk_id(&self) -> Option<Guid>;
    
    /// 获取逻辑扇区大小
    pub fn logical_sector_size(&self) -> Option<u32>;
    
    /// 获取物理扇区大小
    pub fn physical_sector_size(&self) -> Option<u32>;
    
    /// 获取父定位器（差分磁盘）
    pub fn parent_locator(&self) -> Option<ParentLocator<'_>>;

}

/// File Parameters (8字节)
pub struct FileParameters<'a> {
    pub block_size: u32,
    pub flags: u32,
    pub raw: &'a [u8],
}

impl<'a> FileParameters<'a> {
    /// 块大小（1MB-256MB，2的幂）
    pub fn block_size(&self) -> u32;
    
    /// 是否保留块分配（固定磁盘）
    pub fn leave_block_allocated(&self) -> bool;
    
    /// 是否有父磁盘（差分磁盘）
    pub fn has_parent(&self) -> bool;
}

/// Parent Locator（差分磁盘，变长结构）
pub struct ParentLocator<'a>;

impl<'a> ParentLocator<'a> {
    /// 访问Locator Header
    pub fn header(&self) -> LocatorHeader<'_>;
    
    /// 根据索引获取Key-Value Entry
    pub fn entry(&self, index: usize) -> Option<KeyValueEntry<'_>>;
    
    /// 获取所有Key-Value Entries（按需解析为视图列表）
    pub fn entries(&self) -> Vec<KeyValueEntry<'_>>;
    
    /// 获取Key-Value数据区域
    pub fn key_value_data(&self) -> &[u8];

    /// 解析父路径
    ///
    /// 按规范顺序尝试：relative_path -> volume_path -> absolute_win32_path。
    pub fn resolve_parent_path(&self) -> Option<PathBuf>;
}

/// Locator Header (20字节)
pub struct LocatorHeader<'a> {
    pub locator_type: Guid,
    pub reserved: u16,
    pub key_value_count: u16,
    pub raw: &'a [u8],
}

/// Key-Value Entry (12字节)
pub struct KeyValueEntry<'a> {
    pub key_offset: u32,
    pub value_offset: u32,
    pub key_length: u16,
    pub value_length: u16,
    pub raw: &'a [u8],
}

impl<'a> KeyValueEntry<'a> {
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
pub struct Log<'a>;

impl<'a> Log<'a> {
    /// 根据索引获取Entry
    pub fn entry(&self, index: usize) -> Option<Entry<'_>>;
    
    /// 获取所有Entries（按需解析为视图列表）
    pub fn entries(&self) -> Vec<Entry<'_>>;
}

/// Log Entry（组合结构，包含header、descriptors和sectors）
pub struct Entry<'a>;

impl<'a> Entry<'a> {
    /// 获取Log Entry Header
    pub fn header(&self) -> LogEntryHeader<'_>;
    
    /// 根据索引获取单个Descriptor
    pub fn descriptor(&self, index: usize) -> Option<Descriptor<'_>>;
    
    /// 获取所有Descriptors（按原始顺序，按需解析）
    pub fn descriptors(&self) -> Vec<Descriptor<'_>>;
    
    /// 获取Data Sectors（按需解析）
    pub fn data(&self) -> Vec<DataSector<'_>>;
}

/// Descriptor 枚举
pub enum Descriptor<'a> {
    Data(DataDescriptor<'a>),
    Zero(ZeroDescriptor<'a>),
}

/// Data Descriptor (32字节)
pub struct DataDescriptor<'a> {
    pub signature: [u8; 4],
    pub trailing_bytes: u32,
    pub leading_bytes: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
    pub raw: &'a [u8],
}

/// Zero Descriptor (32字节)
pub struct ZeroDescriptor<'a> {
    pub signature: [u8; 4],
    pub reserved: u32,
    pub zero_length: u64,
    pub file_offset: u64,
    pub sequence_number: u64,
    pub raw: &'a [u8],
}

/// Log Entry Header (64字节)
pub struct LogEntryHeader<'a> {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub entry_length: u32,
    pub tail: u32,
    pub sequence_number: u64,
    pub descriptor_count: u32,
    pub reserved: u32,
    pub log_guid: Guid,
    pub flushed_file_offset: u64,
    pub last_file_offset: u64,
    pub raw: &'a [u8],
}

/// Data Sector (4KB)
pub struct DataSector<'a> {
    pub signature: [u8; 4],
    pub sequence_high: u32,
    pub data: &'a [u8],
    pub sequence_low: u32,
    pub raw: &'a [u8],
}
```

### 9. IO

```rust
/// IO模块
/// 
/// 扇区级读写操作
///
/// 【设计约束（强制）】唯一数据平面入口：
/// - File 层不提供 read/write/flush
/// - 所有虚拟磁盘读写必须经由 IO::sector -> Sector::read/write
/// - 禁止在 File 层新增等价的数据读写接口
/// 输入: 全局扇区号 -> 内部自动计算块索引和块内扇区偏移
pub struct IO<'a>;

impl<'a> IO<'a> {
    /// 通过全局扇区号定位并返回Sector
    /// 内部自动: 1) 通过BAT找到对应块 2) 计算块内扇区偏移
    /// 懒加载: Sector缓存按需从文件读取
    pub fn sector(&self, sector: u64) -> Option<Sector<'_>>;
}

/// Sector - 扇区级定位与操作
/// 
/// 封装了PayloadBlock引用和块内扇区索引
#[derive(Clone, Debug, PartialEq)]
pub struct Sector<'a> {
    // 简单类型字段: 块内扇区索引
    pub block_sector_index: u32,
    pub payload: PayloadBlock<'a>,
}

impl<'a> Sector<'a> {
    /// 获取对应的PayloadBlock
    pub fn payload(&self) -> PayloadBlock<'_>;
    
    /// 读取扇区数据
    /// buf长度必须为扇区大小的整数倍
    pub fn read(&self, buf: &mut [u8]) -> Result<usize>;
    
    /// 写入扇区数据
    /// data长度必须为扇区大小的整数倍
    pub fn write(&self, data: &[u8]) -> Result<()>;
}

/// Payload Block - 内部结构
/// 
/// 用户通过Sector访问，不直接操作
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadBlock<'a> {
    pub bytes: &'a [u8],
}
```


## 模块结构

```rust
// lib.rs - 公共 API 导出

// 核心类型
pub use error::{Error, Result};
pub use types::Guid;
pub use file::{LogReplayPolicy, ParentChainInfo};
pub use validation::{SpecValidator, ValidationIssue};

// 规范校验模块
pub mod validation;

// Section 模块
pub mod section {
    pub use sections::Sections;
    pub use header::{Header, FileTypeIdentifier, HeaderStructure, RegionTable, RegionTableHeader, RegionTableEntry};
    pub use bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};
    pub use metadata::{Metadata, MetadataTable, TableHeader, TableEntry, EntryFlags, MetadataItems, FileParameters, ParentLocator, LocatorHeader, KeyValueEntry};
    pub use log::{Log, Entry, LogEntryHeader, DataDescriptor, ZeroDescriptor, DataSector};
}

// IO模块（根级）
pub use io::{IO, Sector, PayloadBlock};

// 主 API
pub use file::File;

// 内部实现 (私有)
mod error;
mod types;
mod common;
mod file;
mod io;
mod sections;
mod header;
mod bat;
mod metadata;
mod log;
```

---

## 使用示例

### 1. 只读打开

```rust
use vhdx::{File, LogReplayPolicy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 只读打开（默认）
    let file = File::open("disk.vhdx")
        .log_replay(LogReplayPolicy::Require)
        .finish()?;

    // 使用独立校验器（推荐）
    file.validator().validate_file()?;
    
    // 获取sections容器
    let sections = file.sections();
    
    // 访问Header Section
    let header = sections.header()?;
    println!("File Type: {:?}", header.file_type().signature);
    println!("Current Header Seq: {}", header.header(0).unwrap().sequence_number);
    
    // 访问Metadata Section（结构化访问）
    let metadata = sections.metadata()?;
    
    // 从 FileParameters 获取磁盘类型和块大小
    if let Some(fp) = metadata.items().file_parameters() {
        println!("Block Size: {} bytes", fp.block_size());
        println!("Has Parent: {}", fp.has_parent());
        println!("Leave Blocks Allocated: {}", fp.leave_block_allocated());
    }
    println!(
        "Virtual Size: {} bytes",
        metadata.items().virtual_disk_size().unwrap_or_default()
    );
    
    // 结构化访问：具体结构
    println!("Metadata Entry count: {}", metadata.table().header().entry_count);
    
    Ok(())
}
```

### 2. 遍历 BAT

```rust
use vhdx::File;
use vhdx::section::BatState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx").finish()?;
    let bat = file.sections().bat()?;
    
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
    
    Ok(())
}
```

### 2a. 使用独立校验器（分项校验）

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx").finish()?;
    let validator = file.validator();

    // 按需分项校验
    validator.validate_header()?;
    validator.validate_region_table()?;
    validator.validate_bat()?;
    validator.validate_metadata()?;
    validator.validate_required_metadata_items()?;
    validator.validate_log()?;

    Ok(())
}
```

### 3. 创建动态磁盘

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 10GB 动态磁盘（默认：非固定、无父磁盘）
    let file = File::create("disk.vhdx")
        .size(10 * 1024 * 1024 * 1024)
        .logical_sector_size(4096)
        .physical_sector_size(4096)
        .block_size(32 * 1024 * 1024)  // 32MB块
        .finish()?;
    
    // 写入数据（通过 IO/Sector 执行扇区写）
    let io = file.io();
    if let Some(sector0) = io.sector(0) {
        let data = vec![0u8; 4096];
        sector0.write(&data)?;
    }
    
    // 验证创建的Metadata
    let metadata = file.sections().metadata()?;
    if let Some(fp) = metadata.items().file_parameters() {
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
    let file = File::create("disk.vhdx")
        .size(10 * 1024 * 1024 * 1024)
        .fixed(true)  // 固定磁盘
        .logical_sector_size(4096)
        .physical_sector_size(4096)
        .block_size(32 * 1024 * 1024)
        .finish()?;
    
    // 验证
    let metadata = file.sections().metadata()?;
    if let Some(fp) = metadata.items().file_parameters() {
        assert!(fp.leave_block_allocated());  // 固定磁盘
        assert!(!fp.has_parent());
    }
    
    Ok(())
}
```

### 4. 导出结构化 Section 信息

```rust
use vhdx::File;
use std::fs::File as StdFile;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("disk.vhdx").finish()?;
    let sections = file.sections();

    // 导出 Header/Metadata 的结构化摘要
    let header = sections.header()?;
    let current_header = header.header(0).unwrap();
    let metadata = sections.metadata()?;

    let summary = format!(
        "seq={}\nlog_length={}\nmetadata_entries={}\n",
        current_header.sequence_number,
        current_header.log_length,
        metadata.table().header().entry_count,
    );

    let mut summary_file = StdFile::create("section_summary.txt")?;
    summary_file.write_all(summary.as_bytes())?;

    println!("Exported structured summary to section_summary.txt");
    
    Ok(())
}
```

### 5. 检查磁盘类型

```rust
use vhdx::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("diff.vhdx")
        .strict(true)
        .finish()?;
    let sections = file.sections();
    let metadata = sections.metadata()?;
    
    if let Some(fp) = metadata.items().file_parameters() {
        if fp.has_parent() {
            println!("This is a differencing disk");
            println!("Block size: {}", fp.block_size());
            
            if let Some(locator) = metadata.items().parent_locator() {
                file.validator().validate_parent_locator()?;
                println!("Parent Locator Entries: {}", locator.header().key_value_count);
                for (i, entry) in locator.entries().iter().enumerate() {
                    let key = entry.key(locator.key_value_data()).unwrap_or_default();
                    let value = entry.value(locator.key_value_data()).unwrap_or_default();
                    println!("  [{}] {}: {}", i, key, value);
                }
                println!("Resolved parent path: {:?}", locator.resolve_parent_path());
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
