# 解决计划：A2 / A3 / A4 API 一致性修复

> 目标：使代码实现与 `docs/plan/API.md` 计划一致，或更新计划文档以匹配合理的实际实现。
> 来源：`docs/plan/一致性分析报告2.md`

---

## 总览

| 编号 | 项目 | 当前（实际） | 目标（计划） | 方向 | 工作量 |
|------|------|-------------|-------------|------|--------|
| A2 | `FileTypeIdentifier::signature()` | `&[u8; 8]`（引用） | `[u8; 8]`（值） | **保留引用→更新计划** | 极小（文档） |
| A3 | `FileTypeIdentifier::creator()` | `String`（解码后） | `&'a [u8]`（原始字节） | **改代码匹配计划** | 小（代码+测试） |
| A4 | `ParentChainInfo::child()/parent()` | `&Path`（引用） | `PathBuf`（值） | **保留引用→更新计划** | 极小（文档） |

---

## A2: `signature()` 改为零拷贝视图

### 现状

```rust
pub struct FileTypeIdentifier<'a> {
    signature: [u8; 8],       // 从 raw 拷贝出来的 8 字节
    creator: &'a [u8],        // 零拷贝视图
    raw: &'a [u8],
}
```

`signature` 字段存储的是从 `raw` 中 `copy_from_slice` 出来的独立数组。`signature()` 方法返回 `&self.signature`（对内部数组的引用）。

### 目标

```rust
pub struct FileTypeIdentifier<'a> {
    signature: &'a [u8; 8],   // 直接引用 raw[0..8]，零拷贝
    creator: &'a [u8],
    raw: &'a [u8],
}
```

### 理由

- `creator` 已经是零拷贝视图，`signature` 也应统一为同样的零拷贝风格
- 消除不必要的 8 字节拷贝
- 生命周期约束由借用检查保证安全

### 修改清单

#### 文件 1: `src/sections/header.rs`

| 行号 | 修改内容 |
|------|----------|
| 164 | `signature: [u8; 8]` → `signature: &'a [u8; 8]` |
| 175 | `let signature = read_array::<8>(data, 0);` → `let signature = &data[0..8].try_into().unwrap();` |
| 193-195 | `pub const fn signature(&self) -> &[u8; 8] { &self.signature }` → 去掉多余的取引用，直接 `self.signature`（已经是 `&[u8; 8]`） |
| | 实际上 `&self.signature` 中的 `&` 已是多余的。改为 `self.signature` 即可 |

**注意**: `data[0..8].try_into()` 会 panic 如果 data 长度不足 8。而 `FileTypeIdentifier::new` 的调用方 `Header::file_type()` 传入的是 `&self.raw_data[0..FILE_TYPE_SIZE]`，其中 `FILE_TYPE_SIZE = 64KB`，所以安全。作为防御，也可以用 `data.get(0..8).and_then(|s| s.try_into().ok()).unwrap_or(&[0u8; 8])`。

#### 文件 2: `tests/api_surface_smoke.rs`

| 行号 | 修改内容 |
|------|----------|
| 422 | `let _sig: &[u8; 8] = fti.signature();` → 无需改动，类型不变 |

#### 文件 3: `docs/plan/API.md`

- 将 `pub fn signature(&self) -> [u8; 8]` 改为 `pub fn signature(&self) -> &[u8; 8]`

---

## A3: `creator()` 返回原始字节而非解码字符串

### 现状

```rust
// 返回解码后的 UTF-16 字符串
pub fn creator(&self) -> String {
    // UTF-16 LE 解码逻辑
}
```

### 目标

```rust
// 返回原始 UTF-16 LE 字节切片（与计划一致）
pub fn creator(&self) -> &'a [u8] {
    self.creator
}
```

### 理由

- API.md 计划明确要求返回 `&'a [u8]`（原始字节）
- 调用方需要字符串时可以自行解码（使用 `String::from_utf16_lossy` 等）
- 保持零拷贝语义：返回原始切片而非分配新 `String`

### 影响范围

`.creator()` 在整个活跃代码库中只有 **2 个调用点**：

| 文件 | 行号 | 修改内容 |
|------|------|----------|
| `src/sections/header.rs` | 201-209 | 整个方法体替换为 `self.creator` |
| `src/sections/header.rs` | 633 | 测试断言 `ft.creator()` → `ft.creator()` 仍返回 `&[u8]`，需调整比较逻辑 |
| `tests/api_surface_smoke.rs` | 423 | `let _creator: String = fti.creator()` → `let _creator: &[u8] = fti.creator()` |

### 测试调整

`src/sections/header.rs:633` 原为：
```rust
assert_eq!(ft.creator(), "TestCreator");
```

改为与原始字节比较：
```rust
// "TestCreator" 的 UTF-16 LE 编码
let expected: Vec<u8> = "TestCreator".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
assert_eq!(ft.creator(), &expected[..]);
```

或者在对应位置添加一个辅助函数 `decode_utf16_le(bytes: &[u8]) -> String` 共测试使用。

---

## A4: `ParentChainInfo` 保留引用，更新计划

### 现状

```rust
pub fn child(&self) -> &Path { &self.child }
pub fn parent(&self) -> &Path { &self.parent }
```

### 目标

不修改代码，仅更新 `docs/plan/API.md`，将：
```rust
pub fn child(&self) -> PathBuf;
pub fn parent(&self) -> PathBuf;
```
改为：
```rust
pub fn child(&self) -> &Path;
pub fn parent(&self) -> &Path;
```

### 理由

- `&Path` 是 Rust 借用惯用做法：内部已存储 `PathBuf`，返回引用避免不必要的克隆
- 调用方需要所有权时可自行 `to_path_buf()`
- 更灵活，不破坏任何现有代码

---

## Z: 全部 struct 内部 `[u8; N]` 字段零拷贝化

### 说明

除 FileTypeIdentifier（已在 A2 中处理）外，还有 **7 个 struct** 的 `signature`（以及 TableHeader 的 `reserved` / `reserved2`）字段存储的是从 `data` 拷贝出来的 `[u8; N]`。这些都可以改为 `&'a [u8; N]` 引用，直接从原始字节切片借用，消除不必要的 memcpy。

**公共方法签名不变**（已经返回 `&[u8; N]`），纯内部优化。

### 修改清单

#### Z1. `HeaderStructure` — `src/sections/header.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 254 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 288 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 310-311 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

#### Z2. `RegionTableHeader` — `src/sections/header.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 495 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 512 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 528-529 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

#### Z3. `TableHeader` — `src/sections/metadata.rs`

这个 struct 有三个 `[u8; N]` 字段：

| 行号 | 当前 | 改为 |
|------|------|------|
| 165 | `signature: [u8; 8]` | `signature: &'a [u8; 8]` |
| 167 | `reserved: [u8; 2]` | `reserved: &'a [u8; 2]` |
| 171 | `reserved2: [u8; 20]` | `reserved2: &'a [u8; 20]` |
| 180 | `let signature = read_array::<8>(data, 0);` | `let signature: &[u8; 8] = data[0..8].try_into().unwrap();` |
| 181 | `let reserved = read_array::<2>(data, 8);` | `let reserved: &[u8; 2] = data[8..10].try_into().unwrap();` |
| 183 | `let reserved2 = read_array::<20>(data, 12);` | `let reserved2: &[u8; 20] = data[12..32].try_into().unwrap();` |
| 201-202 | `pub const fn signature(&self) -> &[u8; 8] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 8] { self.signature }` |
| 213-214 | `pub const fn reserved(&self) -> &[u8; 2] { &self.reserved }` | `pub const fn reserved(&self) -> &[u8; 2] { self.reserved }` |
| 219-220 | `pub const fn reserved2(&self) -> &[u8; 20] { &self.reserved2 }` | `pub const fn reserved2(&self) -> &[u8; 20] { self.reserved2 }` |

#### Z4. `LogEntryHeader` — `src/sections/log.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 702 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 731 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 753-754 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

#### Z5. `DataDescriptor` — `src/sections/log.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 867 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 891 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 908-909 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

#### Z6. `ZeroDescriptor` — `src/sections/log.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 948 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 973 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 990-991 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

#### Z7. `DataSector` — `src/sections/log.rs`

| 行号 | 当前 | 改为 |
|------|------|------|
| 1026 | `signature: [u8; 4]` | `signature: &'a [u8; 4]` |
| 1050 | `signature: read_array::<4>(data, 0),` | `signature: data[0..4].try_into().unwrap(),` |
| 1066-1067 | `pub const fn signature(&self) -> &[u8; 4] { &self.signature }` | `pub const fn signature(&self) -> &[u8; 4] { self.signature }` |

### 注意事项

1. **安全性**: 所有调用方传入的 `data` 长度均已在构造前验证（`HeaderStructure::new` 校验 `data.len() == HEADER_SIZE`，`DataDescriptor::new` 校验 `data.len() >= 32` 等），因此 `data[0..4].try_into().unwrap()` 不会 panic。

2. **`#[derive(Clone, Copy)]`**: 受影响 struct 中只有 `RegionTableHeader` 标注了 `#[derive(Clone, Copy)]`（header.rs:492）。字段从 `[u8; 4]` 改为 `&'a [u8; 4]` 后，引用不再 `Copy`，需要去掉 `Copy` 派生或改为手动实现 Clone。经检查，`RegionTableHeader` 的 `Copy` 仅在 `RegionTable::new` 内部使用（`header.rs:431-432`），去掉 `Copy` 不影响外部调用方。

3. **`read_array` 的消亡**: 所有 `[u8; N]` 拷贝都改为零借用后，`read_array` 函数在 `header.rs` 和 `log.rs` 中将不再被使用，可以删除。`metadata.rs` 中仍被 `TableEntry::new` 中的其它字段（`offset`, `length`, `flags`, `reserved` 等 4 字节字段的默认值填充）使用，暂时保留。

---

## 执行顺序

```
A4 (文档-only) ──┐
                  ├── 同时执行，互不依赖
A2+A3 (代码+文档) ─┤
                   │
Z1-Z7 (纯代码) ────┘
```

A2/A3/A4/Z 四者互不依赖，可以并行。建议先做 A2+A3+Z（代码修改），A4（仅文档）可随时完成。

---

## 验证标准

### A2 验证
- [ ] `cargo build` 通过
- [ ] `cargo test -p vhdx-rs` 通过（`test_file_type_identifier` 等）
- [ ] `tests/api_surface_smoke.rs` 中 `let _sig: &[u8; 8] = fti.signature()` 编译通过

### A3 验证
- [ ] `cargo build` 通过
- [ ] `cargo test -p vhdx-rs` 通过（涉及 `FileTypeIdentifier` 的测试需更新断言）
- [ ] `tests/api_surface_smoke.rs` 中 `let _creator: &[u8] = fti.creator()` 编译通过

### A4 验证
- [ ] `cargo build` 通过（仅文档变更，无代码修改）

### Z1-Z7 验证
- [ ] `cargo build` 通过
- [ ] `cargo test --workspace` 全部通过
- [ ] `RegionTableHeader` 去掉 `Copy` 后无编译错误

---

## 文件修改总览

| 文件 | A2 | A3 | A4 | Z1 | Z2 | Z3 | Z4 | Z5 | Z6 | Z7 |
|------|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| `src/sections/header.rs` | ✅ | ✅ | - | ✅ | ✅ | - | - | - | - | - |
| `src/sections/metadata.rs` | - | - | - | - | - | ✅ | - | - | - | - |
| `src/sections/log.rs` | - | - | - | - | - | - | ✅ | ✅ | ✅ | ✅ |
| `tests/api_surface_smoke.rs` | - | ✅ | - | - | - | - | - | - | - | - |
| `docs/plan/API.md` | ✅ | ✅ | ✅ | - | - | - | - | - | - | - |

---

## 文档版本

- **计划版本**: 1.0
- **编制日期**: 2026-05-01
- **关联报告**: `docs/plan/一致性分析报告2.md`
