# `File` 相关接口

`File` 是当前库里最适合先上手的入口类型。你可以用它打开已有 VHDX，创建新文件，读取或写入虚拟磁盘偏移上的数据，并查询文件级元数据。

## 导航

- 返回 [`index.md`](index.md)，查看库接口分组导航
- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 如果你要继续下钻 section 和底层结构，前往 [`sections.md`](sections.md)

如果你更关心 Header、BAT、Metadata、Log 这些 section 结构，或者要按 sector 继续下钻，请接着看 [`sections.md`](sections.md)。

## 入口与 builder

`File` 本身由 crate root 公开导出，所以常见使用方式是：

```rust,no_run
use vhdx_rs::File;
```

### `File::open(path) -> OpenOptions`

`File::open(path)` 不会立刻打开文件，而是返回 `OpenOptions` builder。默认按只读模式打开，随后用 `.finish()` 完成真正的打开流程。

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
# Ok::<(), vhdx_rs::Error>(())
```

如果你需要写权限，可以继续链式调用 `.write()`：

```rust,no_run
use vhdx_rs::File;

let mut file = File::open("disk.vhdx").write().finish()?;
file.write(0, b"test data")?;
# Ok::<(), vhdx_rs::Error>(())
```

当前测试覆盖了只读打开固定盘与动态盘，也覆盖了通过 `.write()` 重新打开固定盘后再写入的路径。

### `File::create(path) -> CreateOptions`

`File::create(path)` 同样先返回 `CreateOptions` builder。当前公开可用的链式方法以 `size(...)`、`fixed(...)`、`block_size(...)`、`has_parent(...)` 为主，最后通过 `.finish()` 落盘并重新按标准打开流程返回 `File`。

```rust,no_run
use vhdx_rs::File;

let file = File::create("new-disk.vhdx")
    .size(64 * 1024 * 1024)
    .fixed(true)
    .finish()?;

assert!(file.is_fixed());
# Ok::<(), vhdx_rs::Error>(())
```

自定义 block size 的路径也有测试覆盖：

```rust,no_run
use vhdx_rs::File;

let file = File::create("custom.vhdx")
    .size(4 * 1024 * 1024)
    .fixed(false)
    .block_size(1024 * 1024)
    .finish()?;

assert_eq!(file.block_size(), 1024 * 1024);
# Ok::<(), vhdx_rs::Error>(())
```

### 关于 `OpenOptions` 与 `CreateOptions`

这两个 builder 类型是 `src/file.rs` 里的公开结构体，由 `File::open` 与 `File::create` 返回。它们可以在文档里当成公开 API 来描述，但当前 `src/lib.rs` 没有把它们做成 crate root re-export，所以别把它们写成 `vhdx_rs::OpenOptions` 或 `vhdx_rs::CreateOptions` 这样的顶层导出承诺。

## 文件级查询方法

`File` 打开成功后，会把一部分常用信息直接缓存成便于查询的方法：

- `virtual_disk_size() -> u64`
- `block_size() -> u32`
- `logical_sector_size() -> u32`
- `is_fixed() -> bool`
- `has_parent() -> bool`
- `has_pending_logs() -> bool`
- `sections() -> &Sections`
- `io() -> IO`
- `inner() -> &std::fs::File`

这些方法适合在不想自己先拆 Header、Metadata 时直接拿当前文件的高层状态。

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;

println!("virtual size: {}", file.virtual_disk_size());
println!("block size: {}", file.block_size());
println!("logical sector size: {}", file.logical_sector_size());
println!("fixed: {}", file.is_fixed());
println!("has parent: {}", file.has_parent());
println!("pending logs: {}", file.has_pending_logs());
# Ok::<(), vhdx_rs::Error>(())
```

当前集成测试确认了这些查询里最常见的行为：

- 默认 block size 是 `32 MiB`
- 默认 logical sector size 是 `512`
- 新创建的非差分盘 `has_parent()` 为 `false`
- 新创建文件 `has_pending_logs()` 为 `false`

`has_pending_logs()` 的语义也值得单独记一下。当前打开流程会检查 header 里的 log GUID 和 log section，如果只读打开时发现需要日志回放，库会允许文件继续打开，并把这个状态暴露给调用方。可写打开时，当前实现会尝试执行日志回放并清掉 header 里的 log GUID。

## 读取，写入，刷新

### `read(offset, buf) -> Result<usize>`

`File::read` 按虚拟磁盘偏移读取数据，不要求你自己先换算 section 或 BAT 偏移。

```rust,no_run
use vhdx_rs::File;

let file = File::open("disk.vhdx").finish()?;
let mut buf = vec![0u8; 512];
let n = file.read(0, &mut buf)?;
println!("read {} bytes", n);
# Ok::<(), vhdx_rs::Error>(())
```

当前实现行为和测试结果可以概括成两条：

- 对固定盘，读取会直接映射到 payload 区。
- 对动态盘，未分配区域读取目前会返回零值，这一点有集成测试直接覆盖。

如果 `offset` 已经超出 `virtual_disk_size()`，当前实现会返回 `Ok(0)`。

### `write(offset, data) -> Result<usize>`

`File::write` 也是按虚拟磁盘偏移工作。

```rust,no_run
use vhdx_rs::File;

let mut file = File::create("fixed.vhdx")
    .size(8 * 1024 * 1024)
    .fixed(true)
    .finish()?;

file.write(0, b"hello vhdx")?;
# Ok::<(), vhdx_rs::Error>(())
```

这里要把当前限制贴近接口说明：

- 固定盘写入是当前更完整、也有集成测试覆盖的写路径，支持直接写入 payload 区，然后再通过 `flush()` 持久化。
- 动态盘写入还没有完整实现。当前代码只做了很浅的 BAT 检查，遇到未分配块或需要扩展 BAT 时会返回错误。集成测试也明确断言了动态盘写入当前应当失败。

因此，如果你的场景需要稳定写路径，请把 `File::write` 理解成“固定盘可用，动态盘暂不可靠”。不要把当前实现当成完整的动态块分配器。

如果写入起点已经超出虚拟磁盘大小，当前实现会返回 `Error::InvalidParameter`。

### `flush() -> Result<()>`

`flush()` 会调用底层文件同步，把当前待写入内容刷到磁盘。

```rust,no_run
use vhdx_rs::File;

let mut file = File::create("fixed.vhdx")
    .size(1024 * 1024)
    .fixed(true)
    .finish()?;

file.write(0, b"flush test")?;
file.flush()?;
# Ok::<(), vhdx_rs::Error>(())
```

集成测试覆盖了“写入，刷新，重新打开，再读取”的路径，说明固定盘上的 `write + flush + reopen + read` 目前是可工作的。

## 与底层接口的关系

`File` 已经为大多数场景封装了偏移换算和 section 初始化，所以通常优先使用：

- `File::read`
- `File::write`
- `File::flush`
- `File` 上的高层查询方法

只有当你需要检查 Header、BAT、Metadata、Log，或要按 sector 继续导航时，再转到 `file.sections()` 和 `file.io()`。这些更底层的入口放在 [`sections.md`](sections.md) 里展开说明。

如果你已经看完 `File` 这一层，下一步通常是进入 [`sections.md`](sections.md) 查看 section 级结构，或者返回 [`index.md`](index.md) 选择其他库文档入口。
