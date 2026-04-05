# `inspect` 文档分组

这一页把查看和检查类命令放在一起介绍，但 `inspect` 只是文档分组，不是实际存在的 CLI 子命令。

## 导航

- 返回 [`index.md`](index.md)，查看 CLI 分组导航
- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 如果你还没创建文件，先看 [`create.md`](create.md)
- 如果你要继续看修复或差分相关命令，前往 [`maintenance.md`](maintenance.md)

当前代码里的真实顶层命令仍然是：

- `info`
- `sections`
- `check`

## `info`

```bash
vhdx-tool info <file> [--format <text|json>]
```

`info` 会以只读方式打开文件，然后输出文件级概要信息。

### 输出格式

- `--format text`, 默认值，输出可读文本摘要
- `--format json`, 输出一个很小的 JSON 对象

文本输出当前会包含：

- 路径
- 虚拟磁盘大小，字节和人类可读两种表示
- 块大小
- 逻辑扇区大小
- `Disk Type: Fixed` 或 `Disk Type: Dynamic`
- 如果文件带父盘标记，还会额外输出 `Type: Differencing (has parent)`
- 一部分元数据摘要，比如 file parameters 和 virtual disk id

JSON 输出当前字段比较少，只包含：`path`、`virtual_size`、`block_size`、`logical_sector_size`、`is_fixed`、`has_parent`。

如果文件带待处理日志，`info` 会先打印警告，提示运行 `vhdx-tool repair <file>`。

## `sections`

```bash
vhdx-tool sections <file> <header|bat|metadata|log>
```

这是当前 CLI 里真实存在的嵌套命令组之一。文档里的“inspect”分组不会改变它的命令边界。

### `sections header`

输出 header section 的概要字段，包括 sequence number、version、log version、log length、log offset，以及几个 GUID。

### `sections bat`

输出 BAT section 的概要信息。当前实现会计算并打印 `Total BAT Entries`，但不会列出完整 BAT 内容。

这是一个很重要的限制，当前 BAT 查看是摘要型输出，不是逐项浏览工具。

### `sections metadata`

输出 metadata section 的关键条目，包括：

- block size
- leave block allocated
- has parent
- virtual disk size
- virtual disk id
- logical sector size
- physical sector size

### `sections log`

当前只会打印 `Log Section` 和 `Log viewing not yet implemented`。也就是说，这个子命令已经占位，但还不是完整的日志查看器。

和 `info` 一样，如果文件带待处理日志，`sections` 会先输出修复提示。

## `check`

```bash
vhdx-tool check <file> [--repair] [--log-replay]
```

`check` 会做一次偏概要的检查流程，当前重点是确认文件能否正常打开，并且几个主要 section 是否能被解析或访问。

当前成功输出大致表示以下步骤通过：

- 文件成功打开
- headers validated
- region tables parsed
- metadata section valid
- 如果 BAT 可访问，还会输出 `BAT section accessible`

### `--repair`

当前只会额外打印 `Repair requested (not yet implemented)`，不会真正执行修复。

### `--log-replay`

当前只会额外打印 `Log replay requested (not yet implemented)`，不会真正回放日志。

### 待处理日志提示

如果文件包含 pending log entries，`check` 会先给出警告，并提示改用 `vhdx-tool repair <file>` 处理。

## 当前边界总结

- `info` 适合看文件级摘要，并支持 text/json 两种输出。
- `sections` 适合看 section 级摘要，但 BAT 仍是 summary-only，log 仍未实现完整查看。
- `check` 适合做快速可读性和结构性检查，但它不是详细审计工具，两个“带动作”的 flag 目前也还只是占位提示。

如果你要看修复和差分相关命令，请继续看 [`maintenance.md`](maintenance.md)。

如果你只是需要重新确认 CLI 阅读入口，也可以返回 [`index.md`](index.md)。
