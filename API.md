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
│   │   ├── header(&self, index: usize) -> Result<HeaderStructure<'_>>   # 0=current, 1=header1, 2=header2
│   │   └── region_table(&self, index: usize) -> Result<RegionTable<'_>> # 0=current, 1=rt1, 2=rt2
│   │
│   │   └── FileTypeIdentifier<'a>          # 文件类型标识符视图
│   │       ├── signature(&self) -> &'a [u8; 8]
│   │       └── creator(&self) -> &'a [u8]
│   │
│   │   └── HeaderStructure<'a>             # VHDX Header 视图
│   │       ├── signature(&self) -> &'a [u8; 4]
│   │       ├── checksum(&self) -> u32
│   │       ├── sequence_number(&self) -> u64
│   │       ├── file_write_guid(&self) -> Guid
│   │       ├── data_write_guid(&self) -> Guid
│   │       ├── log_guid(&self) -> Guid
│   │       ├── log_version(&self) -> u16
│   │       ├── version(&self) -> u16
│   │       ├── log_length(&self) -> u32
│   │       └── log_offset(&self) -> u64
│   │
│   │   └── RegionTable<'a>                 # Region Table 视图
│   │       └── RegionTableHeader<'a>       # Region Table Header 视图
│   │           ├── signature(&self) -> &'a [u8; 4]
│   │           ├── checksum(&self) -> u32
│   │           ├── entry_count(&self) -> u32
│   │           └── reserved(&self) -> u32
│   │       └── RegionTableEntry<'a>        # Region Table Entry 视图
│   │           ├── guid(&self) -> Guid
│   │           ├── file_offset(&self) -> u64
│   │           ├── length(&self) -> u32
│   │           └── required(&self) -> u32
│   │
│   ├── Bat<'a>                             # BAT Section
│   │   ├── entry(&self, index: u64) -> Result<BatEntry>
│   │   ├── entries(&self) -> impl Iterator<Item = BatEntry<'_>> + '_  # 强制：零拷贝视图迭代
│   │   └── len(&self) -> usize
│   │
│   │   └── BatEntry                        # BAT Entry 结构体
│   │       ├── state(&self) -> BatState
│   │       ├── file_offset_mb(&self) -> u64
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
│   │       ├── entry(&self, item_id: &Guid) -> Result<TableEntry<'_>>
│   │       └── entries(&self) -> impl Iterator<Item = TableEntry<'_>> + '_ # 强制：零拷贝视图迭代
│   │
│   │       └── TableHeader<'a>
│   │           ├── signature(&self) -> &'a [u8; 8]
│   │           ├── reserved(&self) -> &'a [u8; 2]
│   │           ├── entry_count(&self) -> u16
│   │           └── reserved2(&self) -> &'a [u8; 20]
│   │
│   │       └── TableEntry<'a>
│   │           ├── item_id(&self) -> Guid
│   │           ├── offset(&self) -> u32
│   │           ├── length(&self) -> u32
│   │           ├── flags_bits(&self) -> u32
│   │           └── reserved(&self) -> u32
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
│   │           ├── entry(&self, index: usize) -> Result<KeyValueEntry<'_>>
│   │           ├── entries(&self) -> impl Iterator<Item = KeyValueEntry<'_>> + '_ # 强制：零拷贝视图迭代
│   │           └── key_value_data(&self) -> &[u8]
│   │           └── resolve_parent_path(&self) -> Result<ParentPath<'_>> # 按 relative_path->volume_path->absolute_win32_path 顺序解析（零拷贝）
│   │
│   │           └── LocatorHeader<'a>
│   │               ├── locator_type(&self) -> Guid
│   │               ├── reserved(&self) -> u16
│   │               └── key_value_count(&self) -> u16
│   │
│   │           └── KeyValueEntry<'a>
│   │               ├── key_offset(&self) -> u32
│   │               ├── value_offset(&self) -> u32
│   │               ├── key_length(&self) -> u16
│   │               ├── value_length(&self) -> u16
│   │               ├── key(&self, data: &[u8]) -> Result<String>
│   │               └── value(&self, data: &[u8]) -> Result<String>
│   │
│   └── Log<'a>                             # Log Section
│       ├── entry(&self, index: usize) -> Result<Entry<'_>>
│       └── entries(&self) -> impl Iterator<Item = Entry<'_>> + '_ # 强制：零拷贝视图迭代
│    
│       └── Entry<'a>                       # Log Entry
│           ├── header(&self) -> LogEntryHeader<'_>
│           ├── descriptor(&self, index: usize) -> Result<Descriptor<'_>>
│           ├── descriptors(&self) -> impl Iterator<Item = Result<Descriptor<'_>>> + '_ # 强制：零拷贝视图迭代
│           └── data(&self) -> impl Iterator<Item = DataSector<'_>> + '_ # 强制：零拷贝视图迭代
│    
│           └── Descriptor<'a>              # Descriptor 枚举
│               ├── Data(DataDescriptor<'a>)    # Data Descriptor 变体
│               │
│               └── Zero(ZeroDescriptor<'a>)    # Zero Descriptor 变体
│    
│               └── DataDescriptor<'a>      # Data Descriptor
│                   ├── signature(&self) -> &'a [u8; 4]
│                   ├── trailing_bytes(&self) -> u32
│                   ├── leading_bytes(&self) -> u64
│                   ├── file_offset(&self) -> u64
│                   └── sequence_number(&self) -> u64
│    
│               └── ZeroDescriptor<'a>      # Zero Descriptor
│                   ├── signature(&self) -> &'a [u8; 4]
│                   ├── reserved(&self) -> u32
│                   ├── zero_length(&self) -> u64
│                   ├── file_offset(&self) -> u64
│                   └── sequence_number(&self) -> u64
│    
│           └── LogEntryHeader<'a>          # Log Entry Header
│               ├── signature(&self) -> &'a [u8; 4]
│               ├── checksum(&self) -> u32
│               ├── entry_length(&self) -> u32
│               ├── tail(&self) -> u32
│               ├── sequence_number(&self) -> u64
│               ├── descriptor_count(&self) -> u32
│               ├── reserved(&self) -> u32
│               ├── log_guid(&self) -> Guid
│               ├── flushed_file_offset(&self) -> u64
│               └── last_file_offset(&self) -> u64
│    
│           └── DataSector<'a>              # Data Sector
│               ├── signature(&self) -> &'a [u8; 4]
│               ├── sequence_high(&self) -> u32
│               ├── data(&self) -> &'a [u8]
│               └── sequence_low(&self) -> u32
│    
├── IO<'a>                                  # IO模块 (扇区级操作)
│   └── sector(&self, sector: u64) -> Result<Sector<'_>>   # 输入: 全局扇区号
│   │
│   └── Sector<'a>                          # 扇区级定位与操作
│       ├── payload(&self) -> PayloadBlock<'_>
│       ├── read(&self, buf: &mut [u8], semantics: ReadSemanticsPolicy) -> Result<usize>
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
├── ReadSemanticsPolicy                     # BAT读语义策略
│   ├── EffectiveDataPreferred              # 实际数据优先
│   └── RawDataPreferred                    # 原始数据优先
├── ParentChainInfo                         # 差分链校验结果
│   ├── child(&self) -> PathBuf             # 当前子盘路径
│   ├── parent(&self) -> PathBuf            # 解析出的父盘路径
│   └── linkage_matched(&self) -> bool      # 是否匹配 parent_linkage
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

### 零拷贝实现约束（强制）

- 所有 `entries()/descriptors()/data()` 必须实现为**零拷贝视图迭代器**。
- 所有定长字节数组返回（如 signature/reserved）必须返回**借用视图**（`&[u8; N]`），禁止按值返回（`[u8; N]`）。
- 迭代返回项必须借用底层 section 缓冲区（带生命周期），不得在迭代路径中复制 entry/descriptor/sector 原始字节。
- 禁止在上述 API 内部构造中间 `Vec` / `String` / `Box` 作为返回流水线。
- 文档中“按需解析”为惰性解析语义，不得退化为“先整体拷贝再迭代”。

### 1. File - 核心 API

```rust
pub struct File;

impl File {
    /// 打开现有 VHDX 文件（只读默认）
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md（只读语义边界）
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
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md
    pub fn write(self) -> Self;

    /// 设置严格模式
    ///
    /// 标准：docs/Standard/MS-VHDX-宽松扩展标准.md §3
    /// strict=true 时启用严格校验（§3.1）。
    /// strict=false 仅放宽 optional unknown（§3.2），
    /// required unknown（region / metadata）仍必须失败（§3.2）。
    pub fn strict(self, strict: bool) -> Self;

    /// 设置日志回放策略（默认 `LogReplayPolicy::Require`）
    ///
    /// 标准：docs/Standard/MS-VHDX.md §2.3 + docs/Standard/MS-VHDX-只读扩展标准.md §3/§4
    pub fn log_replay(self, policy: LogReplayPolicy) -> Self;

    /// 完成打开操作
    ///
    /// 规范约束：
    /// - 标准：MS-VHDX-只读扩展标准 §4.1
    ///   若日志非空且策略为 `Require`，必须拒绝打开并返回 `Error::LogReplayRequired`。
    /// - 标准：MS-VHDX-只读扩展标准 §4.4
    ///   若策略为 `ReadOnlyNoReplay`，允许只读打开但不回放日志；
    ///   此时仅保证结构读取（Header/Region/Metadata 等），不保证 payload 数据面一致性。
    pub fn finish(self) -> Result<File>;
}
```

```rust
/// 日志回放策略
///
/// 标准：docs/Standard/MS-VHDX.md §2.3 + docs/Standard/MS-VHDX-只读扩展标准.md §4
pub enum LogReplayPolicy {
    /// 若存在日志则返回 LogReplayRequired
    /// 标准：MS-VHDX-只读扩展标准 §4.1
    Require,
    /// 打开阶段自动回放日志
    /// 标准：MS-VHDX-只读扩展标准 §4.2
    Auto,
    /// 只读场景允许以内存方式回放
    /// 标准：MS-VHDX-只读扩展标准 §4.3
    InMemoryOnReadOnly,
    /// 只读打开且不回放日志
    ///
    /// 标准：MS-VHDX-只读扩展标准 §4.4
    /// 约束：仅允许结构读取（Header/Region/Metadata 等），
    /// 不保证 payload 数据面的一致性读取。
    ReadOnlyNoReplay,
}

/// BAT 读语义策略
///
/// - `EffectiveDataPreferred`：实际数据优先
/// - `RawDataPreferred`：原始数据优先
///
/// 差分磁盘规则：无论策略为何，均以子磁盘数据优先。
pub enum ReadSemanticsPolicy {
    EffectiveDataPreferred,
    RawDataPreferred,
}

// 默认行为说明：
// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §3
// File::open(path).finish() 在未显式调用 .log_replay(...) 时，
// 等价于使用 LogReplayPolicy::Require。

/// 差分链校验结果
pub struct ParentChainInfo {
    /// 当前子盘路径
    pub fn child(&self) -> PathBuf,
    /// 解析出的父盘路径
    pub fn parent(&self) -> PathBuf,
    /// 是否匹配 parent_linkage
    pub fn linkage_matched(&self) -> bool,
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
        /// - parent_linkage 必须存在；若存在 parent_linkage2 则返回错误
        /// - relative_path / volume_path / absolute_win32_path 至少存在一个
        pub fn validate_parent_locator(&self) -> Result<()>;

        /// 差分链校验
        ///
        /// 校验 parent_linkage 与父盘 DataWriteGuid 的一致性。
        ///
        /// 若 Parent Locator 中存在 parent_linkage2，必须返回错误并拒绝通过校验。
        pub fn validate_parent_chain(&self) -> Result<ParentChainInfo>;
    }

    /// 可选：结构化校验问题（用于诊断/报告）
    pub struct ValidationIssue {
        pub fn section(&self) -> &'static str,
        pub fn code(&self) -> &'static str,
        pub fn message(&self) -> String,
        pub fn spec_ref(&self) -> &'static str,
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
    /// - index > 2: 返回 Error::InvalidParameter
    pub fn header(&self, index: usize) -> Result<HeaderStructure<'_>>;
    
    /// 获取Region Table
    /// - index = 0: 返回 current header 对应的 region table
    /// - index = 1: 返回 region table 1（偏移 192KB）
    /// - index = 2: 返回 region table 2（偏移 256KB）
    /// - index > 2: 返回 Error::InvalidParameter
    pub fn region_table(&self, index: usize) -> Result<RegionTable<'_>>;
}

/// File Type Identifier (8 bytes signature + 512 bytes creator) (64KB)
pub struct FileTypeIdentifier<'a> {
    pub fn signature(&self) -> &'a [u8; 8],
    pub fn creator(&self) -> &'a [u8],
}

/// VHDX Header 视图（4KB）
pub struct HeaderStructure<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn checksum(&self) -> u32,
    pub fn sequence_number(&self) -> u64,
    pub fn file_write_guid(&self) -> Guid,
    pub fn data_write_guid(&self) -> Guid,
    pub fn log_guid(&self) -> Guid,
    pub fn log_version(&self) -> u16,
    pub fn version(&self) -> u16,
    pub fn log_length(&self) -> u32,
    pub fn log_offset(&self) -> u64,
}

/// Region Table 视图（64KB）
///
/// 零拷贝约束：entries() 必须返回借用底层 Region Table 缓冲区的视图迭代器。
pub struct RegionTable<'a> {
    pub fn header(&self) -> RegionTableHeader<'a>,
    pub fn entries(&self) -> impl Iterator<Item = RegionTableEntry<'a>> + '_,
}

pub struct RegionTableHeader<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn checksum(&self) -> u32,
    pub fn entry_count(&self) -> u32,
    pub fn reserved(&self) -> u32,
}

pub struct RegionTableEntry<'a> {
    pub fn guid(&self) -> Guid,
    pub fn file_offset(&self) -> u64,
    pub fn length(&self) -> u32,
    pub fn required(&self) -> u32,
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
    ///
    /// 失败条件：
    /// - 索引越界 -> Error::BatEntryNotFound
    pub fn entry(&self, index: u64) -> Result<BatEntry>;
    
    /// 获取所有BAT Entries（按需解析为视图列表）
    ///
    /// 零拷贝约束：返回项必须借用 BAT 原始缓冲区，不得复制 Entry 原始字节。
    pub fn entries(&self) -> impl Iterator<Item = BatEntry<'_>> + '_;
    
    /// BAT Entry数量
    pub fn len(&self) -> usize;
    
    pub fn is_empty(&self) -> bool;
}

/// BAT Entry 结构体（零拷贝视图）
/// 
/// 存储 Payload Block 或 Sector Bitmap Block 的元数据
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatEntry<'a> {
    /// Entry 类型和状态
    pub fn state(&self) -> BatState,
    /// 文件偏移（MB为单位）
    pub fn file_offset_mb(&self) -> u64,
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

/// Payload 状态读取语义（新增约束）
///
/// - 差分磁盘：无论何种状态，均以子磁盘数据优先。
/// - Undefined：原始数据语义与实际数据语义均返回 0。
/// - Unmapped：
///   - 原始数据语义：返回当前存储的原始数据；
///   - 实际数据语义：返回 0。
///
/// 最终返回由 `ReadSemanticsPolicy` 决定：
/// - EffectiveDataPreferred -> 实际数据语义
/// - RawDataPreferred -> 原始数据语义

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
    ///
    /// 失败条件：
    /// - 未找到 -> Error::MetadataNotFound { guid }
    pub fn entry(&self, item_id: &Guid) -> Result<TableEntry<'_>>;
    
    /// 获取所有Entries（按需解析为视图列表）
    ///
    /// 零拷贝约束：返回项必须直接借用 Metadata Table 区域。
    pub fn entries(&self) -> impl Iterator<Item = TableEntry<'_>> + '_;
}

/// Table Header (32字节)
pub struct TableHeader<'a> {
    pub fn signature(&self) -> &'a [u8; 8],
    pub fn reserved(&self) -> &'a [u8; 2],
    pub fn entry_count(&self) -> u16,
    pub fn reserved2(&self) -> &'a [u8; 20],
}

/// Table Entry (32字节)
pub struct TableEntry<'a> {
    pub fn item_id(&self) -> Guid,
    pub fn offset(&self) -> u32,
    pub fn length(&self) -> u32,
    pub fn flags_bits(&self) -> u32,
    pub fn reserved(&self) -> u32,
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
    pub fn block_size(&self) -> u32,
    pub fn flags(&self) -> u32,
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
    ///
    /// 失败条件：
    /// - 索引越界 -> Error::InvalidParameter
    pub fn entry(&self, index: usize) -> Result<KeyValueEntry<'_>>;
    
    /// 获取所有Key-Value Entries（按需解析为视图列表）
    ///
    /// 零拷贝约束：entries() 仅产出借用视图；不得在此处提前解码/分配字符串。
    pub fn entries(&self) -> impl Iterator<Item = KeyValueEntry<'_>> + '_;
    
    /// 获取Key-Value数据区域
    pub fn key_value_data(&self) -> &[u8];

    /// 解析父路径（零拷贝视图）
    ///
    /// 按规范顺序尝试：relative_path -> volume_path -> absolute_win32_path。
    /// 返回借用视图，不在该 API 内分配 `PathBuf`。
    ///
    /// 失败条件：
    /// - 所有路径均无法访问或丢失 -> Error::ParentNotFound
    pub fn resolve_parent_path(&self) -> Result<ParentPath<'_>>;
}

/// Parent 路径零拷贝视图
pub enum ParentPath<'a> {
    Relative(&'a std::path::Path),
    Volume(&'a std::path::Path),
    AbsoluteWin32(&'a std::path::Path),
}

/// Locator Header (20字节)
pub struct LocatorHeader<'a> {
    pub fn locator_type(&self) -> Guid,
    pub fn reserved(&self) -> u16,
    pub fn key_value_count(&self) -> u16,
}

/// Key-Value Entry (12字节)
pub struct KeyValueEntry<'a> {
    pub fn key_offset(&self) -> u32,
    pub fn value_offset(&self) -> u32,
    pub fn key_length(&self) -> u16,
    pub fn value_length(&self) -> u16,
}

impl<'a> KeyValueEntry<'a> {
    /// 从key_value_data中获取Key字符串（UTF-16LE解码）
    ///
    /// 失败条件：
    /// - 编码非法或数据切片越界 -> Error::LogEntryCorrupted
    pub fn key(&self, data: &[u8]) -> Result<String>;
    
    /// 从key_value_data中获取Value字符串（UTF-16LE解码）
    ///
    /// 失败条件：
    /// - 编码非法或数据切片越界 -> Error::LogEntryCorrupted
    pub fn value(&self, data: &[u8]) -> Result<String>;
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
    ///
    /// 失败条件：
    /// - 索引越界 -> Error::LogEntryCorrupted
    pub fn entry(&self, index: usize) -> Result<Entry<'_>>;
    
    /// 获取所有Entries（按需解析为视图列表）
    ///
    /// 零拷贝约束：返回借用 Log 区域的 Entry 视图，禁止复制条目载荷。
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> + '_;
}

/// Log Entry（组合结构，包含header、descriptors和sectors）
pub struct Entry<'a>;

impl<'a> Entry<'a> {
    /// 获取Log Entry Header
    pub fn header(&self) -> LogEntryHeader<'_>;
    
    /// 根据索引获取单个Descriptor
    ///
    /// 失败条件：
    /// - 索引越界 -> Error::InvalidParameter
    /// - 签名非法 -> Error::LogEntryCorrupted
    pub fn descriptor(&self, index: usize) -> Result<Descriptor<'_>>;
    
    /// 获取所有Descriptors（按原始顺序，按需解析）
    ///
    /// 零拷贝约束：返回借用当前 Entry 缓冲区的 Descriptor 视图。
    /// 每个迭代项独立验证签名，非法签名返回 Err。
    pub fn descriptors(&self) -> impl Iterator<Item = Result<Descriptor<'_>>> + '_;
    
    /// 获取Data Sectors（按需解析）
    ///
    /// 零拷贝约束：仅返回对现有 Data Sector 区域的借用视图。
    pub fn data(&self) -> impl Iterator<Item = DataSector<'_>> + '_;
}

/// Descriptor 枚举
///
/// 解析规则（签名判定）：
/// - 4 字节签名 == `"desc"` -> `Data` 变体
/// - 4 字节签名 == `"zero"` -> `Zero` 变体
/// - 其他签名 -> always `Error::LogEntryCorrupted`
///
/// 注意：Descriptor 属于日志结构的内核组成部分，出现未知签名
/// 等价于数据损坏，不受 strict 模式影响——无论 strict=true 还是
/// strict=false，未知签名均返回 LogEntryCorrupted。
pub enum Descriptor<'a> {
    Data(DataDescriptor<'a>),
    Zero(ZeroDescriptor<'a>),
}

/// Data Descriptor (32字节)
pub struct DataDescriptor<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn trailing_bytes(&self) -> u32,
    pub fn leading_bytes(&self) -> u64,
    pub fn file_offset(&self) -> u64,
    pub fn sequence_number(&self) -> u64,
}

/// Zero Descriptor (32字节)
pub struct ZeroDescriptor<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn reserved(&self) -> u32,
    pub fn zero_length(&self) -> u64,
    pub fn file_offset(&self) -> u64,
    pub fn sequence_number(&self) -> u64,
}

/// Log Entry Header (64字节)
pub struct LogEntryHeader<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn checksum(&self) -> u32,
    pub fn entry_length(&self) -> u32,
    /// 活跃序列起始偏移量（MS-VHDX §2.3.1.1）
    ///
    /// 从 Log 区起始到本条 Entry 所属活跃序列的第一条 Entry 的字节偏移。
    /// 必须是 4 KB 的倍数。单 Entry 序列中 tail 指向自身。
    pub fn tail(&self) -> u32,
    pub fn sequence_number(&self) -> u64,
    pub fn descriptor_count(&self) -> u32,
    pub fn reserved(&self) -> u32,
    pub fn log_guid(&self) -> Guid,
    pub fn flushed_file_offset(&self) -> u64,
    pub fn last_file_offset(&self) -> u64,
}

/// Data Sector 日志数据单元
///
/// VHDX 日志中的 Data Sector（4KB）存储了原始扇区数据的中间段（字节 8~4091）。
/// 日志回放时，需要结合关联 DataDescriptor 的 LeadingBytes（8 字节）和
/// TrailingBytes（4 字节），拼接成完整的 4KB 原始扇区：
///
///   完整扇区（4096 字节） = LeadingBytes + [8..4091]的 Data + TrailingBytes
///
/// 本结构体仅包含日志文件中的中间段字段；data() 返回的是**拼装后的完整 4KB 原始扇区**
/// （而非仅日志中存储的 4084 字节中间段）。
pub struct DataSector<'a> {
    pub fn signature(&self) -> &'a [u8; 4],
    pub fn sequence_high(&self) -> u32,
    /// 返回拼装后的完整原始扇区（4096 字节）
    ///
    /// 该返回值由 `LeadingBytes(8B) + 日志data区(4084B) + TrailingBytes(4B)`
    /// 拼接而成，与最后一次写入该扇区的原始数据一致。
    pub fn data(&self) -> &'a [u8],
    pub fn sequence_low(&self) -> u32,
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
    ///
    /// 失败条件：
    /// - 扇区号越界（超出虚拟磁盘范围） -> Error::SectorOutOfBounds
    /// - 文件状态异常（如父链不可用）    -> 对应具体错误
    pub fn sector(&self, sector: u64) -> Result<Sector<'_>>;
}

/// Sector - 扇区级定位与操作
/// 
/// 封装了PayloadBlock引用和块内扇区索引
#[derive(Clone, Debug, PartialEq)]
pub struct Sector<'a> {
    // 简单类型字段: 块内扇区索引
    pub fn block_sector_index(&self) -> u32,
    pub fn payload_ref(&self) -> PayloadBlock<'a>,
}

impl<'a> Sector<'a> {
    /// 获取对应的PayloadBlock
    pub fn payload(&self) -> PayloadBlock<'_>;
    
    /// 读取扇区数据
    /// buf长度必须为扇区大小的整数倍
    ///
    /// 读取语义补充：
    /// - 差分磁盘始终以子磁盘数据优先。
    /// - `Undefined` 状态：原始/实际语义均返回 0。
    /// - `Unmapped` 状态：
    ///   - `ReadSemanticsPolicy::EffectiveDataPreferred` 返回 0；
    ///   - `ReadSemanticsPolicy::RawDataPreferred` 返回当前存储的原始数据。
    ///
    /// 调用方可按本次读取传入语义策略；推荐默认传入
    /// `ReadSemanticsPolicy::EffectiveDataPreferred`（实际数据优先）。
    pub fn read(&self, buf: &mut [u8], semantics: ReadSemanticsPolicy) -> Result<usize>;
    
    /// 写入扇区数据
    /// data长度必须为扇区大小的整数倍
    pub fn write(&self, data: &[u8]) -> Result<()>;
}

/// Payload Block - 内部结构
/// 
/// 用户通过Sector访问，不直接操作
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadBlock<'a> {
    pub fn bytes(&self) -> &'a [u8],
}
```


## 模块结构

```rust
// lib.rs - 公共 API 导出

// 核心类型
pub use error::{Error, Result};
pub use types::Guid;
pub use file::{LogReplayPolicy, ReadSemanticsPolicy, ParentChainInfo};
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
    // 标准：docs/Standard/MS-VHDX-只读扩展标准.md
    let file = File::open("disk.vhdx")
        // 标准：docs/Standard/MS-VHDX-只读扩展标准.md §4.1（Require）
        .log_replay(LogReplayPolicy::Require)
        .finish()?;

    // 使用独立校验器（推荐）
    file.validator().validate_file()?;
    
    // 获取sections容器
    let sections = file.sections();
    
    // 访问Header Section
    let header = sections.header()?;
    println!("File Type: {:?}", header.file_type().signature());
    println!("Current Header Seq: {}", header.header(0)?.sequence_number());
    
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
    println!("Metadata Entry count: {}", metadata.table().header().entry_count());
    
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
        let entry = bat.entry(i)?;
        match entry.state() {
            BatState::Payload(state) => {
                println!("Block {}: Payload State={:?}, Offset={}MB",
                    i, state, entry.file_offset_mb());
            }
            BatState::SectorBitmap(state) => {
                println!("Block {}: SectorBitmap State={:?}, Offset={}MB",
                    i, state, entry.file_offset_mb());
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
    let sector0 = io.sector(0)?;
    let data = vec![0u8; 4096];
    sector0.write(&data)?;
    
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
    let current_header = header.header(0)?;
    let metadata = sections.metadata()?;

    let summary = format!(
        "seq={}\nlog_length={}\nmetadata_entries={}\n",
        current_header.sequence_number(),
        current_header.log_length(),
        metadata.table().header().entry_count(),
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
        // 标准：docs/Standard/MS-VHDX-宽松扩展标准.md §3.1（strict=true）
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
                println!("Parent Locator Entries: {}", locator.header().key_value_count());
                for (i, entry) in locator.entries().enumerate() {
                    let key = entry.key(locator.key_value_data())?;
                    let value = entry.value(locator.key_value_data())?;
                    println!("  [{}] {}: {}", i, key, value);
                }
                println!("Resolved parent path: {:?}", locator.resolve_parent_path()?);
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
