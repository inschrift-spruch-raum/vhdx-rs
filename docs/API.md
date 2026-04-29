# vhdx-rs API Reference

本文档只描述当前 `src/lib.rs` 实际导出面。
目标是让文档和代码保持一一对应。

## Root exports (`vhdx_rs::*`)

```text
vhdx_rs::
├── File
├── OpenOptions
├── CreateOptions
├── LogReplayPolicy
├── ParentChainInfo
├── IO<'a>
├── Sector<'a>
├── PayloadBlock<'a>
├── SectionsConfig
├── Guid
├── Error
├── Result<T>
├── SpecValidator<'a>
├── ValidationIssue
├── crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32
├── mod section
│   ├── Sections<'a>
│   ├── Header<'a>
│   ├── HeaderStructure<'a>
│   ├── RegionTable<'a>
│   ├── RegionTableHeader<'a>
│   ├── RegionTableEntry<'a>
│   ├── Bat<'a>
│   ├── BatEntry
│   ├── BatState
│   ├── PayloadBlockState
│   ├── SectorBitmapState
│   ├── Metadata<'a>
│   ├── MetadataTable<'a>
│   ├── TableHeader<'a>
│   ├── TableEntry<'a>
│   ├── EntryFlags
│   ├── MetadataItems<'a>
│   ├── FileParameters<'a>
│   ├── ParentLocator<'a>
│   ├── LocatorHeader<'a>
│   ├── KeyValueEntry<'a>
│   ├── Log<'a>
│   ├── LogEntry<'a>
│   ├── Entry<'a> = LogEntry<'a>
│   ├── LogEntryHeader<'a>
│   ├── Descriptor<'a>
│   ├── DataDescriptor<'a>
│   ├── ZeroDescriptor<'a>
│   └── DataSector<'a>
└── mod constants
    └── pub use crate::common::constants::*
```

## Key signatures

### `File`

```rust
impl File {
    pub fn open(path: impl AsRef<Path>) -> OpenOptions;
    pub fn create(path: impl AsRef<Path>) -> CreateOptions;
    pub const fn sections(&self) -> &Sections<'_>;
    pub const fn io(&self) -> IO<'_>;
    pub const fn inner(&self) -> &std::fs::File;
    pub fn validator(&self) -> vhdx_rs::validation::SpecValidator<'_>;

    pub const fn virtual_disk_size(&self) -> u64;
    pub const fn block_size(&self) -> u32;
    pub const fn logical_sector_size(&self) -> u32;
    pub const fn is_fixed(&self) -> bool;
    pub const fn has_parent(&self) -> bool;
    pub const fn has_pending_logs(&self) -> bool;
}
```

### `OpenOptions`

```rust
impl OpenOptions {
    pub const fn write(self) -> Self;
    pub const fn strict(self, strict: bool) -> Self;
    pub const fn log_replay(self, policy: LogReplayPolicy) -> Self;
    pub fn finish(self) -> Result<File>;
}
```

### `LogReplayPolicy`

`LogReplayPolicy` 的严格路径是 `Require`、`Auto`、`InMemoryOnReadOnly`。
`ReadOnlyNoReplay` 被保留为**兼容模式例外**，用于只读诊断场景。

需要明确的是，`ReadOnlyNoReplay` **不是**严格 MS-VHDX 一致性路径。
当文件存在 pending log 时，该策略会保留 pending 状态，不触发回放写入。

### `CreateOptions`

```rust
impl CreateOptions {
    pub const fn size(self, size: u64) -> Self;
    pub const fn fixed(self, fixed: bool) -> Self;
    pub const fn block_size(self, block_size: u32) -> Self;
    pub const fn logical_sector_size(self, logical_sector_size: u32) -> Self;
    pub const fn physical_sector_size(self, physical_sector_size: u32) -> Self;
    pub fn parent_path(self, path: impl AsRef<Path>) -> Self;
    pub fn finish(self) -> Result<File>;
}
```

### `IO<'a>`, `Sector<'a>`, `PayloadBlock<'a>`

```rust
impl<'a> IO<'a> {
    pub const fn new(file: &'a File) -> Self;
    pub fn sector(&self, sector: u64) -> Option<Sector<'a>>;
}

impl Sector<'_> {
    pub fn read(&self, buf: &mut [u8]) -> Result<usize>;
    pub fn write(&self, data: &[u8]) -> Result<()>;
    pub fn payload(&self) -> PayloadBlock<'_>;
}

pub struct PayloadBlock<'a> {
    pub bytes: &'a [u8],
}
```

## Section path details

### `section::Sections<'a>`

```rust
impl<'a> Sections<'a> {
    pub fn new(config: SectionsConfig) -> Self;
    pub fn header(&self) -> Result<std::cell::Ref<'_, Header<'a>>>;
    pub fn bat(&self) -> Result<std::cell::Ref<'_, Bat<'a>>>;
    pub fn metadata(&self) -> Result<std::cell::Ref<'_, Metadata<'a>>>;
    pub fn log(&self) -> Result<std::cell::Ref<'_, Log<'a>>>;
}
```

### `section::RegionTable<'a>` public shape

```rust
pub struct RegionTable<'a> {
    pub header: RegionTableHeader<'a>,
    pub entries: Vec<RegionTableEntry<'a>>,
}

impl<'a> RegionTable<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self>;
    pub const fn raw(&self) -> &[u8];
    pub fn header(&self) -> RegionTableHeader<'_>;
    pub fn entry(&self, index: u32) -> Option<RegionTableEntry<'_>>;
    pub fn entries(&self) -> Vec<RegionTableEntry<'_>>;
    pub fn find_entry(&self, guid: &Guid) -> Option<RegionTableEntry<'_>>;
}
```

### `section::FileParameters<'a>` public shape

```rust
pub struct FileParameters<'a> {
    pub block_size: u32,
    pub flags: u32,
    pub raw: &'a [u8],
}

impl<'a> FileParameters<'a> {
    pub fn from_bytes(data: &'a [u8]) -> Self;
    pub const fn block_size(&self) -> u32;
    pub const fn leave_block_allocated(&self) -> bool;
    pub const fn has_parent(&self) -> bool;
    pub const fn flags(&self) -> u32;
    pub const fn raw(&self) -> &[u8];
}
```

### `section::Entry` alias

```rust
pub use crate::sections::LogEntry as Entry;
```

`section::Entry<'a>` 和 `section::LogEntry<'a>` 是同一类型。
