# vhdx-rs 库 API 参考

> 本文档以 API 树的形式列出 `vhdx-rs` 库的全部公共 API。
>
> 基于 MS-VHDX 规范实现，支持 Fixed、Dynamic、Differencing 三种 VHDX 磁盘类型。

## API 树总览

```
vhdx_rs
├── File                          # VHDX 虚拟硬盘文件句柄
├── OpenOptions                   # 文件打开选项构建器
├── CreateOptions                 # 文件创建选项构建器
├── IO<'a>                        # 扇区/块级 IO 操作入口
├── Sector<'a>                    # 虚拟磁盘中的一个逻辑扇区
├── PayloadBlock<'a>              # 虚拟磁盘中的一个 Payload Block
├── Sections                      # 各区域的延迟加载容器
├── SectionsConfig                # Sections 初始化配置
├── Guid                          # 128 位全局唯一标识符
├── Error                         # 统一错误类型（枚举）
├── Result<T>                     # 统一结果类型别名
├── crc32c_with_zero_field()      # CRC32C 校验和辅助函数
├── Header                        # 头部区域的统一访问接口
├── FileTypeIdentifier<'a>        # 文件类型标识符（§2.2.1）
├── HeaderStructure<'a>           # 头部结构（§2.2.2）
├── RegionTable<'a>               # 区域表（§2.2.3）
├── RegionTableHeader<'a>         # 区域表头部（§2.2.3.1）
├── RegionTableEntry<'a>          # 区域表条目（§2.2.3.2）
├── Bat                           # 块分配表（BAT）（§2.5）
├── BatEntry                      # BAT 条目（§2.5.1）
├── BatEntryIter<'a>              # BAT 条目迭代器
├── BatState                      # BAT 条目块状态枚举
├── PayloadBlockState             # Payload Block 状态枚举（§2.5.1.1）
├── SectorBitmapState             # Sector Bitmap Block 状态枚举（§2.5.1.2）
├── Log (sections)                # 日志区域包装类型（§2.3）
├── LogEntry<'a>                  # 日志条目（§2.3.1）
├── LogEntryHeader<'a>            # 日志条目头部（§2.3.1.1）
├── Descriptor<'a>                # 日志描述符（§2.3.1.2/§2.3.1.3）
├── DataDescriptor<'a>            # 数据描述符（§2.3.1.3）
├── ZeroDescriptor<'a>            # 零描述符（§2.3.1.2）
├── DataSector<'a>                # 数据扇区（§2.3.1.4）
├── Metadata (sections)           # 元数据区域包装类型（§2.6）
├── MetadataTable<'a>             # 元数据表（§2.6.1）
├── TableHeader<'a>               # 元数据表头部（§2.6.1.1）
├── TableEntry<'a>                # 元数据表项（§2.6.1.2）
├── EntryFlags                    # 元数据表项标志位（§2.6.1.2）
├── MetadataItems<'a>             # 类型化元数据访问器（§2.6.2）
├── FileParameters                # 文件参数元数据（§2.6.2.1）
├── ParentLocator<'a>             # 父磁盘定位器（§2.6.2.6）
├── LocatorHeader<'a>             # 父磁盘定位器头部（§2.6.2.6.1）
└── KeyValueEntry                 # 父磁盘定位器键值对条目（§2.6.2.6.2）
```

---

## 详细 API

### `File` — VHDX 虚拟硬盘文件句柄

顶层操作入口，提供打开、创建、读写虚拟磁盘数据的能力。

```
File
├── open(path: impl AsRef<Path>) -> OpenOptions
│   └── 创建打开选项构建器，用于打开现有 VHDX 文件
├── create(path: impl AsRef<Path>) -> CreateOptions
│   └── 创建选项构建器，用于创建新的 VHDX 文件
├── sections(&self) -> &Sections
│   └── 获取 VHDX 区域容器的引用
├── io(&self) -> IO<'_>
│   └── 获取扇区/块级 IO 操作接口
├── inner(&self) -> &StdFile
│   └── 获取底层操作系统文件句柄的引用
├── virtual_disk_size(&self) -> u64
│   └── 获取虚拟磁盘大小（字节）
├── block_size(&self) -> u32
│   └── 获取块大小（字节）
├── logical_sector_size(&self) -> u32
│   └── 获取逻辑扇区大小（字节）
├── is_fixed(&self) -> bool
│   └── 检查是否为 Fixed 类型
├── has_parent(&self) -> bool
│   └── 检查是否为差分磁盘
├── has_pending_logs(&self) -> bool
│   └── 检查是否存在未回放的日志条目
├── read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>
│   └── 从虚拟磁盘读取数据
├── write(&mut self, offset: u64, data: &[u8]) -> Result<usize>
│   └── 向虚拟磁盘写入数据
└── flush(&mut self) -> Result<()>
    └── 将所有挂起的写入刷新到磁盘
```

### `OpenOptions` — 文件打开选项构建器

Builder 模式，通过 `File::open()` 获取实例。

```
OpenOptions
├── write(self) -> Self
│   └── 设置以写入模式打开文件
└── finish(self) -> Result<File>
    └── 完成选项配置并打开 VHDX 文件
```

### `CreateOptions` — 文件创建选项构建器

Builder 模式，通过 `File::create()` 获取实例。

```
CreateOptions
├── size(self, size: u64) -> Self
│   └── 设置虚拟磁盘大小（字节），必填参数
├── fixed(self, fixed: bool) -> Self
│   └── 设置是否创建 Fixed 类型的虚拟磁盘
├── has_parent(self, has_parent: bool) -> Self
│   └── 设置是否为差分磁盘（具有父磁盘引用）
├── block_size(self, block_size: u32) -> Self
│   └── 设置块大小（字节），默认 32MB
└── finish(self) -> Result<File>
    └── 完成选项配置并创建 VHDX 文件
```

---

### `IO<'a>` — 扇区/块级 IO 操作入口

通过 `File::io()` 获取实例。

```
IO<'a>
├── new(file: &'a File) -> Self
│   └── 从 VHDX 文件引用创建 IO 实例
├── sector(&self, sector: u64) -> Option<Sector<'a>>
│   └── 获取指定逻辑扇区号的扇区对象，超出范围返回 None
├── read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<usize>
│   └── 批量读取连续扇区数据到缓冲区
└── write_sectors(&self, start_sector: u64, data: &[u8]) -> Result<usize>
    └── 批量写入连续扇区（当前未完全实现）
```

### `Sector<'a>` — 虚拟磁盘中的一个逻辑扇区

```
Sector<'a>
├── block_idx(&self) -> u64
│   └── 获取所属 Payload Block 索引
├── block_sector_idx(&self) -> u32
│   └── 获取在所属 Payload Block 内的扇区索引
├── global_sector(&self) -> u64
│   └── 计算全局扇区号
├── read(&self, buf: &mut [u8]) -> Result<usize>
│   └── 读取扇区数据到缓冲区
└── payload(&self) -> PayloadBlock<'_>
    └── 获取此扇区所属的 Payload Block
```

### `PayloadBlock<'a>` — 虚拟磁盘中的一个 Payload Block

```
PayloadBlock<'a>
├── block_idx(&self) -> u64
│   └── 获取 Block 索引
├── read(&self, offset: u64, buf: &mut [u8]) -> Result<usize>
│   └── 从 Block 的指定偏移量读取数据
├── bat_entry(&self) -> Option<BatEntry>
│   └── 获取此 Block 的 BAT 条目
└── is_allocated(&self) -> bool
    └── 检查此 Block 是否已分配（FullyPresent 状态）
```

---

### `Sections` — 各区域的延迟加载容器

使用 `RefCell<Option<T>>` 模式，首次访问时从文件读取并缓存。

```
Sections
├── new(config: SectionsConfig) -> Self
│   └── 从配置创建 Sections 实例
├── header(&self) -> Result<Ref<'_, Header>>
│   └── 获取头部区域（延迟加载）
├── bat(&self) -> Result<Ref<'_, Bat>>
│   └── 获取 BAT 区域（延迟加载）
├── metadata(&self) -> Result<Ref<'_, Metadata>>
│   └── 获取元数据区域（延迟加载）
└── log(&self) -> Result<Ref<'_, Log>>
    └── 获取日志区域（延迟加载）
```

### `SectionsConfig` — Sections 初始化配置

```
SectionsConfig
├── file: std::fs::File           # VHDX 文件句柄
├── bat_offset: u64               # BAT 区域偏移量
├── bat_size: u64                 # BAT 区域大小
├── metadata_offset: u64          # 元数据区域偏移量
├── metadata_size: u64            # 元数据区域大小
├── log_offset: u64               # 日志区域偏移量
├── log_size: u64                 # 日志区域大小
└── entry_count: u64              # BAT 条目总数
```

---

### `Guid` — 128 位全局唯一标识符

```
Guid
├── from_bytes(data: [u8; 16]) -> Self
│   └── 从 16 字节数组创建 GUID
├── as_bytes(&self) -> &[u8; 16]
│   └── 返回 GUID 的 16 字节原始数据引用
├── nil() -> Self
│   └── 创建全零（空）GUID
├── is_nil(&self) -> bool
│   └── 检查是否为全零 GUID
├── impl Debug                     # XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX（混合字节序）
├── impl Display                   # 同 Debug
├── impl From<[u8; 16]>            # 从字节数组转换
├── impl From<uuid::Uuid>          # 从 uuid crate 的 Uuid 转换
├── impl Into<uuid::Uuid>          # 转换为 uuid crate 的 Uuid
└── impl Default                   # 默认值为全零 GUID
```

---

### `Error` — 统一错误类型

所有变体均通过 `thiserror` 实现 `std::error::Error` trait。

```
Error (enum)
├── Io(std::io::Error)                           # 底层 IO 错误
├── FileLocked                                    # 文件被其他进程锁定
├── InvalidFile(String)                           # 无效的 VHDX 文件
├── CorruptedHeader(String)                       # 头部结构损坏（§2.2.2）
├── InvalidChecksum { expected: u32, actual: u32 } # 校验和验证失败
├── UnsupportedVersion(u16)                       # 不支持的 VHDX 版本
├── InvalidBlockState(u8)                         # 无效的块状态值（§2.5.1）
├── ParentNotFound { path: PathBuf }              # 父磁盘未找到（§2.6.2.6）
├── ParentMismatch { expected: Guid, actual: Guid } # 父磁盘不匹配
├── LogReplayRequired                             # 需要日志回放（§2.3.3）
├── InvalidParameter(String)                      # 无效的参数
├── MetadataNotFound { guid: Guid }               # 元数据项未找到（§2.6.1）
├── ReadOnly                                      # 文件为只读模式
├── InvalidSignature { expected: String, found: String } # 无效的签名
├── BatEntryNotFound { index: u64 }               # BAT 条目未找到
├── InvalidRegionTable(String)                    # 无效的区域表（§2.2.3）
├── InvalidMetadata(String)                       # 无效的元数据（§2.6）
├── LogEntryCorrupted(String)                     # 日志条目损坏（§2.3.1）
├── SectorOutOfBounds { sector: u64, max: u64 }   # 扇区索引超出范围
└── BlockNotPresent { block_idx: u64, state: String } # 数据块未分配（§2.5.1.1）
```

### `Result<T>` — 统一结果类型别名

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

---

### `crc32c_with_zero_field` — CRC32C 校验和辅助函数

```
crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32
└── 计算 CRC32C 校验和，计算前将指定偏移范围的字节置零
```

---

### `Header` — 头部区域的统一访问接口

包装 1MB 头部区域的原始数据。

```
Header
├── new(data: Vec<u8>) -> Result<Self>
│   └── 从原始数据创建，验证长度必须为 1MB
├── raw(&self) -> &[u8]
│   └── 返回原始字节数据
├── file_type(&self) -> FileTypeIdentifier<'_>
│   └── 获取文件类型标识符（§2.2.1）
├── header(&self, index: usize) -> Option<HeaderStructure<'_>>
│   └── 获取头部结构（index=0: 活动, 1: Header1, 2: Header2）
└── region_table(&self, index: usize) -> Option<RegionTable<'_>>
    └── 获取区域表（index=0|1: 表1, 2: 表2）
```

### `FileTypeIdentifier<'a>` — 文件类型标识符（§2.2.1）

```
FileTypeIdentifier<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── signature(&self) -> &[u8]            # 8 字节签名 "vhdxfile"
├── creator(&self) -> String             # UTF-16 LE 创建者字符串
└── create(creator: Option<&str>) -> Vec<u8>  # 构造新的标识符数据（64KB）
```

### `HeaderStructure<'a>` — 头部结构（§2.2.2）

```
HeaderStructure<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── signature(&self) -> &[u8]            # 签名 "head"
├── checksum(&self) -> u32               # CRC32C 校验和
├── verify_checksum(&self) -> Result<()> # 验证 CRC32C 校验和
├── sequence_number(&self) -> u64        # 序列号（确定活动头部）
├── file_write_guid(&self) -> Guid       # 文件写入 GUID
├── data_write_guid(&self) -> Guid       # 数据写入 GUID
├── log_guid(&self) -> Guid              # 日志 GUID
├── log_version(&self) -> u16            # 日志版本（= 0）
├── version(&self) -> u16                # VHDX 版本（= 1）
├── log_length(&self) -> u32             # 日志区域长度
├── log_offset(&self) -> u64             # 日志区域偏移量
└── create(sequence_number, file_write_guid, data_write_guid,
│         log_guid, log_length, log_offset) -> Vec<u8>
    └── 构造新的头部结构数据（4KB），自动计算 CRC32C
```

### `RegionTable<'a>` — 区域表（§2.2.3）

```
RegionTable<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── header(&self) -> RegionTableHeader<'_>
├── entry(&self, index: u32) -> Option<RegionTableEntry<'_>>
├── entries(&self) -> Vec<RegionTableEntry<'_>>
└── find_entry(&self, guid: &Guid) -> Option<RegionTableEntry<'_>>
```

### `RegionTableHeader<'a>` — 区域表头部（§2.2.3.1）

```
RegionTableHeader<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── signature(&self) -> &[u8]            # 签名 "regi"
├── checksum(&self) -> u32               # CRC32C 校验和
├── verify_checksum(&self) -> Result<()> # 验证 CRC32C 校验和
└── entry_count(&self) -> u32            # 区域条目数量
```

### `RegionTableEntry<'a>` — 区域表条目（§2.2.3.2）

```
RegionTableEntry<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── guid(&self) -> Guid                  # 区域 GUID
├── file_offset(&self) -> u64            # 区域偏移量（字节）
├── length(&self) -> u32                 # 区域长度（字节）
└── required(&self) -> bool              # 是否为必需区域
```

---

### `Bat` — 块分配表（§2.5）

```
Bat
├── new(data: Vec<u8>, entry_count: u64) -> Result<Self>
├── raw(&self) -> &[u8]
├── entry(&self, index: usize) -> Option<BatEntry>
├── entries(&self) -> BatEntryIter<'_>
├── len(&self) -> usize
├── is_empty(&self) -> bool
├── calculate_chunk_ratio(logical_sector_size: u32, block_size: u32) -> u32
├── calculate_payload_blocks(virtual_disk_size: u64, block_size: u32) -> u64
├── calculate_sector_bitmap_blocks(payload_blocks: u64, chunk_ratio: u32) -> u64
└── calculate_total_entries(virtual_disk_size: u64, block_size: u32,
    │                            logical_sector_size: u32) -> u64
```

### `BatEntry` — BAT 条目（§2.5.1）

```
BatEntry
├── state: BatState                     # 块状态（公共字段）
├── file_offset_mb: u64                 # 文件偏移量 MB（公共字段）
├── from_raw(raw: u64) -> Result<Self>
├── raw(&self) -> u64
├── file_offset(&self) -> u64           # 偏移量（字节）
└── new(state: BatState, file_offset_mb: u64) -> Self
```

### `BatEntryIter<'a>` — BAT 条目迭代器

> **注意**: 此类型未通过 `pub use` 导出，外部用户无法直接按名导入。
> 仅作为 `Bat::entries()` 的返回类型使用，通过类型推断即可。

```
BatEntryIter<'a> (impl Iterator<Item = (usize, BatEntry)>)
```

### `BatState` — BAT 条目块状态枚举

```
BatState (enum)
├── Payload(PayloadBlockState)          # Payload Block 状态
├── SectorBitmap(SectorBitmapState)     # Sector Bitmap Block 状态
├── from_bits(bits: u8) -> Result<Self>
└── to_bits(&self) -> u8
```

### `PayloadBlockState` — Payload Block 状态枚举（§2.5.1.1）

```
PayloadBlockState (enum)
├── NotPresent      = 0   # 块不存在，读取返回零
├── Undefined       = 1   # 块未定义，不应依赖内容
├── Zero            = 2   # 块内容为零
├── Unmapped        = 3   # 已 UNMAP（TRIM 释放）
├── FullyPresent    = 6   # 块数据完全存在
├── PartiallyPresent = 7  # 块数据部分存在（仅差分 VHDX）
├── from_bits(bits: u8) -> Self
├── to_bits(&self) -> u8
├── is_allocated(&self) -> bool          # FullyPresent 或 PartiallyPresent
└── needs_read(&self) -> bool            # 是否需要实际 I/O
```

### `SectorBitmapState` — Sector Bitmap Block 状态枚举（§2.5.1.2）

```
SectorBitmapState (enum)
├── NotPresent = 0   # 扇区位图不存在
├── Present    = 6   # 扇区位图存在
├── from_bits(bits: u8) -> Self
└── to_bits(&self) -> u8
```

---

### `Log` (sections) — 日志区域包装类型（§2.3）

```
Log
├── new(data: Vec<u8>) -> Self
├── raw(&self) -> &[u8]
├── entry(&self, index: usize) -> Option<LogEntry<'_>>
├── entries(&self) -> Vec<LogEntry<'_>>
├── is_replay_required(&self) -> bool
└── replay(&self, file: &mut std::fs::File) -> Result<()>
```

### `LogEntry<'a>` — 日志条目（§2.3.1）

```
LogEntry<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── header(&self) -> LogEntryHeader<'_>
├── descriptor(&self, index: usize) -> Option<Descriptor<'_>>
├── descriptors(&self) -> Vec<Descriptor<'_>>
└── data(&self) -> Vec<DataSector<'_>>
```

### `LogEntryHeader<'a>` — 日志条目头部（§2.3.1.1）

```
LogEntryHeader<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── signature(&self) -> &[u8]            # 签名 "loge"
├── checksum(&self) -> u32               # CRC32C 校验和
├── entry_length(&self) -> u32           # 条目总长度
├── tail(&self) -> u32                   # 尾部偏移量
├── sequence_number(&self) -> u64        # 序列号
├── descriptor_count(&self) -> u32       # 描述符数量
├── log_guid(&self) -> Guid              # 日志 GUID
├── flushed_file_offset(&self) -> u64    # 已刷写的文件偏移量
└── last_file_offset(&self) -> u64       # 最后写入的文件偏移量
```

### `Descriptor<'a>` — 日志描述符（§2.3.1.2/§2.3.1.3）

```
Descriptor<'a> (enum)
├── Data(DataDescriptor<'a>)             # 数据描述符 — 写入数据
├── Zero(ZeroDescriptor<'a>)             # 零描述符 — 写入零填充
├── parse(data: &'a [u8]) -> Result<Self>
└── raw(&self) -> &[u8]
```

### `DataDescriptor<'a>` — 数据描述符（§2.3.1.3）

```
DataDescriptor<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── trailing_bytes(&self) -> u32         # 数据前的零字节数
├── leading_bytes(&self) -> u64          # 数据后的零字节数
├── file_offset(&self) -> u64            # 目标文件写入偏移量
└── sequence_number(&self) -> u64        # 序列号
```

### `ZeroDescriptor<'a>` — 零描述符（§2.3.1.2）

```
ZeroDescriptor<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── zero_length(&self) -> u64            # 零填充长度
├── file_offset(&self) -> u64            # 目标文件写入偏移量
└── sequence_number(&self) -> u64        # 序列号
```

### `DataSector<'a>` — 数据扇区（§2.3.1.4）

```
DataSector<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── sequence_high(&self) -> u32          # 序列号高 32 位
├── data(&self) -> &[u8]                 # 实际数据内容（字节 8-4092）
├── sequence_low(&self) -> u32           # 序列号低 32 位
└── sequence_number(&self) -> u64        # 组合序列号（撕裂写入检测）
```

---

### `Metadata` (sections) — 元数据区域包装类型（§2.6）

```
Metadata
├── new(data: Vec<u8>) -> Result<Self>
├── raw(&self) -> &[u8]
├── table(&self) -> MetadataTable<'_>
└── items(&self) -> MetadataItems<'_>
```

### `MetadataTable<'a>` — 元数据表（§2.6.1）

```
MetadataTable<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── header(&self) -> TableHeader<'_>
├── entry(&self, item_id: &Guid) -> Option<TableEntry<'_>>
└── entries(&self) -> Vec<TableEntry<'_>>
```

### `TableHeader<'a>` — 元数据表头部（§2.6.1.1）

```
TableHeader<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── signature(&self) -> &[u8]            # 签名 "metadata"
└── entry_count(&self) -> u16            # 表项数量
```

### `TableEntry<'a>` — 元数据表项（§2.6.1.2）

```
TableEntry<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── item_id(&self) -> Guid               # 元数据项 GUID
├── offset(&self) -> u32                 # 数据偏移量
├── length(&self) -> u32                 # 数据长度
└── flags(&self) -> EntryFlags           # 属性标志位
```

### `EntryFlags` — 元数据表项标志位（§2.6.1.2）

```
EntryFlags(u32)
├── is_user(&self) -> bool               # bit 31 — 用户自定义元数据
├── is_virtual_disk(&self) -> bool       # bit 30 — 虚拟磁盘相关
└── is_required(&self) -> bool           # bit 29 — 必需项
```

### `MetadataItems<'a>` — 类型化元数据访问器（§2.6.2）

```
MetadataItems<'a>
├── new(metadata: &'a Metadata) -> Self
├── file_parameters(&self) -> Option<FileParameters>
│   └── 文件参数（§2.6.2.1）
├── virtual_disk_size(&self) -> Option<u64>
│   └── 虚拟磁盘大小（§2.6.2.2）
├── virtual_disk_id(&self) -> Option<Guid>
│   └── 虚拟磁盘标识符（§2.6.2.3）
├── logical_sector_size(&self) -> Option<u32>
│   └── 逻辑扇区大小（§2.6.2.4）
├── physical_sector_size(&self) -> Option<u32>
│   └── 物理扇区大小（§2.6.2.5）
└── parent_locator(&self) -> Option<ParentLocator<'_>>
    └── 父磁盘定位器（§2.6.2.6，仅差分磁盘）
```

### `FileParameters` — 文件参数元数据（§2.6.2.1）

```
FileParameters
├── from_bytes(data: &[u8]) -> Self
├── block_size(&self) -> u32             # 块大小（1MB-256MB）
├── leave_block_allocated(&self) -> bool # bit 0 — 保留块空间
├── has_parent(&self) -> bool            # bit 1 — 差分磁盘
└── flags(&self) -> u32                  # 原始标志位值
```

### `ParentLocator<'a>` — 父磁盘定位器（§2.6.2.6）

```
ParentLocator<'a>
├── new(data: &'a [u8]) -> Result<Self>
├── raw(&self) -> &[u8]
├── header(&self) -> LocatorHeader<'_>
├── entry(&self, index: usize) -> Option<KeyValueEntry>
├── entries(&self) -> Vec<KeyValueEntry>
└── key_value_data(&self) -> &[u8]       # UTF-16 LE 键值对数据区域
```

### `LocatorHeader<'a>` — 父磁盘定位器头部（§2.6.2.6.1）

```
LocatorHeader<'a>
├── new(data: &'a [u8]) -> Self
├── raw(&self) -> &[u8]
├── locator_type(&self) -> Guid          # 定位器类型 GUID
└── key_value_count(&self) -> u16        # 键值对条目数量
```

### `KeyValueEntry` — 父磁盘定位器键值对条目（§2.6.2.6.2）

```
KeyValueEntry
├── new(data: &[u8]) -> Result<Self>
├── raw(&self) -> [u8; 12]
├── key(&self, data: &[u8]) -> Option<String>    # 读取键字符串
└── value(&self, data: &[u8]) -> Option<String>  # 读取值字符串
```

---

## 常量

### 文件布局常量

| 常量 | 类型 | 值 | 说明 |
|------|------|----|------|
| `KiB` | `u64` | 1024 | 千字节 |
| `MiB` | `u64` | 1048576 | 兆字节 |
| `HEADER_SECTION_SIZE` | `usize` | 1MB | 头部区域总大小（§2.2） |
| `FILE_TYPE_SIZE` | `usize` | 64KB | 文件类型标识符大小（§2.2.1） |
| `HEADER_1_OFFSET` | `usize` | 64KB | Header 1 偏移量 |
| `HEADER_2_OFFSET` | `usize` | 128KB | Header 2 偏移量 |
| `HEADER_SIZE` | `usize` | 4KB | 单个头部结构大小（§2.2.2） |
| `REGION_TABLE_1_OFFSET` | `usize` | 192KB | 区域表 1 偏移量 |
| `REGION_TABLE_2_OFFSET` | `usize` | 256KB | 区域表 2 偏移量 |
| `REGION_TABLE_SIZE` | `usize` | 64KB | 区域表大小（§2.2.3） |
| `METADATA_TABLE_SIZE` | `usize` | 64KB | 元数据表大小（§2.6.1） |
| `BAT_ENTRY_SIZE` | `usize` | 8 | BAT 条目大小（§2.5.1） |
| `LOGICAL_SECTOR_SIZE_512` | `u32` | 512 | 默认逻辑扇区大小 |
| `DEFAULT_BLOCK_SIZE` | `u32` | 32MB | 默认块大小 |
| `MIN_BLOCK_SIZE` | `u32` | 1MB | 最小块大小（§2.6.2.1） |
| `MAX_BLOCK_SIZE` | `u32` | 256MB | 最大块大小（§2.6.2.1） |
| `CHUNK_RATIO_CONSTANT` | `u64` | 2^23 | 块比率常量（§2.5） |
| `LOG_ENTRY_HEADER_SIZE` | `usize` | 64 | 日志条目头部大小（§2.3.1.1） |
| `DATA_SECTOR_SIZE` | `usize` | 4KB | 数据扇区大小（§2.3.1.4） |
| `DESCRIPTOR_SIZE` | `usize` | 32 | 描述符大小 |
| `VHDX_VERSION` | `u16` | 1 | VHDX 格式版本号 |
| `LOG_VERSION` | `u16` | 0 | 日志版本号 |

### 签名常量

| 常量 | 类型 | 值 | 说明 |
|------|------|----|------|
| `FILE_TYPE_SIGNATURE` | `&[u8; 8]` | `b"vhdxfile"` | 文件类型签名（§2.2.1） |
| `HEADER_SIGNATURE` | `&[u8; 4]` | `b"head"` | 头部签名（§2.2.2） |
| `REGION_TABLE_SIGNATURE` | `&[u8; 4]` | `b"regi"` | 区域表签名（§2.2.3） |
| `METADATA_SIGNATURE` | `&[u8; 8]` | `b"metadata"` | 元数据表签名（§2.6.1.1） |
| `LOG_ENTRY_SIGNATURE` | `&[u8; 4]` | `b"loge"` | 日志条目签名（§2.3.1.1） |
| `DATA_DESCRIPTOR_SIGNATURE` | `&[u8; 4]` | `b"desc"` | 数据描述符签名（§2.3.1.3） |
| `ZERO_DESCRIPTOR_SIGNATURE` | `&[u8; 4]` | `b"zero"` | 零描述符签名（§2.3.1.2） |

### 区域 GUID 常量（`constants::region_guids::*`）

| 常量 | 类型 | 说明 |
|------|------|------|
| `BAT_REGION` | `Guid` | 块分配表区域 GUID（§2.2.3.2） |
| `METADATA_REGION` | `Guid` | 元数据区域 GUID（§2.2.3.2） |

### 元数据项 GUID 常量（`constants::metadata_guids::*`）

| 常量 | 类型 | 说明 |
|------|------|------|
| `FILE_PARAMETERS` | `Guid` | 文件参数（§2.6.2.1） |
| `VIRTUAL_DISK_SIZE` | `Guid` | 虚拟磁盘大小（§2.6.2.2） |
| `VIRTUAL_DISK_ID` | `Guid` | 虚拟磁盘标识符（§2.6.2.3） |
| `LOGICAL_SECTOR_SIZE` | `Guid` | 逻辑扇区大小（§2.6.2.4） |
| `PHYSICAL_SECTOR_SIZE` | `Guid` | 物理扇区大小（§2.6.2.5） |
| `PARENT_LOCATOR` | `Guid` | 父磁盘定位器（§2.6.2.6） |

### 辅助函数

| 函数 | 签名 | 说明 |
|------|------|------|
| `align_up` | `(value: u64, alignment: u64) -> u64` | 向上对齐到 alignment 的整数倍 |
| `align_1mib` | `(value: u64) -> u64` | 向上对齐到 1MB 边界 |

---

## Trait 实现

### `Guid`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `PartialEq` | 相等比较 |
| `Eq` | 全等比较 |
| `Hash` | 哈希 |
| `Debug` | `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX` 格式 |
| `Display` | 同 Debug |
| `Default` | 全零 GUID |
| `From<[u8; 16]>` | 从字节数组转换 |
| `From<uuid::Uuid>` | 从 uuid 转换 |
| `Into<uuid::Uuid>` | 转换为 uuid |

### `BatEntry`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |
| `PartialEq` | 相等比较 |

### `BatState`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |
| `PartialEq` | 相等比较 |
| `Eq` | 全等比较 |

### `PayloadBlockState`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |
| `PartialEq` | 相等比较 |
| `Eq` | 全等比较 |

### `SectorBitmapState`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |
| `PartialEq` | 相等比较 |
| `Eq` | 全等比较 |

### `EntryFlags`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |

### `KeyValueEntry`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |

### `FileParameters`

| Trait | 说明 |
|-------|------|
| `Clone` | 值复制 |
| `Copy` | 值语义 |
| `Debug` | 调试输出 |

### `Error`

| Trait | 说明 |
|-------|------|
| `Error` (std) | thiserror 派生 |
| `Debug` | 调试输出 |
