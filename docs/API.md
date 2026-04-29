# vhdx-rs API Reference

本文档以 `docs/plan/API.md` 为承诺基线，且与 `src/lib.rs` 当前导出面保持一致。
只描述承诺面，避免把计划未承诺项写成公开契约。

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
├── Guid
├── Error
├── Result<T>
├── SpecValidator<'a>
├── ValidationIssue
├── mod validation
├── mod section
│   ├── Sections<'a>
│   ├── Header<'a>
│   ├── FileTypeIdentifier<'a>
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
│   ├── DataSector<'a>
│   └── mod StandardItems
└── mod constants
```

## Key signatures (承诺面)

### `File`

```rust
impl File {
    pub fn open(path: impl AsRef<Path>) -> OpenOptions;
    pub fn create(path: impl AsRef<Path>) -> CreateOptions;
    pub fn sections(&self) -> &Sections<'_>;
    pub fn io(&self) -> IO<'_>;
    pub fn validator(&self) -> vhdx_rs::validation::SpecValidator<'_>;
    pub fn inner(&self) -> &std::fs::File;
}
```

### `OpenOptions`

```rust
impl OpenOptions {
    pub fn write(self) -> Self;
    pub fn strict(self, strict: bool) -> Self;
    pub fn log_replay(self, policy: LogReplayPolicy) -> Self;
    pub fn finish(self) -> Result<File>;
}
```

### `LogReplayPolicy`

- 严格路径：`Require`、`Auto`、`InMemoryOnReadOnly`。
- `ReadOnlyNoReplay` 是兼容模式例外，可用于只读诊断。
- `ReadOnlyNoReplay` 不是严格 MS-VHDX 一致性路径，存在 pending log 时不会触发回放写入。

### `CreateOptions`

```rust
impl CreateOptions {
    pub fn size(self, size: u64) -> Self;
    pub fn fixed(self, fixed: bool) -> Self;
    pub fn block_size(self, block_size: u32) -> Self;
    pub fn logical_sector_size(self, logical_sector_size: u32) -> Self;
    pub fn physical_sector_size(self, physical_sector_size: u32) -> Self;
    pub fn parent_path(self, path: impl AsRef<Path>) -> Self;
    pub fn finish(self) -> Result<File>;
}
```

### `validation::SpecValidator`

```rust
impl SpecValidator<'_> {
    pub fn validate_file(&self) -> Result<()>;
    pub fn validate_header(&self) -> Result<()>;
    pub fn validate_region_table(&self) -> Result<()>;
    pub fn validate_bat(&self) -> Result<()>;
    pub fn validate_metadata(&self) -> Result<()>;
    pub fn validate_required_metadata_items(&self) -> Result<()>;
    pub fn validate_log(&self) -> Result<()>;
    pub fn validate_parent_locator(&self) -> Result<()>;
    pub fn validate_parent_chain(&self) -> Result<ParentChainInfo>;
}
```

### `IO<'a>`, `Sector<'a>`, `PayloadBlock<'a>`

```rust
impl<'a> IO<'a> {
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

## 非承诺边界（文档防越界）

- `IO::write_sectors`、`IO::read_sectors` 属于内部实现项，不是公开承诺面。
- 本文档不把 plan 未承诺项升级为公开契约。
