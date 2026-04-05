# API 与 CLI 文档索引

`docs/API.md` 现在是当前文档系统里的稳定顶层入口，用来快速说明公开能力范围，并把你带到更细的库接口页和 CLI 说明页。这里保留高层导航和实现边界，不再承载整份单体式长参考。

## 相关入口

- 返回 [`index.md`](index.md)，查看文档总览
- 回到 [`../README.md`](../README.md)，查看项目快速入口
- 库接口分组导航见 [`API/library/index.md`](API/library/index.md)
- CLI 分组导航见 [`API/cli/index.md`](API/cli/index.md)

仓库当前包含两条公开使用面：

- 根 crate `vhdx-rs`，提供库接口
- `vhdx-cli/`，提供命令行工具 `vhdx-tool`

## API tree

下面这份树保留为顶层导航，描述当前公开导出面与当前 CLI 结构，方便先定位入口再继续下钻。

```text
vhdx_rs
├── File
│   ├── open(path) -> OpenOptions
│   ├── create(path) -> CreateOptions
│   ├── sections() -> &Sections
│   ├── io() -> IO
│   ├── inner() -> &std::fs::File
│   ├── virtual_disk_size() -> u64
│   ├── block_size() -> u32
│   ├── logical_sector_size() -> u32
│   ├── is_fixed() -> bool
│   ├── has_parent() -> bool
│   ├── has_pending_logs() -> bool
│   ├── read(offset, buf) -> Result<usize>
│   ├── write(offset, data) -> Result<usize>
│   └── flush() -> Result<()>
│
├── file module public builder types
│   ├── OpenOptions
│   └── CreateOptions
│
├── Sections
│   ├── header() -> Result<Header>
│   ├── bat() -> Result<Bat>
│   ├── metadata() -> Result<Metadata>
│   └── log() -> Result<Log>
│
├── section and IO exports
│   ├── Header / RegionTable / Metadata / Log / Bat
│   ├── IO / Sector / PayloadBlock
│   └── Guid / Error / Result<T>
│
vhdx-tool
├── info <file> [--format <text|json>]
├── create <path> --size <SIZE> [--disk-type <dynamic|fixed|differencing>] [--block-size <SIZE>] [--parent <PATH>]
├── check <file> [--repair] [--log-replay]
├── repair <file> [--dry-run]
├── sections <file> <header|bat|metadata|log>
└── diff <file> <parent|chain>
```

## 文档分组

新版文档目前拆成两个导航分组：

- `library/`，面向 `vhdx-rs` 的公开库接口
- `cli/`，面向 `vhdx-tool` 的命令说明

这种拆分是文档组织方式，不是代码层面的重新分组。尤其是 CLI 文档里的 `inspect` 与 `maintenance` 只是阅读入口，当前代码里的顶层命令仍然是 `info`、`create`、`check`、`repair`、`sections`、`diff`。

## 库接口导航

- [`docs/API/library/index.md`](API/library/index.md)：库接口分组入口
- [`docs/API/library/file.md`](API/library/file.md)：`File` 入口，以及打开、创建、按偏移读写相关能力
- [`docs/API/library/sections.md`](API/library/sections.md)：`Sections`、Header、BAT、Metadata、Log，以及更细粒度的结构访问入口

当前实现面里，`File` 仍是最稳定的主入口。crate root 公开导出 `File`、`IO`、`PayloadBlock`、`Sector`，以及多种 section 类型；`OpenOptions` 与 `CreateOptions` 是 `src/file.rs` 中的公开构建器类型，由 `File::open` 和 `File::create` 返回，但不是 crate root re-export。

## CLI 文档导航

- [`docs/API/cli/index.md`](API/cli/index.md)：CLI 分组入口
- [`docs/API/cli/create.md`](API/cli/create.md)：`create` 命令导航
- [`docs/API/cli/inspect.md`](API/cli/inspect.md)：文档层面汇总 `info`、`sections`、`check`
- [`docs/API/cli/maintenance.md`](API/cli/maintenance.md)：文档层面汇总 `repair`、`diff`

当前 CLI 结构仍以实际命令行为准，不存在名为 `inspect` 或 `maintenance` 的真实顶层子命令。

## 当前实现边界

这份索引只保留最需要先知道的限制，详细说明见各子页：

- 固定盘的创建、读取、写入、刷新是当前更完整的主路径
- 动态盘支持创建，未分配区域读取会返回零值
- 动态盘写入还没有完整实现，当前会返回错误
- `IO::write_sectors` 仍未完整实现，如果你要稳定读写路径，优先使用 `File::read` 与 `File::write`
- 差分盘相关能力目前主要停留在元数据入口和基础检查层面，不能当成完整父链支持
- `repair` 与日志重放路径可触发修复流程，但当前 CLI 输出仍偏概要

## 建议阅读顺序

如果你是第一次查看这套文档，建议顺序如下：

1. 根目录 `README.md`
2. 本页，先确认公开入口和限制
3. `library/` 或 `cli/` 下与你当前任务最相关的子页
4. `src/lib.rs`、`tests/integration_test.rs`、`vhdx-cli/tests/cli_integration.rs` 中的代码与测试示例

如果你已经确定方向，可以直接进入 [`API/library/file.md`](API/library/file.md)、[`API/library/sections.md`](API/library/sections.md)、[`API/cli/create.md`](API/cli/create.md)、[`API/cli/inspect.md`](API/cli/inspect.md) 或 [`API/cli/maintenance.md`](API/cli/maintenance.md)。
