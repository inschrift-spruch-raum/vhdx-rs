# `maintenance` 文档分组

这一页把修复和差分相关命令放在一起介绍，但 `maintenance` 只是文档分组，不是实际存在的 CLI 子命令。

## 导航

- 返回 [`index.md`](index.md)，查看 CLI 分组导航
- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 如果你要先看文件创建，前往 [`create.md`](create.md)
- 如果你要回到查看和检查类命令，前往 [`inspect.md`](inspect.md)

当前代码里的真实顶层命令仍然是：

- `repair`
- `diff`

## `repair`

```bash
vhdx-tool repair <file> [--dry-run]
```

`repair` 的目标是走可写打开路径，让带待回放日志的文件有机会完成日志重放。

### 默认行为

不带 `--dry-run` 时，命令会：

1. 打印正在修复的文件路径
2. 以可写方式打开文件
3. 如果打开成功，输出 `File repaired successfully` 和 `Log entries replayed`

这里的输出是简要状态提示，不是完整修复报告。它不会列出修了哪些结构，也不会给出详细的日志回放明细。

### `--dry-run`

`--dry-run` 会改成只读检查：

- 如果文件有待处理日志，输出“这些日志本来会被回放”
- 如果没有待处理日志，输出文件不需要修复

集成测试覆盖了这个 dry-run 成功路径。

### 当前限制

- `repair` 更像是“触发修复路径”的入口，不是完整诊断和修复报告工具。
- 成功输出并不表示所有潜在问题都被详细分析过，它只说明当前可写打开和相关修复路径完成了。

## `diff`

```bash
vhdx-tool diff <file> <parent|chain>
```

这是当前 CLI 里另一个真实存在的嵌套命令组。

### `diff parent`

读取差分盘元数据里的 parent locator 信息。

- 如果文件没有父盘标记，命令会输出 `This is not a differencing disk (no parent)`。
- 如果文件带父盘标记，并且 metadata 里有 parent locator，命令会逐项打印 locator entries 的 key/value。

需要注意的是，这里展示的是当前能读到的 parent locator 条目，不代表 CLI 已经具备完整父链解析能力。

### `diff chain`

输出一个简要的链路视图。

- 总会先打印当前文件路径
- 如果没有父盘标记，会标成 `base disk`
- 如果有父盘标记，会提示 `has parent - chain traversal not yet implemented`

这说明 `diff chain` 目前只是一个很轻量的链路摘要入口，还不能真正遍历完整差分链。

## 当前边界总结

- `repair` 可以触发当前修复路径，但输出不是完整 repair report。
- `diff parent` 只展示当前能读到的父盘定位信息。
- `diff chain` 还没有完整链路遍历。
- 整体差分盘支持仍是部分实现，文档不应把这些命令描述成成熟的差分管理工作流。

如果你需要查看概要信息、section 细节或快速完整性检查，请返回 [`inspect.md`](inspect.md)。

如果你想重新选择 CLI 阅读路径，也可以回到 [`index.md`](index.md)。
