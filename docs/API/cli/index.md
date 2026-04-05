# CLI 文档导航

`vhdx-tool` 当前的 clap 命令树在顶层保持平铺。实际可调用的顶层命令是：

## 导航

- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 回到 [`../../index.md`](../../index.md)，查看文档总览
- 继续进入 [`create.md`](create.md)、[`inspect.md`](inspect.md)、[`maintenance.md`](maintenance.md)

- `info`
- `create`
- `check`
- `repair`
- `sections`
- `diff`

这意味着当前 CLI 里并不存在名为 `inspect` 或 `maintenance` 的真实子命令。它们只是文档里的阅读分组，用来把相关命令放在一起说明。

## 文档分组

- [`create.md`](create.md)：单独说明 `vhdx-tool create`
- [`inspect.md`](inspect.md)：文档分组，汇总 `info`、`sections`、`check`
- [`maintenance.md`](maintenance.md)：文档分组，汇总 `repair`、`diff`

## 当前命令边界

### `create`

创建新的 VHDX 文件。当前支持固定盘、动态盘，以及带父盘标记的差分盘创建入口，但差分相关支持仍然是部分实现。

### `info`

输出文件级概要信息，支持文本和 JSON 两种输出格式。适合快速确认虚拟磁盘大小、块大小、扇区大小，以及是否带父盘标记。

### `check`

做一次偏概要的完整性检查。它会验证文件是否能打开、头部和区域表是否能解析、元数据是否可读，并在可用时确认 BAT 可访问。`--repair` 和 `--log-replay` 目前只会打印“请求了该动作”，不会真正执行对应流程。

### `repair`

触发可写打开路径，用于处理带待回放日志的文件。当前输出是简要成功或失败提示，不是完整修复报告。

### `sections`

查看内部 section。这里才有真实的嵌套子命令：`header`、`bat`、`metadata`、`log`。

### `diff`

查看差分盘相关信息。这里也有真实的嵌套子命令：`parent`、`chain`。

## 建议阅读顺序

1. 先看 [`create.md`](create.md)，确认创建参数和当前限制。
2. 再看 [`inspect.md`](inspect.md)，了解 `info`、`sections`、`check` 这些查看与检查命令。
3. 最后看 [`maintenance.md`](maintenance.md)，了解 `repair` 与 `diff` 的当前能力边界。

如果你想看完整顶层导航和更高层的实现限制，返回 [`../../API.md`](../../API.md)。

如果你已经知道自己要做什么，创建新文件时先看 [`create.md`](create.md)；查看与检查命令见 [`inspect.md`](inspect.md)；修复与差分相关命令见 [`maintenance.md`](maintenance.md)。
