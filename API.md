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
│   ├── io(&self) -> Result<IO<'_>>         # 获取IO模块
│   ├── validator(&self) -> validation::SpecValidator<'_>  # 获取规范校验器
│   └── inner(&self) -> &std::fs::File
│
│   └── OpenOptions                         # 关联类型：打开选项
│       ├── write(self) -> Self             # 启用写权限（RW）
│       ├── strict(self, bool) -> Self      # 是否启用严格模式（默认 true，所有 unknown 始终失败）
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
│   │   ├── validate_file(&self) -> Result<Vec<ValidationIssue>> # 总入口
│   │   ├── validate_header(&self) -> Result<Vec<ValidationIssue>>
│   │   ├── validate_region_table(&self) -> Result<Vec<ValidationIssue>>
│   │   ├── validate_bat(&self) -> Result<Vec<ValidationIssue>>
│   │   ├── validate_metadata(&self) -> Result<Vec<ValidationIssue>>
│   │   ├── validate_required_metadata_items(&self) -> Result<Vec<ValidationIssue>>
│   │   ├── validate_log(&self) -> Result<Vec<ValidationIssue>>
│   │   └── validate_parent_locator(&self) -> Result<Vec<ValidationIssue>> # 含 parent_linkage2 检查 + 差分链 GUID 校验
│   └── ValidationIssue                      # 结构化校验问题
│
├── section::                               # Section模块 - 物理文件结构映射
│   ├── Sections<'a>                        # 容器，管理所有sections (懒加载)
│   │   ├── header(&self) -> Result<Header<'_>>
│   │   ├── bat(&self) -> Result<Bat<'_>>
│   │   ├── metadata(&self) -> Result<Metadata<'_>>
│   │   └── log(&self) -> Result<Log<'_>>
│   │
│   ├── Header<'a>                          # Header Section (1 MB)
│   │   ├── file_type(&self) -> FileTypeIdentifier<'_>
│   │   ├── header(&self, index: usize) -> Result<HeaderStructure<'_>>   # 0=current, 1=header1, 2=header2
│   │   └── region_table(&self, index: usize) -> Result<RegionTable<'_>> # 0=current, 1=rt1, 2=rt2
│   │
│   │   └── FileTypeIdentifier<'a>          # 文件类型标识符视图
│   │       ├── signature(&self) -> &'a [u8; 8]
│   │       └── creator(&self) -> &'a [u8; 512]
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
│   │       ├── header(&self) -> RegionTableHeader<'_>
│   │       ├── entries(&self) -> impl Iterator<Item = RegionTableEntry<'_>> + '_  # 强制：零拷贝视图迭代
│   │       │
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
│   │   └── BatEntry<'a>                    # BAT Entry 结构体
│   │       ├── state(&self) -> Result<BatState>
│   │       ├── file_offset_mb(&self) -> u64
│   │
│   │       └── BatState 枚举:                  # Entry 类型枚举
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
│   │           ├── reserved(&self) -> u32
│   │           └── flags(&self) -> EntryFlags
│   │
│   │           └── EntryFlags
│   │               ├── is_user(&self) -> bool
│   │               ├── is_virtual_disk(&self) -> bool
│   │               └── is_required(&self) -> bool
│   │
│   │       └── StandardItems                # 标准 Metadata Item GUID 常量
│   │           ├── FILE_PARAMETERS          # CAA16737-FA36-4D43-B3B6-33F0AA44E76B
│   │           ├── VIRTUAL_DISK_SIZE        # 2FA54224-CD1B-4876-B211-5DBED83BF4B8
│   │           ├── VIRTUAL_DISK_ID          # BECA12AB-B2E6-4523-93EF-C309E000C746
│   │           ├── LOGICAL_SECTOR_SIZE      # 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
│   │           ├── PHYSICAL_SECTOR_SIZE     # CDA348C7-445D-4471-9CC9-E9885251C556
│   │           ├── PARENT_LOCATOR           # A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
│   │           └── LOCATOR_TYPE_VHDX        # B04AEFB7-D19E-4A81-B789-25B8E9445913
│   │
│   │   └── MetadataItems<'a>
│   │       ├── file_parameters(&self) -> Result<FileParameters<'_>>
│   │       ├── virtual_disk_size(&self) -> Result<u64>
│   │       ├── virtual_disk_id(&self) -> Result<Guid>
│   │       ├── logical_sector_size(&self) -> Result<u32>
│   │       ├── physical_sector_size(&self) -> Result<u32>
│   │       └── parent_locator(&self) -> Result<ParentLocator<'_>>
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
│   │           ├── key_value_data(&self) -> &[u8]
│   │           ├── resolve_parent_path(&self) -> Result<PathBuf> # 按 relative_path->volume_path->absolute_win32_path 顺序解析（UTF-16LE 解码）
│   │           │
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
│               ├── sequence_number(&self) -> u64
│               └── data(&self) -> Cow<'a, [u8]>    # Cow：拼接 LeadingBytes+中间段+TrailingBytes 无法零拷贝
│    
├── IO<'a>                                  # IO模块 (扇区级操作)
│   └── sector(&self, start: u64, count: u64) -> Result<Sector<'_>>   # 输入: 起始扇区号 + 连续扇区数
│
│   └── Sector<'a>                          # 扇区级定位与操作（游标式）
│       ├── semantics(self, ReadSemanticsPolicy) -> Self  # 设置读语义策略（链式，默认 EffectiveDataPreferred）
│       ├── impl Read   → fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>
│       ├── impl Write  → fn write(&mut self, buf: &[u8]) -> io::Result<usize>
│       │               + fn flush(&mut self) -> io::Result<()>
│       └── impl Seek   → fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>
│
├── Guid                                    # GUID 类型
├── LogReplayPolicy                         # 日志回放策略
│   ├── Require                             # 若存在日志则返回 LogReplayRequired
│   ├── Auto                                # 打开阶段自动回放日志
│   ├── InMemoryOnReadOnly                  # 只读场景以内存方式回放
│   └── ReadOnlyNoReplay                    # 只读打开且不回放日志（允许带未回放日志读取元数据）
├── ReadSemanticsPolicy                     # BAT读语义策略（通过 Sector::semantics 链式设置）
│   ├── EffectiveDataPreferred              # 实际数据优先（默认）
│   └── RawDataPreferred                    # 原始数据优先
│
├── SignaturePosition                       # 签名错误位置枚举
│   ├── FileTypeIdentifier                  # 文件类型标识符（vhdxfile）
│   ├── Header                              # Header（head）
│   ├── RegionTable                         # Region Table（regi）
│   ├── MetadataTable                       # Metadata Table（metadata）
│   ├── LogEntry                            # Log Entry Header（loge）
│   ├── Descriptor                          # Descriptor（desc/zero）
│   └── DataSector                          # Data Sector（data）
│
└── Error                                   # 错误类型
    ├── Io(std::io::Error)                  # 底层 IO 错误
    ├── InvalidFile(String)                 # 无效的 VHDX 文件
    ├── InvalidSignature                    # 签名不匹配
    │   { position: SignaturePosition, expected: [u8; 8], found: [u8; 8] }
    │   # 说明：部分签名仅 4 字节（如 Header "head"、Region Table "regi"），
    │   # 存储到 8 字节字段时高位补零。expected/found 均按 8 字节表示以统一接口类型。
    ├── CorruptedHeader(String)             # 头部损坏
    ├── HeaderLogGuidMismatch               # 双 Header LogGuid 不一致
    │   { header1_log_guid: Guid, header2_log_guid: Guid }
    ├── HeaderSequenceNumberInvalid         # 双 Header 序列号异常
    │   { sequence_number_1: u64, sequence_number_2: u64 }
    ├── UnsupportedVersion { version: u16 } # 不支持的 VHDX 版本（Version != 1）
    ├── UnsupportedLogVersion { version: u16 } # 不支持的 Log 版本（LogVersion != 0）
    ├── InvalidChecksum                     # CRC32C 校验和不匹配
    │   { expected: u32, actual: u32 }
    ├── InvalidBlockState(u8)               # 无效的 Payload Block 状态值
    ├── InvalidSectorBitmapState(u8)        # 无效的 Sector Bitmap 块状态值
    ├── StateMismatch                       # BAT 状态值与磁盘类型不匹配
    │   { state: u8, description: String }
    ├── BatFileOffsetUnaligned              # BAT 条目文件偏移未按块大小对齐
    │   { offset_mb: u64, block_size: u32 }
    ├── BatEntryCountInsufficient           # BAT entry 数量不足以覆盖虚拟磁盘
    │   { actual: u64, expected: u64 }
    ├── BatFileOffsetDuplicate              # BAT 多 entry 指向同偏移
    │   { offset_mb: u64 }
    ├── InvalidRegionTable(String)          # 区域表格式错误
    ├── RegionRequiredUnknown               # 未知的 required region
    │   { guid: Guid }
    ├── RegionOptionalUnknown               # 未知的 optional region
    │   { guid: Guid }
    ├── InvalidMetadata(String)             # 元数据格式错误
    ├── MetadataGuidUnknown                 # 未知 Metadata Item GUID
    │   { guid: Guid }
    ├── MetadataRequiredMissing             # required metadata item 缺失
    │   { guid: Guid }
    ├── MetadataRequiredUnknown             # 未知的 required metadata item
    │   { guid: Guid }
    ├── MetadataOptionalUnknown             # 未知的 optional metadata item（strict=true 时阻断，strict=false 时忽略）
    │   { guid: Guid }
    ├── MetadataReservedFlagsSet            # Metadata 保留标志位被设置
    │   { flags: u32 }
    ├── InvalidParentLocator(String)        # 父定位器格式错误
    ├── MetadataNotFound { guid: Guid }     # 元数据项未找到
    ├── LogReplayRequired                   # 需要日志回放
    ├── LogEntryCorrupted(String)           # 日志条目损坏
    ├── LogSequenceGap                      # 日志 sequence 不连续
    │   { expected: u64, found: u64 }
    ├── LogSequenceGuidMismatch             # 日志 sequence GUID 不匹配
    │   { entry_log_guid: Guid, header_log_guid: Guid }
    ├── LogActiveSequenceEmpty              # 活跃 sequence 为空
    ├── BatEntryNotFound { index: u64 }     # BAT 条目未找到
    ├── BlockNotPresent                     # 数据块未分配
    │   { block_idx: u64, state: String }
    ├── SectorOutOfBounds                   # 扇区索引越界
    │   { sector: u64, max: u64 }
    ├── ParentNotFound                      # 父磁盘未找到（三个路径均不可访问）
    ├── ParentMismatch                      # 父磁盘 GUID 不匹配
    │   { expected: Guid, actual: Guid }
    ├── ParentLocatorMissingLinkage         # parent_linkage key 不存在
    ├── ParentLocatorLinkage2Conflict       # parent_linkage2 存在（merge 冲突）
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
│   ├── --logical-sector-size <size>        # 逻辑扇区大小 (默认: 4096)
│   ├── --physical-sector-size <size>       # 物理扇区大小 (默认: 4096)
│   └── --force                             # 覆盖已存在文件
│
├── check [file]                            # 检查文件完整性
│   ├── --log-replay                        # 重放日志
│   └── --strict [<bool>]                   # 启用/禁用严格模式 (默认: true)
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
- 除以下极少数特殊情况外，禁止在上述 API 内部构造中间 `Vec` / `String` / `Box` 作为返回流水线：
  1. `DataSector::data()`：拼接 `LeadingBytes(DataDescriptor) + 中间段(DataSector) + TrailingBytes(DataDescriptor)` 为完整 4KB 扇区，无法零拷贝完成（见 DataSector FIXME）。
  2. `ParentLocator::resolve_parent_path()`：UTF-16LE 解码为 `PathBuf`，需要分配内存，无法返回借用视图。
  3. `KeyValueEntry::key()` / `value()`：UTF-16LE 解码为 `String`，需要分配内存，无法返回借用视图。
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
    /// 前置条件：文件已成功打开（只读即满足）
    /// IO 创建可能失败（如元数据不完整），因此返回 `Result<IO<'_>>`。
    pub fn io(&self) -> Result<IO<'_>>;

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
pub struct OpenOptions;

impl OpenOptions {
    /// 启用写权限（默认为只读）
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md
    pub fn write(self) -> Self;

    /// 设置严格模式（默认 true）
    ///
    /// 标准：docs/Standard/MS-VHDX-宽松扩展标准.md §3
    /// 宽松扩展标准要求 strict 默认值为 true（§6.最小合规清单）。
    /// strict=true 时启用严格校验（§3.1）：所有 unknown（含 optional）一律失败。
    /// strict=false 仅放宽 optional unknown（§3.2）：optional unknown MUST 忽略，
    /// required unknown（region / metadata）仍必须失败。
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogReplayPolicy {
    /// 若存在日志则返回 LogReplayRequired
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §4.1
    Require,
    /// 打开阶段自动回放日志
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §4.2
    /// 若为只读打开，回放 **MUST** 采用内存语义（在内存中构建回放后视图），**MUST NOT** 写回底层文件。
    /// 若为可写打开，回放按标准流程写入文件。
    Auto,
    /// 以内存方式回放日志（仅限只读打开）
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §4.3
    /// 实现 MUST 在内存中构建回放后视图，**MUST NOT** 写回底层文件。
    /// 仅允许用于只读打开模式（即未调用 `OpenOptions::write()`）。
    /// 若以可写打开模式调用 `finish()` 并传递此策略，
    /// 实现 **MUST** 返回策略冲突错误并拒绝打开。
    InMemoryOnReadOnly,
    /// 不回放日志的只读打开
    ///
    /// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §4.4
    /// 仅允许用于只读打开模式（即未调用 `OpenOptions::write()`）。
    /// 若以可写打开模式调用 `finish()` 并传递此策略，
    /// 实现 **MUST** 返回策略冲突错误并拒绝打开。
    /// 实现 **MUST** 跳过日志回放并允许 `finish()` 成功。
    /// 约束：仅保证结构面读取（Header/Region/Metadata 等），
    /// 不保证 payload 数据面的一致性读取。
    ReadOnlyNoReplay,
}

/// BAT 读语义策略
///
/// 标准：docs/Standard/MS-VHDX.md §2.5.1.1（BAT entry state 语义）
/// 对应 PayloadBlockState 的 Undefined / Unmapped 在不同策略下的行为差异。
///
/// - `EffectiveDataPreferred`：实际数据优先（Unmapped 返回 0）
/// - `RawDataPreferred`：原始数据优先（Unmapped 返回当前存储的原始数据）
///
/// 差分磁盘规则：无论策略为何，均以子磁盘数据优先。
///
/// 通过 `Sector::semantics()` 链式设置在 Sector 上。
/// 未显式设置时，默认使用 `EffectiveDataPreferred`。
pub enum ReadSemanticsPolicy {
    EffectiveDataPreferred,
    RawDataPreferred,
}

// 默认行为说明：
// 标准：docs/Standard/MS-VHDX-只读扩展标准.md §3
// File::open(path).finish() 在未显式调用 .log_replay(...) 时，
// 等价于使用 LogReplayPolicy::Require。

### 3. File::CreateOptions - 创建选项

```rust
pub struct CreateOptions;

impl CreateOptions {
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
    /// 约束：只能为 512 或 4096。
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
    ///
    /// ⚠️ **已知限制**：本校验器将 `parent_linkage2` 视为冲突。
    /// MS-VHDX 规范 §2.6.2.6.3 定义 `parent_linkage2` 为"can't be present"，
    /// 但 Microsoft 产品实现在磁盘 merge 过渡期间会暂时写入该字段（merge 完成后解决）。
    /// `validate_parent_locator()` 检测到 parent_linkage2 时将返回错误，
    /// 因此**无法校验处于 merge 进行中的 VHDX 文件**。
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
        /// - Differencing: 在 has_parent=true 时覆盖 Parent Locator 校验（含 parent_linkage 约束 + 差分链 GUID 一致性）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的校验问题，
        /// 空 vec 表示完全合规。第一个导致校验终止的错误通过 `Err` 返回。
        pub fn validate_file(&self) -> Result<Vec<ValidationIssue>>;

        /// Header Section 校验（签名/CRC/current header/version/log 对齐）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 Header 校验问题。
        pub fn validate_header(&self) -> Result<Vec<ValidationIssue>>;

        /// Region Table 校验（regi/CRC/entry 约束/required unknown 拒绝加载）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 Region Table 校验问题。
        pub fn validate_region_table(&self) -> Result<Vec<ValidationIssue>>;

        /// BAT 校验（entry 状态合法性与磁盘类型匹配）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 BAT 校验问题。
        pub fn validate_bat(&self) -> Result<Vec<ValidationIssue>>;

        /// Metadata 校验（table/entry/已知项约束，不含 required 完整性）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 Metadata 校验问题。
        pub fn validate_metadata(&self) -> Result<Vec<ValidationIssue>>;

        /// 仅校验 Metadata required item 约束
        ///
        /// 对于 IsRequired=true 但未知/缺失的项，返回错误。
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有缺失/未知的 required item。
        pub fn validate_required_metadata_items(&self) -> Result<Vec<ValidationIssue>>;

        /// Log 校验（entry/descriptor/data sector/active sequence/replay 前置）
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 Log 校验问题。
        pub fn validate_log(&self) -> Result<Vec<ValidationIssue>>;

        /// 校验 Parent Locator 约束（含 parent_linkage 约束 + 差分链 GUID 一致性）
        ///
        /// - parent_linkage 必须存在
        /// - 若存在 parent_linkage2 则返回错误（见 struct 文档的已知限制说明）
        /// - relative_path / volume_path / absolute_win32_path 至少存在一个
        /// - 若父磁盘可访问，校验子盘 DataWriteGuid 与父盘预期的一致性
        ///
        /// 返回 `Vec<ValidationIssue>` 列出所有发现的 Parent Locator / 差分链校验问题。
        pub fn validate_parent_locator(&self) -> Result<Vec<ValidationIssue>>;
    }

    /// 结构化校验问题
    ///
    /// 由 `validate_*()` 方法通过 `Result::Ok(Vec<ValidationIssue>)` 返回。
    /// 空 vec 表示该阶段未发现校验问题。
    pub struct ValidationIssue {
        /// 校验阶段名称，如 `"bat"`、`"log"`、`"header"`（对应 §3.3 section 值）
        pub fn section(&self) -> &'static str,
        /// 错误码，如 `"BAT_ENTRY_INVALID_STATE"`（对应 §4 字典）
        pub fn code(&self) -> &'static str,
        /// 人类可读的描述，包含关键上下文值
        pub fn message(&self) -> String,
        /// 标准章节引用，如 `"MS-VHDX/2.5.1.1"`
        pub fn spec_ref(&self) -> &'static str,
    }
}
```

---


### 4. Section 容器

```rust
/// VHDX文件中的所有Section的容器
/// 
/// 采用懒加载策略：访问具体Section时才从文件读取。
/// 内部使用 `OnceLock` 缓存已加载的 section 缓冲区，
/// 多次调用同一方法仅产生一次 I/O。
pub struct Sections<'a> {
    // 内部字段：缓存已加载的sections
}

impl<'a> Sections<'a> {
    /// 访问Header Section
    /// 懒加载：首次调用时从文件读取1MB Header Section
    pub fn header(&self) -> Result<Header<'_>>;
    
    /// 访问BAT Section
    /// 懒加载：首次调用时从文件读取BAT区域
    pub fn bat(&self) -> Result<Bat<'_>>;
    
    /// 访问Metadata Section
    /// 懒加载：首次调用时从文件读取Metadata区域
    pub fn metadata(&self) -> Result<Metadata<'_>>;
    
    /// 访问Log Section
    /// 懒加载：首次调用时从文件读取Log区域
    pub fn log(&self) -> Result<Log<'_>>;
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
    /// - index = 0: 返回 current header 对应的 region table（配对策略见 docs/Standard/MS-VHDX.md重点解读.md，按物理位置推断，Header 1 ↔ Region Table 1）
    /// - index = 1: 返回 region table 1（偏移 192KB）
    /// - index = 2: 返回 region table 2（偏移 256KB）
    /// - index > 2: 返回 Error::InvalidParameter
    pub fn region_table(&self, index: usize) -> Result<RegionTable<'_>>;
}

/// File Type Identifier (8 bytes signature + 512 bytes creator) (64KB)
pub struct FileTypeIdentifier<'a> {
    pub fn signature(&self) -> &'a [u8; 8],
    pub fn creator(&self) -> &'a [u8; 512],
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
}

/// BAT Entry 结构体（零拷贝视图）
/// 
/// 存储 Payload Block 或 Sector Bitmap Block 的元数据
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatEntry<'a> {
    pub fn state(&self) -> Result<BatState>,
    /// 文件偏移（MB为单位）
    pub fn file_offset_mb(&self) -> u64,
}

/// BAT Entry 类型枚举
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatState {
    /// Payload Block 状态
    Payload(PayloadBlockState),
    /// Sector Bitmap Block 状态
    SectorBitmap(SectorBitmapState),
}

/// Payload Block State
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
///
/// Flags 字段按小端 u32 存储，标准 MS-VHDX §2.6.1.2 Figure 19：
/// - A=IsUser     (bit 0, 即 Byte24 bit0)
/// - B=IsVirtualDisk (bit 1, 即 Byte24 bit1)
/// - C=IsRequired (bit 2, 即 Byte24 bit2)
/// - 其余位保留（MUST be 0）
pub struct EntryFlags(pub u32);

impl EntryFlags {
    /// 是否为用户元数据 (Bit 0)
    pub fn is_user(&self) -> bool;
    
    /// 是否为虚拟磁盘元数据 (Bit 1)
    pub fn is_virtual_disk(&self) -> bool;
    
    /// 是否为必需项 (Bit 2)
    pub fn is_required(&self) -> bool;
}

/// Metadata Items (64KB之后，变长)
///
/// 注意：required metadata item（如 FileParameters / VirtualDiskSize / VirtualDiskID）
/// 在合法 VHDX 文件中必须存在。返回 `Result` 而非 `Option` 以明确此语义——
/// 缺失属于格式错误，而非"可选字段不存在"。
///
/// 失败条件（required items 缺失）：对应 required GUID 的 item 未找到 -> Error::MetadataRequiredMissing { guid }
///
/// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4 METADATA_REQUIRED_MISSING
pub struct MetadataItems<'a>;

impl<'a> MetadataItems<'a> {
    /// 获取 File Parameters
    ///
    /// 失败条件：FileParameters item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::FILE_PARAMETERS }
    pub fn file_parameters(&self) -> Result<FileParameters<'_>>;
    
    /// 获取虚拟磁盘大小
    ///
    /// 失败条件：VirtualDiskSize item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::VIRTUAL_DISK_SIZE }
    pub fn virtual_disk_size(&self) -> Result<u64>;
    
    /// 获取虚拟磁盘ID
    ///
    /// 失败条件：VirtualDiskID item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::VIRTUAL_DISK_ID }
    pub fn virtual_disk_id(&self) -> Result<Guid>;
    
    /// 获取逻辑扇区大小
    ///
    /// 失败条件：LogicalSectorSize item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::LOGICAL_SECTOR_SIZE }
    pub fn logical_sector_size(&self) -> Result<u32>;
    
    /// 获取物理扇区大小
    ///
    /// 失败条件：PhysicalSectorSize item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::PHYSICAL_SECTOR_SIZE }
    pub fn physical_sector_size(&self) -> Result<u32>;
    
    /// 获取父定位器（差分磁盘）
    ///
    /// 失败条件：ParentLocator item 不存在 -> Error::MetadataRequiredMissing { guid: StandardItems::PARENT_LOCATOR }
    pub fn parent_locator(&self) -> Result<ParentLocator<'_>>;
}

/// File Parameters (8字节)
pub struct FileParameters<'a>;

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

    /// 解析父路径
    ///
    /// 按规范顺序尝试：relative_path -> volume_path -> absolute_win32_path。
    /// 返回 owned `PathBuf`：UTF-16LE 解码需要分配内存，无法返回借用视图。
    ///
    /// 失败条件：
    /// - 所有路径均无法访问或丢失 -> Error::ParentNotFound
    pub fn resolve_parent_path(&self) -> Result<PathBuf>;
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
    /// - 编码非法或数据切片越界 -> Error::InvalidParentLocator
    ///
    /// FIXME: 原使用 Error::LogEntryCorrupted，语义错误——KeyValueEntry 属 Metadata/ParentLocator 而非 Log。
    /// 已更正为 Error::InvalidParentLocator（见 API 树 Error 节）。
    pub fn key(&self, data: &[u8]) -> Result<String>;
    
    /// 从key_value_data中获取Value字符串（UTF-16LE解码）
    ///
    /// 失败条件：
    /// - 编码非法或数据切片越界 -> Error::InvalidParentLocator
    ///
    /// FIXME: 同 key()，原 LogEntryCorrupted 语义不匹配，已更正。
    pub fn value(&self, data: &[u8]) -> Result<String>;
}

/// 标准Metadata Item GUID常量
///
/// 路径：`vhdx::section::StandardItems`
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
    ///
    /// 标准：docs/Standard/MS-VHDX.md §2.6.2.6.3（VHDX Parent Locator）
    /// VHDX 父定位器类型 GUID，用于 Parent Locator Header 的 LocatorType 字段。
    /// 值：B04AEFB7-D19E-4A81-B789-25B8E9445913
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
    /// - 索引越界 -> Error::InvalidParameter
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
///
/// FIXME: "拼装"需要将 LeadingBytes(DataDescriptor) + 中间段(DataSector) + TrailingBytes(DataDescriptor)
/// 三部分拼接，这意味着 data() 无法单纯借用现有缓冲区返回 &[u8]。若走内部缓存则违反零拷贝约束。
/// 当前用 Cow 折中：可借用时返回 Borrowed，必须拼装时返回 Owned。实现时需注意分配开销。
pub struct DataSector<'a> {
    /// Signature. MUST be `"data"` (0x61746164).
    pub fn signature(&self) -> &'a [u8; 4],
    /// The reconstructed full 64-bit sequence number.
    pub fn sequence_number(&self) -> u64,
    /// 返回拼装后的完整原始扇区（4096 字节）
    ///
    /// 该返回值由 `LeadingBytes(8B) + 日志data区(4084B) + TrailingBytes(4B)`
    /// 拼接而成，与最后一次写入该扇区的原始数据一致。
    ///
    /// NOTE: 返回 `Cow<'a, [u8]>` 而非 `&'a [u8]`，因为拼装无法零拷贝完成。
    /// 零拷贝约束 §"禁止构造中间 Vec/Box/String 作为返回流水线"在此处需要豁免，
    /// 或考虑在 DataSector 内部增加缓存字段以支持后续的借用返回。
    pub fn data(&self) -> std::borrow::Cow<'a, [u8]>,
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
pub struct IO<'a>;

impl<'a> IO<'a> {
    /// 通过起始扇区号和连续扇区数定位并返回 Sector
    /// 
    /// 参数：
    /// - `start`: 起始全局扇区号（0-based）
    /// - `count`: 连续扇区数量（必须 > 0）
    ///
    /// 内部自动:
    /// 1) 通过 BAT 找到对应块 2) 计算块内扇区偏移
    /// 3) 跨块边界时自动按块拆分读写操作
    /// 
    /// 懒加载: Sector 缓存按需从文件读取
    ///
    /// 失败条件：
    /// - `count == 0`                       -> Error::InvalidParameter
    /// - `start + count` 算术溢出            -> Error::InvalidParameter
    /// - 扇区范围超出虚拟磁盘范围            -> Error::SectorOutOfBounds
    /// - 文件状态异常（如父链不可用）         -> 对应具体错误
    pub fn sector(&self, start: u64, count: u64) -> Result<Sector<'_>>;
}

/// Sector —— 扇区级定位与操作（游标式 IO）
///
/// 通过 `IO::sector()` 创建，行为对标 `std::fs::File`。
///
/// # 读取语义策略
///
/// Sector 默认使用 `ReadSemanticsPolicy::EffectiveDataPreferred` 读取语义。
/// 可通过 `semantics()` 链式方法覆盖策略，不破坏 `std::io::Read` trait 兼容性：
///
/// ```rust,ignore
/// let sector = io.sector(0, 256)?
///     .semantics(ReadSemanticsPolicy::RawDataPreferred);
/// sector.read(&mut buf)?;  // 使用 RawDataPreferred 语义
/// ```
///
/// # 游标
///
/// Sector 内部维护一个字节游标（`pos`），所有读写操作都从游标位置开始，
/// 操作完成后自动推进游标。通过 `Seek` trait 调整游标位置。
///
/// # 实现 Traits
///
/// | Trait | 方法 | 说明 |
/// |-------|------|------|
/// | `Read` | `read(&mut self, buf) -> io::Result<usize>` | 从游标位置读取，返回实际读取字节数。在范围末尾返回 `Ok(0)`。 |
/// | `Write` | `write(&mut self, buf) -> io::Result<usize>` | 从游标位置写入，返回实际写入字节数。在范围末尾返回 `Ok(0)`。 |
/// | `Write` | `flush(&mut self) -> io::Result<()>` | 无操作（写入直接入文件，无缓冲区）。 |
/// | `Seek` | `seek(&mut self, pos: SeekFrom) -> io::Result<u64>` | 移动游标。`Start`/`End`/`Current` 均被支持，结果钳位到 `[0, range_bytes]`。 |
///
/// # 行为说明
///
/// - **部分读写**：当 `buf.len()` 超过剩余可用范围时，只读取/写入可用部分。
/// - **范围末尾 (EOF)**：游标在末尾时，`read`/`write` 返回 `Ok(0)`，不是错误。
/// - **块边界**：跨块范围自动按 VHDX block 边界拆分，对调用方透明。
/// - **读取语义**：默认使用 `EffectiveDataPreferred`（`Unmapped` 块返回零），可通过 `semantics()` 覆盖。
/// - **写入前提**：目标块必须为 `FullyPresent` 或 `PartiallyPresent` 状态。
///
/// # 错误映射
///
/// 内部 VHDX 错误自动转换为 `std::io::Error`：
///
/// | 内部错误 | io::ErrorKind |
/// |---|---|
/// | 参数错误、格式错误、校验失败等 | `InvalidData` |
/// | 块/元数据不存在 | `NotFound` |
/// | 只读、日志待回放 | `PermissionDenied` |
/// | 扇区越界 | `UnexpectedEof` |
///
/// # 示例
///
/// ```rust,ignore
/// let file = File::open("disk.vhdx").finish()?;
/// let io = file.io()?;
/// let mut sector = io.sector(0, 256)?;  // 256 sectors = 1MB
///
/// // 写入数据
/// sector.seek(SeekFrom::Start(500))?;
/// sector.write_all(b"hello")?;
///
/// // 读取并验证
/// sector.seek(SeekFrom::Start(0))?;
/// let mut buf = vec![0u8; 4096];
/// sector.read_exact(&mut buf)?;
///
/// // 查询大小
/// let size = sector.seek(SeekFrom::End(0))?;
/// assert_eq!(size, 256 * 4096);
/// ```

```


### 10. Error - 错误类型

```rust
/// 签名错误位置
///
/// 标识 `InvalidSignature` 错误发生的具体模块/位置。
/// 用于替代此前为每个位置创建独立错误变体的设计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignaturePosition {
    FileTypeIdentifier,
    Header,
    RegionTable,
    MetadataTable,
    LogEntry,
    Descriptor,
    DataSector,
}

/// VHDX 操作错误类型
///
/// 涵盖 IO 错误、规范违反、数据损坏、逻辑错误等场景。
/// 所有 public API 通过 `crate::Result<T>`（即 `Result<T, Error>`）返回错误。
pub enum Error {
    /// 底层 IO 错误
    Io(std::io::Error),

    /// 无效的 VHDX 文件
    InvalidFile(String),

    /// 签名不匹配
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1/§4.2/§4.4/§4.5
    /// `expected` 和 `found` 统一使用 `[u8; 8]` 表示。
    /// 对于 4 字节签名（如 `"head"`、`"regi"` 等），放入低 4 字节，高 4 字节置零。
    /// 对于 8 字节签名（如 `"vhdxfile"`、`"metadata"`），直接填入全部 8 字节。
    /// 对应 CODE（由 position 区分）：
    /// - FileTypeIdentifier → HEADER_FILE_TYPE_ID_INVALID（MS-VHDX/2.2.1）
    /// - Header → HEADER_SIGNATURE_INVALID（MS-VHDX/2.2.2）
    /// - RegionTable → REGION_SIGNATURE_INVALID（MS-VHDX/2.2.3.1）
    /// - MetadataTable → METADATA_TABLE_SIGNATURE_INVALID（MS-VHDX/2.6.1.1）
    /// - LogEntry → LOG_SIGNATURE_INVALID（MS-VHDX/2.3.1.1）
    /// - Descriptor → LOG_DESCRIPTOR_SIGNATURE_INVALID（MS-VHDX/2.3.1）
    /// - DataSector → LOG_DATA_SECTOR_INVALID（MS-VHDX/2.3.1.4）
    InvalidSignature {
        position: SignaturePosition,
        expected: [u8; 8],
        found: [u8; 8],
    },

    /// 头部损坏
    CorruptedHeader(String),

    /// 双 Header LogGuid 不一致
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1
    /// 对应 CODE：HEADER_LOG_GUID_MISMATCH（MS-VHDX/2.2.2）
    /// 两个 VHDX Header 中的 LogGuid 字段值不一致。
    /// 规范要求 active header 的 LogGuid 用于日志有效性判定。
    HeaderLogGuidMismatch {
        header1_log_guid: Guid,
        header2_log_guid: Guid,
    },

    /// 双 Header 序列号异常，无法选择 active header
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1
    /// 对应 CODE：HEADER_SEQUENCE_NUMBER_INVALID（MS-VHDX/2.2.2）
    /// 当两个 Header 均有效但 SequenceNumber 相等时，无法确定 current header。
    HeaderSequenceNumberInvalid {
        sequence_number_1: u64,
        sequence_number_2: u64,
    },

    /// 不支持的 VHDX 版本
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1
    /// 对应 CODE：HEADER_VERSION_UNSUPPORTED（MS-VHDX/2.2.2）
    /// VHDX Version 字段 MUST be 1，不符合时实现 MUST NOT 继续处理。
    UnsupportedVersion {
        version: u16,
    },

    /// 不支持的 Log 版本
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1
    /// 对应 CODE：HEADER_LOG_VERSION_UNSUPPORTED（MS-VHDX/2.2.2）
    /// Header LogVersion 字段 MUST be 0。若 LogVersion != 0 且 LogGuid != 0，
    /// 实现 MUST NOT 继续处理（MS-VHDX §2.2.2）。
    UnsupportedLogVersion {
        version: u16,
    },

    /// CRC32C 校验和不匹配
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.1/§4.2/§4.5
    /// 对应 CODE（由上下文区分）：
    /// - Header → HEADER_CHECKSUM_MISMATCH（MS-VHDX/2.2.2）
    /// - Region Table → REGION_CHECKSUM_MISMATCH（MS-VHDX/2.2.3.1）
    /// - Log Entry → LOG_ENTRY_CHECKSUM_MISMATCH（MS-VHDX/2.3.1.1）
    InvalidChecksum {
        expected: u32,
        actual: u32,
    },

    /// 无效的 Payload Block 状态值
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 对应 CODE：BAT_ENTRY_INVALID_STATE（MS-VHDX/2.5.1.1）
    /// u8 参数为不合法原始状态值。
    InvalidBlockState(u8),

    /// 无效的 Sector Bitmap 块状态值
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 对应 CODE：BAT_SECTOR_BITMAP_INVALID_STATE（MS-VHDX/2.5.1.2）
    /// 合法的 Sector Bitmap 状态仅为 NotPresent(0) 和 Present(6)。
    InvalidSectorBitmapState(u8),

    /// BAT 状态值与磁盘类型不匹配
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 用于 BAT 条目状态值合法但与磁盘类型不兼容的场景，如：
    /// - 非差分盘上出现 PartiallyPresent（§2.5.1.1 规定仅在差分盘有效）
    /// - 非差分盘上 SectorBitmap 状态为 Present（非 NotPresent）
    /// 对应 CODE：BAT_ENTRY_STATE_MISMATCH（MS-VHDX/2.5.1.1）
    StateMismatch {
        state: u8,
        description: String,
    },

    /// BAT 条目文件偏移未按块大小对齐
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 对应 CODE：BAT_ENTRY_FILE_OFFSET_UNALIGNED（MS-VHDX/2.5）
    BatFileOffsetUnaligned {
        offset_mb: u64,
        block_size: u32,
    },

    /// BAT entry 数量不足以覆盖虚拟磁盘范围
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 对应 CODE：BAT_ENTRY_COUNT_INSUFFICIENT（MS-VHDX/2.5）
    /// actual 为实际 entry 数量，expected 为覆盖虚拟磁盘所需最小 entry 数量。
    BatEntryCountInsufficient {
        actual: u64,
        expected: u64,
    },

    /// BAT 中多个条目指向同一文件偏移
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.3
    /// 对应 CODE：BAT_FILE_OFFSET_DUPLICATE（MS-VHDX/2.5）
    /// 多个 BAT entry 映射到相同的 file_offset_mb，违反非重叠约束。
    BatFileOffsetDuplicate {
        offset_mb: u64,
    },

    /// 区域表格式错误
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.2
    /// 涵盖：entry 对齐/偏移最小值/区间重叠/格式异常
    /// 对应 CODE（由 message 区分）：
    /// - 对齐错误 → REGION_ENTRY_ALIGNMENT（MS-VHDX/2.2.3.2）
    /// - 偏移 < 1MB → REGION_ENTRY_OFFSET_MINIMUM（MS-VHDX/2.2.3.2）
    /// - 区间重叠 → REGION_ENTRY_OVERLAP（MS-VHDX/2.1）
    InvalidRegionTable(String),

    /// 未知的 required region
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.2 / MS-VHDX-宽松扩展标准.md §3
    /// 对应 CODE：REGION_REQUIRED_UNKNOWN（RELAX）
    /// Region Table 中存在实现无法识别且标记为 required 的条目。
    RegionRequiredUnknown {
        guid: Guid,
    },

    /// 未知的 optional region（严格模式下拒绝）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.2 / MS-VHDX-宽松扩展标准.md §3
    /// 对应 CODE：REGION_OPTIONAL_UNKNOWN（RELAX）
    /// strict=true 时，Region Table 中存在实现无法识别且未标记 required 的条目。
    /// strict=false 时，此类条目 MUST 被忽略，不会触发本错误。
    RegionOptionalUnknown {
        guid: Guid,
    },

    /// 元数据格式错误
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4
    /// 涵盖：Table Entry 格式异常/offset 越界/items 重叠/item 损坏
    /// 对应 CODE（由 message 区分）：
    /// - entry 异常 → METADATA_ENTRY_INVALID（MS-VHDX/2.6.1.2）
    /// - offset < 64KB → METADATA_ENTRY_OFFSET_MINIMUM（MS-VHDX/2.6.1.2）
    /// - items 重叠 → METADATA_ITEMS_OVERLAP（MS-VHDX/2.6.2）
    /// - item 损坏 → METADATA_ITEM_CORRUPTED（MS-VHDX/2.6.2）
    InvalidMetadata(String),

    /// 未知的 Metadata Item GUID
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4
    /// 对应 CODE：METADATA_GUID_UNKNOWN（MS-VHDX/2.6.2）
    /// Metadata Table 中存在实现无法识别的 Item GUID。
    MetadataGuidUnknown {
        guid: Guid,
    },

    /// required metadata item 缺失
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4 / MS-VHDX-宽松扩展标准.md §3
    /// 对应 CODE：METADATA_REQUIRED_MISSING（RELAX）
    /// 必须存在的 Metadata Item（如 FileParameters、VirtualDiskSize、VirtualDiskID）未找到。
    MetadataRequiredMissing {
        guid: Guid,
    },

    /// 未知的 required metadata item
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4 / MS-VHDX-宽松扩展标准.md §3
    /// 对应 CODE：METADATA_REQUIRED_UNKNOWN（RELAX）
    /// Metadata Table 中存在实现无法识别且标记为 required 的 Item GUID。
    MetadataRequiredUnknown {
        guid: Guid,
    },

    /// 未知的 optional metadata item（严格模式下拒绝）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4 / MS-VHDX-宽松扩展标准.md §3
    /// 对应 CODE：METADATA_OPTIONAL_UNKNOWN（RELAX）
    /// strict=true 时，Metadata Table 中存在实现无法识别且未标记 required 的 Item GUID。
    /// strict=false 时，此类条目 MUST 被忽略，不会触发本错误。
    MetadataOptionalUnknown {
        guid: Guid,
    },

    /// Metadata Entry 保留标志位被设置
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4
    /// 对应 CODE：METADATA_RESERVED_FLAGS_SET（MS-VHDX/2.6.1.2）
    /// Metadata Table Entry 中的保留位（非 Bit 29-31）被置 1。
    MetadataReservedFlagsSet {
        flags: u32,
    },

    /// 父定位器格式错误（ParentLocator 键值对解析失败）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.6
    /// 用于 Metadata::ParentLocator::KeyValueEntry 中 key/value 的 UTF-16LE 解码或数据切片越界场景。
    /// 与 InvalidMetadata 的区别：InvalidParentLocator 专指 ParentLocator 内部结构的解析错误，
    /// InvalidMetadata 涵盖 Metadata Table / Entry 的格式错误。
    /// 对应 CODE：PARENT_LOCATOR_FORMAT_ERROR（VALEXT，专用于 KeyValueEntry 解析失败）
    InvalidParentLocator(String),

    /// 元数据项未找到
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.4
    /// 专用于 MetadataTable::entry() 按 GUID 查找失败的场景，
    /// 与 MetadataRequiredMissing（required 项缺失）不同。
    MetadataNotFound {
        guid: Guid,
    },

    /// 需要日志回放
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.5
    /// 在打开路径（File::open → finish()）中，当策略为 Require 且存在可回放日志时，
    /// 此错误阻断打开流程，要求调用方显式设置日志回放策略。
    ///
    /// 注意：在校验路径（validate_log）中，存在可回放日志通常视为状态提示而非硬错误，
    /// 因此校验器中的 LOG_REPLAY_REQUIRED 为 status hint 语义。本 Error 变体专用于
    /// 打开路径的阻断语义。
    /// 对应 CODE：LOG_REPLAY_REQUIRED（ROEXT）
    LogReplayRequired,

    /// 日志条目损坏
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.5
    /// 涵盖：EntryLength 非法 / Tail 非法 / DescriptorCount 不匹配等
    /// 对应 CODE（由 message 区分）：
    /// - EntryLength 非法 → LOG_ENTRY_LENGTH_INVALID（MS-VHDX/2.3.1.1）
    /// - Tail 非法 → LOG_ENTRY_TAIL_INVALID（MS-VHDX/2.3.1.1）
    /// - 描述符数量不匹配 → LOG_DESCRIPTOR_COUNT_MISMATCH（MS-VHDX/2.3.1）
    LogEntryCorrupted(String),

    /// 日志 sequence 不连续
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.5
    /// 对应 CODE：LOG_SEQUENCE_GAP（MS-VHDX/2.3.2）
    /// active sequence 内相邻 log entry 的 SequenceNumber 不连续。
    LogSequenceGap {
        expected: u64,
        found: u64,
    },

    /// 日志 sequence GUID 不匹配
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.5
    /// 对应 CODE：LOG_SEQUENCE_GUID_MISMATCH（MS-VHDX/2.3.2）
    /// Log Entry 的 LogGuid 与 Header 的 LogGuid 不一致。
    LogSequenceGuidMismatch {
        entry_log_guid: Guid,
        header_log_guid: Guid,
    },

    /// 活跃 sequence 为空（日志损坏）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.5
    /// 对应 CODE：LOG_ACTIVE_SEQUENCE_EMPTY（MS-VHDX/2.3.3）
    /// 在日志中未找到有效的 active sequence。
    LogActiveSequenceEmpty,

    /// BAT 条目未找到（索引越界）
    ///
    /// 索引超出 BAT entry 数量范围时返回。
    BatEntryNotFound {
        index: u64,
    },

    /// 数据块未分配
    BlockNotPresent {
        block_idx: u64,
        state: String,
    },

    /// 扇区索引越界
    ///
    /// IO 数据面读取路径错误：请求的虚拟扇区范围超出虚拟磁盘大小。
    SectorOutOfBounds {
        sector: u64,
        max: u64,
    },

    /// 父磁盘未找到（三个路径均不可访问）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.6
    /// 对应 CODE：PARENT_LOCATOR_NO_VALID_PATH（MS-VHDX/2.6.2.6.3）
    /// 按规范顺序尝试 relative_path → volume_path → absolute_win32_path 后，
    /// 三个路径均无法访问或丢失。
    ParentNotFound,

    /// 父磁盘 GUID 不匹配
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.6
    /// 子盘 parent_linkage 记录的父盘 DataWriteGuid 与实际父盘不一致。
    /// 对应 CODE：PARENT_LOCATOR_GUID_MISMATCH（MS-VHDX/2.6.2.6）
    ParentMismatch {
        expected: Guid,
        actual: Guid,
    },

    /// parent_linkage key 不存在
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.6
    /// 对应 CODE：PARENT_LOCATOR_MISSING_LINKAGE（MS-VHDX/2.6.2.6.3）
    /// Parent Locator 中缺少 parent_linkage key。
    ParentLocatorMissingLinkage,

    /// parent_linkage2 存在（merge 过渡期冲突）
    ///
    /// 标准：docs/Standard/MS-VHDX-校验扩展标准.md §4.6
    /// 对应 CODE：PARENT_LOCATOR_LINKAGE2_CONFLICT（MS-VHDX/2.6.2.6.3）
    /// 本库不支持 merge 过渡期，parent_linkage2 的出现被视为格式冲突。
    ParentLocatorLinkage2Conflict,

    /// 参数无效
    InvalidParameter(String),

    /// 只读模式
    ReadOnly,
}
```

---

## 模块结构

```rust
// lib.rs - 公共 API 导出

pub mod section;

mod bat;
pub(crate) mod common;
mod error;
mod file;
mod header;
mod io;
mod log;
pub(crate) mod log_replay;
mod metadata;
mod sections;
mod types;
pub mod validation;

pub use error::{Error, Result, SignaturePosition};
pub use file::{CreateOptions, File, LogReplayPolicy, OpenOptions, ReadSemanticsPolicy};
pub use io::{IO, Sector};
pub use types::Guid;

// Section 模块
pub mod section {
    pub use sections::Sections;
    pub use header::{Header, FileTypeIdentifier, HeaderStructure, RegionTable, RegionTableHeader, RegionTableEntry};
    pub use bat::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};
    pub use metadata::{Metadata, MetadataTable, TableHeader, TableEntry, EntryFlags, MetadataItems, FileParameters, ParentLocator, LocatorHeader, KeyValueEntry, StandardItems};
    pub use log::{Log, Entry, LogEntryHeader, Descriptor, DataDescriptor, ZeroDescriptor, DataSector};
}

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
    if let Ok(fp) = metadata.items().file_parameters() {
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

    // 按需分项校验，每项返回 Vec<ValidationIssue>
    let header_issues = validator.validate_header()?;
    let region_issues = validator.validate_region_table()?;
    let bat_issues = validator.validate_bat()?;
    let metadata_issues = validator.validate_metadata()?;
    let required_issues = validator.validate_required_metadata_items()?;
    let log_issues = validator.validate_log()?;

    // 汇总输出所有校验问题
    let all_issues = header_issues.into_iter()
        .chain(region_issues)
        .chain(bat_issues)
        .chain(metadata_issues)
        .chain(required_issues)
        .chain(log_issues)
        .collect::<Vec<_>>();

    if all_issues.is_empty() {
        println!("All validations passed.");
    } else {
        for issue in &all_issues {
            println!("[{}] {}: {} ({})",
                issue.section(), issue.code(), issue.message(), issue.spec_ref());
        }
    }

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
    use std::io::Write;
    let io = file.io()?;
    let mut sector = io.sector(0, 1)?;
    let data = vec![0u8; 4096];
    sector.write_all(&data)?;
    
    // 验证创建的Metadata
    let metadata = file.sections().metadata()?;
    if let Ok(fp) = metadata.items().file_parameters() {
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
    if let Ok(fp) = metadata.items().file_parameters() {
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
    
    if let Ok(fp) = metadata.items().file_parameters() {
        if fp.has_parent() {
            println!("This is a differencing disk");
            println!("Block size: {}", fp.block_size());
            
            if let Ok(locator) = metadata.items().parent_locator() {
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
