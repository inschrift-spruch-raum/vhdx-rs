# `Sections` 与分段读取接口

`Sections` 是 `File` 之下的延迟加载入口，用来读取 VHDX 的 Header、BAT、Metadata、Log 这些 section。它更适合检查文件结构、查看元数据条目、理解 BAT 状态，或继续使用 sector 级导航接口。

## 导航

- 返回 [`index.md`](index.md)，查看库接口分组导航
- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 如果你想先回到更稳定的文件级入口，返回 [`file.md`](file.md)

如果你的目标只是打开文件、创建文件，或者按虚拟磁盘偏移稳定读写，先看 [`file.md`](file.md) 会更直接。

## `File::sections()` 与 `Sections`

`File::sections()` 返回 `&Sections`。当前 `Sections` 会记住 BAT、Metadata、Log 的文件偏移和大小，并在你第一次访问某个 section 时再去读文件。

可用的主入口是：

- `header() -> Result<Header>`
- `bat() -> Result<Bat>`
- `metadata() -> Result<Metadata>`
- `log() -> Result<Log>`

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let sections = file.sections();

let header = sections.header()?;
let bat = sections.bat()?;
let metadata = sections.metadata()?;
let log = sections.log()?;

println!("bat entries: {}", bat.len());
println!("needs log replay: {}", log.is_replay_required());
# Ok::<(), vhdx_rs::Error>(())
```

当前测试已经覆盖了新创建文件后读取这四类 section 的基本路径。

## Header 导航

`Header` 表示 VHDX 开头固定的 1 MiB header section，里面包含：

- `FileTypeIdentifier`
- 两份 `HeaderStructure`
- 两份 `RegionTable`

常见读法如下：

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let header = file.sections().header()?;

let current = header.header(0).unwrap();
println!("version: {}", current.version());
println!("log version: {}", current.log_version());

let file_type = header.file_type();
println!("creator: {}", file_type.creator());
# Ok::<(), vhdx_rs::Error>(())
```

实用上可以这样理解：

- `Header::file_type()` 读取文件类型标识和 creator 字符串。
- `Header::header(0)` 返回当前实现挑选出的“当前 header”，也就是序列号更高的那一份。
- `Header::region_table(0)` 返回当前实现关联的 region table，用来继续找到 BAT 和 Metadata 的物理位置。

crate root 还公开导出了 `FileTypeIdentifier`、`HeaderStructure`、`RegionTable`、`RegionTableHeader`、`RegionTableEntry`，方便你继续往下拆字段。

## BAT 导航

`Bat` 是 Block Allocation Table 的包装，负责把虚拟块映射到文件中的实际位置。常用入口包括：

- `entry(index)`
- `entries()`
- `len()`
- `is_empty()`

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let bat = file.sections().bat()?;

if let Some(entry) = bat.entry(0) {
    println!("offset: {}", entry.file_offset());
    println!("raw: {}", entry.raw());
}
# Ok::<(), vhdx_rs::Error>(())
```

相关的公开类型主要有：

- `BatEntry`
- `BatState`
- `PayloadBlockState`
- `SectorBitmapState`

它们适合在你需要判断某个块是否已分配、是否是全零块、是否涉及 differencing bitmap 时使用。当前 `PayloadBlock::is_allocated()` 只把 `FullyPresent` 当成已分配块，这一点更偏当前实现细节，而不是完整的 VHDX 状态建模承诺。

## Metadata 导航

`Metadata` 用于读取 metadata region，并进一步拆成 metadata table 和常见 metadata items。

常见入口有：

- `table()`，读目录结构
- `items()`，按常见类型取值

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let metadata = file.sections().metadata()?;
let items = metadata.items();

println!("virtual size: {:?}", items.virtual_disk_size());
println!("logical sector size: {:?}", items.logical_sector_size());

if let Some(params) = items.file_parameters() {
    println!("block size: {}", params.block_size());
    println!("has parent: {}", params.has_parent());
}
# Ok::<(), vhdx_rs::Error>(())
```

当前公开出来、在实践里最有用的 metadata 相关类型有：

- `Metadata`
- `MetadataTable`
- `MetadataItems`
- `TableHeader`
- `TableEntry`
- `EntryFlags`
- `FileParameters`
- `ParentLocator`
- `LocatorHeader`
- `KeyValueEntry`

从当前代码和测试看，最值得优先关注的是：

- `MetadataItems::virtual_disk_size()`
- `MetadataItems::logical_sector_size()`
- `MetadataItems::physical_sector_size()`
- `MetadataItems::virtual_disk_id()`
- `MetadataItems::file_parameters()`
- `MetadataItems::parent_locator()`

这里的限制也要说清楚。虽然 metadata 已经能暴露 `has_parent()` 和 `parent_locator()` 入口，但当前仓库对差分盘的支持仍主要停留在 metadata 层和基础检查层面，不应把它当成完整父链读写支持。

## Log 导航

`Log` 用来查看 log region，并判断是否需要在打开时执行日志回放。当前公开的主要入口有：

- `entry(index)`
- `entries()`
- `is_replay_required()`
- `replay(file)`

crate root 同时公开了这些更细的日志类型：

- `LogEntry`
- `LogEntryHeader`
- `Descriptor`
- `DataDescriptor`
- `ZeroDescriptor`
- `DataSector`

这组 API 对理解日志条目结构很有帮助，但这里也要贴近真实实现：

- `Log::entries()` 会扫描 log buffer 并尝试解析有效条目。
- `Log::entry(index)` 现在还是 stub，当前实现固定返回 `None`，不能把它写成成熟的随机访问接口。
- 新创建文件的集成测试确认 `log.is_replay_required()` 当前应为 `false`。

如果你只是想知道文件是否带有待回放日志，优先看 `File::has_pending_logs()` 会更省事。它已经把打开阶段的检查结果整理成文件级布尔值。

## `IO`、`Sector`、`PayloadBlock`

`IO` 是介于高层 `File` API 和 section 结构之间的一层导航接口，适合按 sector 和 payload block 观察数据布局。

从 `File` 进入的方式是：

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let io = file.io();

if let Some(sector) = io.sector(0) {
    println!("block idx: {}", sector.block_idx());
    println!("global sector: {}", sector.global_sector());
}
# Ok::<(), vhdx_rs::Error>(())
```

### `IO`

常见入口：

- `sector(sector_number) -> Option<Sector>`
- `read_sectors(start_sector, buf) -> Result<usize>`
- `write_sectors(start_sector, data) -> Result<usize>`

`read_sectors` 当前可用，但要求缓冲区长度必须是 logical sector size 的整数倍。超出虚拟磁盘范围的 sector 会按零值填充。

`write_sectors` 的限制必须贴近这个接口本身说明：它现在还没有完整实现，当前直接返回错误，错误文本也明确说明它需要可变访问路径。不要把它写成稳定写接口。如果你需要当前更可靠的写路径，请退回 `File::write`，而且最好只在固定盘上使用。

### `Sector`

`Sector` 表示全局 sector 号映射到的一个 sector 视图，常用方法有：

- `block_idx()`
- `block_sector_idx()`
- `global_sector()`
- `read(buf)`
- `payload()`

`Sector::read` 会转回 `File::read`，所以仍然继承当前实现的行为边界，比如动态盘未分配区域读零值。

### `PayloadBlock`

`PayloadBlock` 表示一个 payload block 视图，常用方法有：

- `block_idx()`
- `read(offset, buf)`
- `bat_entry()`
- `is_allocated()`

它很适合把 BAT 里的块状态和实际读取行为对应起来看，但目前更偏调试和结构浏览用途，而不是一条完整的底层写路径。

## 什么时候用哪一层

可以用一个简单规则区分：

- 想稳定地打开文件、创建文件、按字节偏移读写，先用 `File`
- 想检查 section 布局和元数据，转到 `Sections`
- 想按 sector 或 payload block 导航，使用 `IO`、`Sector`、`PayloadBlock`

当前实现里，越往底层走，越需要你接受“更接近内部结构，也更容易碰到尚未补齐的写路径”这个现实。尤其是 `IO::write_sectors`，现在还不应该当成可依赖的写接口。

如果你需要回到更高层的稳定读写入口，请返回 [`file.md`](file.md)。如果你想重新选阅读路线，可以回到 [`index.md`](index.md) 或 [`../../API.md`](../../API.md)。
