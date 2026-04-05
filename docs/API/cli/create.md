# `create`

`vhdx-tool create` 用来创建新的 VHDX 文件。当前命令入口是平铺顶层命令，不属于任何额外子命令组。

## 导航

- 返回 [`index.md`](index.md)，查看 CLI 分组导航
- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 创建后想检查结果时，继续看 [`inspect.md`](inspect.md)

## 基本用法

```bash
vhdx-tool create <path> --size <SIZE>
```

常见例子：

```bash
vhdx-tool create demo.vhdx --size 64MiB
vhdx-tool create fixed.vhdx --size 1GiB --disk-type fixed
vhdx-tool create custom.vhdx --size 1GiB --block-size 1MiB
```

集成测试覆盖了固定盘、动态盘、显式块大小，以及缺少 `--size` 或给出非法大小字符串时的失败路径。

## 参数

### `<path>`

目标文件路径。命令成功时会创建该文件，并打印创建结果摘要。如果目标路径已经存在，命令会失败并输出 `Error creating VHDX file`。

### `--size <SIZE>`

必填参数，表示虚拟磁盘大小。CLI 通过自定义解析器接收类似 `1M`、`100MiB`、`1GiB` 这样的值。省略该参数时会被 clap 直接拒绝，非法字符串会返回 `Invalid size`。

### `--disk-type <dynamic|fixed|differencing>`

默认值是 `dynamic`。

- `dynamic`, 创建动态盘
- `fixed`, 创建固定盘
- `differencing`, 创建带父盘标记的差分盘入口

当前实现里，命令内部只有一个 `fixed` 布尔位和一个 `has_parent` 布尔位参与创建流程，所以 `differencing` 的行为本质上是“非固定盘，并设置 has_parent”。

### `--block-size <SIZE>`

可选，默认值是 `32MiB`。集成测试覆盖了 `1MiB` 这种显式设置，并验证输出里会包含人类可读的块大小。

### `--parent <PATH>`

用于差分盘场景。只要命令被视为差分盘，或者显式传了 `--parent`，当前实现都会要求这个参数存在。缺少时命令会直接报错：`Differencing disk requires --parent option`。

## 当前行为

成功时，命令会输出：

- 创建的文件路径
- 虚拟容量的人类可读值
- 块大小的人类可读值
- 识别出的类型，`Fixed`、`Dynamic` 或 `Differencing`
- 如果传了 `--parent`，还会回显父盘路径

## 当前限制和注意事项

- 固定盘和动态盘创建路径是当前更可靠的主路径，集成测试已经覆盖。
- 动态盘虽然能创建，但仓库当前整体状态仍把动态写入视为未完整实现，所以不要把“能创建动态盘”理解成“动态盘写路径已经完整可用”。
- 差分盘支持仍是部分实现。当前 `create` 命令会检查并回显 `--parent`，也会把文件按带父盘状态创建出来，但它没有把传入的父路径持久化成完整可用的父链支持。
- 因为上面这层限制，`--disk-type differencing` 更适合被理解为“写入差分相关元数据状态的入口”，而不是成熟的差分盘工作流。

如果你还需要查看创建后文件的概要信息，下一步通常是运行 `vhdx-tool info <file>`，相关说明见 [`inspect.md`](inspect.md)。

如果你后面要处理日志回放或差分盘检查，可以继续看 [`maintenance.md`](maintenance.md)。
