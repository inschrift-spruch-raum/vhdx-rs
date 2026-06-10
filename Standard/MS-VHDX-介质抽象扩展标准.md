# MS-VHDX 介质抽象扩展标准

> 基线：MS-VHDX v20240423  
> 作用域：API 行为层中的介质抽象、打开/创建语义、能力分层、数据面读写、父介质解析和校验边界；不修改 on-disk 格式。  
> 目的：将 `vhdx-rs` 的核心模型从路径文件改为基于 `std::io` 的通用介质模型。

---

## 1. 范围

本文定义“介质抽象”的完整行为约束：

- 核心类型 `Medium<T>` 的语义；
- `Medium::open(inner)` 与 `Medium::create(inner)` 的入口约束；
- `Read` / `Write` / `Seek` 与 `Len` / `SetLen` / `SyncData` 的能力分层；
- 打开模式、日志回放、header 更新和虚拟扇区 I/O 的边界；
- differencing 父介质解析、父链校验与默认拒绝策略；
- `sections()`、`validator()`、`io()` 在泛型介质下的借用与错误语义。

本文 `MUST/SHOULD/MAY` 采用 RFC2119 语义；如与 MS-VHDX §1.7 + §2 冲突，以 MS-VHDX 为准。

---

## 2. 术语

- **介质（Medium）**：一个已打开的 VHDX API 对象，负责在底层 `std::io` 字节流上解释 VHDX 布局。  
- **底层介质（inner）**：调用方传入的 `T`，实现 `Read + Seek` 或更强能力。  
- **路径**：创建 `std::fs::File` 等底层介质的一种调用方行为，不是 `Medium<T>` 的属性。  
- **只读打开**：未调用 `OpenOptions::write()` 的打开模式。  
- **可写打开**：调用 `OpenOptions::write()` 后的打开模式。  
- **结构面**：Header / Region / BAT / Metadata / Log 结构读取。  
- **数据面**：虚拟扇区读写，即用户可见虚拟磁盘内容。  
- **父介质（ParentMedium）**：用于 differencing 回退读取的父级 VHDX 数据面抽象。  
- **父介质解析器（ParentResolver）**：把 child parent locator 解析为 `ParentMedium` 的调用方策略对象。

---

## 3. 核心模型

### 3.1 类型命名

实现 **MUST** 将公开核心类型命名为 `Medium<T>`。

实现 **MUST NOT** 继续将公开核心类型命名为 `File`，也 **MUST NOT** 提供 `File` 兼容别名。

实现 **MUST** 将对应模块命名为 `medium`，并从 crate root 导出新的介质语言：

```rust
pub use medium::{
    CreateOptions,
    Len,
    LogReplayPolicy,
    Medium,
    OpenOptions,
    ReadSemanticsPolicy,
    SetLen,
    SyncData,
};
```

### 3.2 路径无关性

`Medium<T>` **MUST NOT** 保存路径、存储位置、base directory 或任何等价 location 字段。

实现 **MUST NOT** 提供专门的路径入口，例如 `open_path`、`create_path`、`open_on_path`。

调用方如需从路径打开 VHDX，**MUST** 自行创建 `std::fs::File` 或其它底层介质，再传给 `Medium::open(inner)` 或 `Medium::create(inner)`。

### 3.3 底层介质所有权

`Medium::open(inner)` 与 `Medium::create(inner)` **MUST** 消费传入的 `inner`，并返回控制该底层介质的 `Medium<T>`。

借用型底层介质 **MAY** 通过 `T = &mut U` 自然表达；实现 **MUST NOT** 为借用介质设计单独的公开 API 族。

`Medium<T>` **SHOULD** 提供以下标准访问器：

```rust
impl<T> Medium<T> {
    pub fn get_ref(&self) -> InnerRef<'_, T>;
    pub fn get_mut(&mut self) -> &mut T;
    pub fn into_inner(self) -> T;
}
```

`get_ref()` **MAY** 返回只读 guard-backed 引用，以便实现内部同步。`get_mut()` 文档 **MUST** 警告调用方：直接修改底层介质可能使 `Medium<T>` 的缓存、BAT、Metadata 或 Log 视图失效。

---

## 4. 能力分层

### 4.1 标准库能力

实现 **MUST** 以 `std::io::{Read, Write, Seek}` 作为底层介质读写能力基础。

实现 **MUST NOT** 定义 `ReadSeek` 或 `ReadWriteSeek` 组合 trait。公开签名 **MUST** 直接写出标准库 trait bound。

### 4.2 长度与扩容能力

实现 **MUST** 定义最小扩展能力：

```rust
pub trait Len {
    fn len(&mut self) -> std::io::Result<u64>;
}

pub trait SetLen: Len {
    fn set_len(&mut self, len: u64) -> std::io::Result<()>;
}
```

`Len` **MUST NOT** 规定 `is_empty()` 默认方法。

`Len::len` **MUST** 使用 `&mut self`，以支持只能通过 `Seek` 查询长度的底层介质。

实现 **MUST** 为 `std::fs::File` 提供 `Len` 与 `SetLen` 实现。`std::fs::File` 的 `Len` 实现 **MUST** 使用 `metadata().len()`，不得通过移动 stream position 查询长度。

实现 **MUST** 为 `std::io::Cursor<Vec<u8>>` 提供 `Len` 与 `SetLen` 实现。

实现 **MUST NOT** 提供基于 `SeekFrom::End(0)` 的 blanket `Len` 实现。

### 4.3 稳定写入能力

实现 **MUST** 定义稳定写入能力：

```rust
pub trait SyncData {
    fn sync_data(&mut self) -> std::io::Result<()>;
}
```

`SyncData` **MUST NOT** 有 blanket no-op 实现。

实现 **MUST** 为 `std::fs::File` 映射到 `std::fs::File::sync_data`。

实现 **MUST** 为 `std::io::Cursor<Vec<u8>>` 提供 no-op `SyncData` 实现。

实现 **MUST** 在文档中区分 `std::io::Write::flush()` 与 `SyncData::sync_data()`。前者只代表写缓冲刷新，后者代表 VHDX 崩溃一致性所需的稳定写入边界。

---

## 5. 打开 API

### 5.1 入口

`Medium::open(inner)` **MUST** 是打开已有 VHDX 的唯一公开入口：

```rust
impl<T> Medium<T> {
    pub fn open(inner: T) -> OpenOptions<T, ReadOnly>;
}
```

`OpenOptions` **MUST** 使用模式 type-state 表达只读/可写打开：

```rust
pub struct ReadOnly;
pub struct ReadWrite;

pub struct OpenOptions<T, Mode = ReadOnly> { /* private fields */ }
```

### 5.2 Builder 方法

`strict(...)` 与 `log_replay(...)` **MUST** 可用于只读与可写 builder：

```rust
impl<T, Mode> OpenOptions<T, Mode> {
    pub fn strict(self, strict: bool) -> Self;
    pub fn log_replay(self, policy: LogReplayPolicy) -> Self;
}
```

`write()` **MUST** 是唯一进入可写打开模式的方法；实现 **MUST NOT** 提供 `read_only(false)` 等等价入口。

```rust
impl<T> OpenOptions<T, ReadOnly>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
    pub fn write(self) -> OpenOptions<T, ReadWrite>;
}
```

### 5.3 Finish 约束

只读打开完成 **MUST** 仅要求 `T: Read + Seek`：

```rust
impl<T> OpenOptions<T, ReadOnly>
where
    T: Read + Seek,
{
    pub fn finish(self) -> Result<Medium<T>>;
}
```

可写打开完成 **MUST** 要求完整写介质能力：

```rust
impl<T> OpenOptions<T, ReadWrite>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
    pub fn finish(self) -> Result<Medium<T>>;
}
```

`Medium<T>` 本身 **MUST NOT** 携带打开模式类型参数。实现 **MUST** 在 `Medium<T>` 内保存运行时 `write` 策略，用于在数据面写入时返回 `ReadOnly`。

### 5.4 Header section 读取

打开阶段实现 **MUST** 从 offset 0 读取完整 1 MiB Header section。

若底层介质不足 8 字节签名，`finish()` **MUST** 返回“介质过小，无法包含 VHDX signature”的可区分错误。

若签名足够但不足完整 1 MiB Header section，`finish()` **MUST** 返回“介质过小，无法包含 VHDX header section”的可区分错误。

---

## 6. 创建 API

`Medium::create(inner)` **MUST** 接收调用方已准备好的底层介质：

```rust
impl<T> Medium<T> {
    pub fn create(inner: T) -> CreateOptions<T>;
}
```

`CreateOptions<T>` 的 setter **MUST NOT** 要求 I/O 能力 bound。`finish()` **MUST** 要求完整写介质能力：

```rust
impl<T> CreateOptions<T> {
    pub fn size(self, size: u64) -> Self;
    pub fn fixed(self, fixed: bool) -> Self;
    pub fn block_size(self, block_size: u32) -> Self;
    pub fn logical_sector_size(self, logical_sector_size: u32) -> Self;
    pub fn physical_sector_size(self, physical_sector_size: u32) -> Self;
}

impl<T> CreateOptions<T>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
    pub fn finish(self) -> Result<Medium<T>>;
}
```

`Medium::create(inner)` **MUST NOT** 自动将底层介质截断到 0。清空、截断、打开模式和宿主权限 **MUST** 由调用方在创建底层介质时负责。

创建流程 **MUST** 按 VHDX 创建语义写入 header、region table、BAT、metadata、payload/fixed 区域，并通过 `SetLen` 设置最终长度。

---

## 7. 日志回放与可写打开

`LogReplayPolicy::Auto` **MUST** 保留双语义：

- 只读打开时，若存在可回放日志，**MUST** 构建内存 replay overlay，且 **MUST NOT** 写回底层介质；
- 可写打开时，若存在可回放日志，**MUST** 将日志回放写回底层介质，并使用 `SyncData` 稳定化。

`OpenOptions<T, ReadWrite>::finish()` **MUST** 执行 open-time header update。实现 **MUST** 在返回可写 `Medium<T>` 前更新 non-current header 并执行稳定写入。

可写打开即使调用方之后不写虚拟扇区，也 **MAY** 修改底层介质；这是 MS-VHDX header 更新语义的结果。

`InMemoryOnReadOnly` 与 `ReadOnlyNoReplay` **MUST** 继续仅允许只读打开。若用于可写打开，`finish()` **MUST** 返回策略冲突错误。

---

## 8. 结构面 API

### 8.1 Sections

`sections()` **MUST** 返回轻量的结构面容器；Header 之外的 Section 数据 **MUST** 在对应 accessor 首次访问时按需加载并缓存：

```rust
impl<T> Medium<T>
where
    T: Read + Seek,
{
    pub fn sections(&self) -> Result<Sections<'_, T>>;
}
```

`Sections<'_, T>` **MUST NOT** 长期持有 `&mut Medium<T>` 或公开锁 guard。实现 **SHOULD** 使用稳定缓存快照，使解析视图借用缓存字节而不复制 Section 内容。

### 8.2 Validator

`validator()` **MUST** 通过 `&mut self` 构建或读取 validator buffer，并返回持有稳定快照的 `Result<SpecValidator>`：

```rust
impl<T> Medium<T>
where
    T: Read + Seek,
{
    pub fn validator(&mut self) -> Result<SpecValidator>;
}
```

`SpecValidator` **MUST** 持有 validator buffer 的稳定快照，使后续 cache 失效或写入不会改变已创建 validator 的只读视图。

`validator()` **MUST** 使用 `OpenOptions::strict(...)` 配置的 strict 策略。

`validator().validate_file()` **MUST** 默认只校验当前介质结构，不得自动打开或解析父介质链。

parent-chain 校验 **MUST** 是显式 API，且 **MUST** 由调用方提供父介质解析策略。

---

## 9. 数据面 API

### 9.1 单一入口

实现 **MUST** 提供单一数据面入口 `io()`：

```rust
impl<T> Medium<T>
where
    T: Read + Seek,
{
    pub fn io(&mut self) -> Result<IO<'_, T>>;
}
```

`IO<'_, T>` **MUST** 长期独占借用 `&mut Medium<T>`。在 `IO` 存活期间，调用方不得同时调用 `sections()`、`validator()` 或其它需要 `&mut Medium<T>` 的结构面 API。

`IO` **MUST NOT** 实现 `Clone`。

### 9.2 Sector 标准 I/O traits

`Sector<'_, T>` **MUST** 在 `T: Read + Seek` 时实现 `std::io::Read` 与 `std::io::Seek`。

`Sector<'_, T>` **MUST** 在 `T: Read + Write + Seek + Len + SetLen + SyncData` 时实现 `std::io::Write`。

写入方法 **MUST** 在执行前检查 `Medium<T>` 的运行时 `write` 策略。若介质未以 `.write()` 打开，写入 **MUST** 返回 `Error::ReadOnly` 或等价可区分错误。

`Sector` **MUST NOT** 实现 `Clone`。

### 9.3 内部偏移 I/O

实现 **MUST** 统一通过内部 helper 执行 offset-based read/write：

```rust
fn read_exact_at<T: Read + Seek>(inner: &mut T, offset: u64, buf: &mut [u8]) -> io::Result<()>;
fn write_all_at<T: Write + Seek>(inner: &mut T, offset: u64, buf: &[u8]) -> io::Result<()>;
```

实现 **MUST** 删除平台特化的 `io::platform::{read_at, write_at}` 作为核心 I/O 路径。初版介质抽象 **MUST NOT** 依赖 `std::os::{unix,windows}::fs::FileExt` positioned I/O。

---

## 10. 父介质与 differencing

### 10.1 默认策略

`Medium<T>` **MUST NOT** 在打开阶段自动解析父盘。

`OpenOptions` 的默认父介质策略 **MUST** 是拒绝隐式父介质解析。若 differencing 读取实际需要父介质而调用方未提供 resolver，数据面读取 **MUST** 返回 `Error::ParentResolverRequired` 或等价可区分错误。

该错误 **MUST** 与 `ParentNotFound` 区分：

- `ParentResolverRequired`：调用方没有提供 resolver；
- `ParentNotFound`：resolver 已提供，但未能找到或打开父介质。

### 10.2 Resolver 位置

父介质 resolver **MUST** 配置在 `OpenOptions` 上，并随 `finish()` 进入 `Medium<T>`。

resolver **MUST NOT** 配置在 `IO` 上；`IO` 只负责在数据面需要父介质时懒解析。

`IO` **MUST** 持有已解析的父介质缓存，resolver 从所属 `Medium<T>` 取得。

`with_parent_resolver` **MUST** 使用 builder 风格消费并返回 `OpenOptions`：

```rust
impl<T, Mode> OpenOptions<T, Mode> {
    pub fn with_parent_resolver<R>(self, resolver: R) -> Self
    where
        R: ParentResolver + Send + 'static;
}
```

`with_parent_resolver` **MUST** 接收泛型 resolver 并在内部装箱。resolver trait object **MUST** 要求 `Send` 以保持 `Medium<T>` 的线程边界；**MUST NOT** 要求 `Sync`，默认 **MUST** 要求 `'static`。

### 10.3 ParentRequest

`ParentResolver` **MUST** 接收 `ParentRequest<'_>`，不得接收路径作为核心参数：

```rust
pub struct ParentRequest<'a> {
    pub locator: ParentLocator<'a>,
    pub expected_data_write_guid: Guid,
    pub child_logical_sector_size: u32,
    pub child_virtual_disk_size: u64,
}

pub trait ParentResolver {
    fn resolve_parent(&mut self, request: ParentRequest<'_>) -> Result<Box<dyn ParentMedium>>;
}
```

`ParentRequest` **MUST** 从 child metadata 与 parent locator 派生，不得依赖 child path。

### 10.4 ParentMedium

`ParentMedium` **MUST** 表示父级 VHDX 的虚拟数据面读取能力，而不是父级底层字节流：

```rust
pub trait ParentMedium {
    fn data_write_guid(&mut self) -> Result<Guid>;
    fn logical_sector_size(&mut self) -> Result<u32>;
    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> Result<()>;
}
```

`ParentMedium::read_sector` **MUST** 要求 `buf.len() == logical_sector_size() as usize`。不匹配时 **MUST** 返回 `Error::InvalidParameter` 或等价可区分错误。

核心实现 **MUST** 负责校验 parent linkage。resolver 只负责返回候选父介质。

若 child 与 parent logical sector size 不一致，核心实现 **MUST** 返回 `Error::ParentSectorSizeMismatch { child, parent }` 或等价可区分错误。

父介质解析 **MUST** 懒执行：只有当数据面读取命中 child 不包含的 sector 时，才调用 resolver。

成功解析、校验 linkage 与 sector size 后，`IO` **MUST** 缓存 `ParentMedium`。

### 10.5 路径 helper

本标准 **MUST NOT** 要求实现提供 `PathParentResolver`。

实现 **MAY** 在后续作为独立 helper 提供路径父介质 resolver，但该 helper **MUST NOT** 改变 `Medium<T>` 的路径无关核心语义。

---

## 11. Debug 与并发边界

`Medium<T>` **SHOULD** 实现 `Debug`，且 **MUST NOT** 要求 `T: Debug`。

`Debug` 输出 **MAY** 包含 `write`、`strict`、`log_replay_policy` 等策略字段，但 **MUST NOT** 打印底层介质内容或路径假设。

`Medium<T>` 类型定义和核心 API **MUST NOT** 要求 `T: Send + Sync + 'static`。`Medium<T>` 的 auto trait 行为 **MUST** 随 `T` 与内部字段自然推导。

---

## 12. 最小合规清单

- [ ] 公开核心类型为 `Medium<T>`，模块为 `medium`，crate root 不再导出 `File`。  
- [ ] `Medium<T>` 不保存路径或 storage location，不提供路径专用 open/create 入口。  
- [ ] `Medium::open(inner)` / `Medium::create(inner)` 消费底层介质。  
- [ ] 公开签名直接使用 `Read + Write + Seek`，不定义组合 trait。  
- [ ] 定义 `Len`、`SetLen`、`SyncData`，且无 blanket no-op `SyncData`。  
- [ ] 为 `std::fs::File` 与 `Cursor<Vec<u8>>` 提供必要能力实现。  
- [ ] `OpenOptions<T, Mode>` 使用只读/可写 type-state；只提供 `.write()`。  
- [ ] 可写 `finish()` 执行 header update 与稳定写入。  
- [ ] 只读 `Auto` 使用内存 replay overlay；可写 `Auto` replay 到介质。  
- [ ] `sections(&self)` 返回懒加载快照容器；`validator(&mut self)` 返回持有稳定 validator buffer snapshot 的校验器。
- [ ] `validator()` 默认只校验当前介质，parent-chain 校验显式触发。  
- [ ] 单一 `io()` 返回长期借用 `&mut Medium<T>` 的 `IO`。  
- [ ] `Sector` 实现 `Read + Seek`，在完整写能力下实现 `Write`，写入时检查 runtime `write`。  
- [ ] 删除 `io::platform::{read_at, write_at}` 核心路径，统一使用 `seek + read_exact/write_all` helper。  
- [ ] 默认父介质策略为 Deny，缺 resolver 时返回 `ParentResolverRequired`。  
- [ ] `ParentResolver` 返回 `Box<dyn ParentMedium>`；`IO` 懒解析并缓存父介质。  
- [ ] 核心负责 parent linkage 与 logical sector size 校验。
