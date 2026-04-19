# AGENTS.md — vhdx-rs 项目知识库

> AI Agent 在本项目中工作时，请先阅读此文件。本文件提供项目全貌、约定和关键决策的快速索引。

## 项目概述

**vhdx-rs** 是 VHDX (Virtual Hard Disk v2) 文件格式的 Rust 实现。VHDX 是 Microsoft 定义的虚拟硬盘格式，本库提供纯 Rust 的解析、创建和读写能力。

- **规范参考**: Microsoft MS-VHDX 协议（`misc/MS-VHDX.md`）
- **Rust Edition**: 2024
- **Workspace**: 两个 crate — `vhdx-rs`（库）+ `vhdx-tool`（CLI）
- **许可证**: 待定

## 构建 & 测试

```bash
cargo build                              # 构建库
cargo build -p vhdx-tool                 # 构建 CLI（注意包名是 vhdx-tool，不是 vhdx-cli）
cargo test --workspace                   # 运行全部测试（约 81 个）
cargo test -p vhdx-rs                    # 仅库测试
cargo test -p vhdx-tool                  # 仅 CLI 测试
cargo clippy --workspace                 # Lint 检查
cargo fmt --check                        # 格式检查
```

**平台注意**: Windows 上 PowerShell 不支持 `VAR=val cmd` 语法，需用 `$env:VAR='val'; cmd`。

## 项目结构

```
vhdx-rs/
├── src/                        # 📦 库 crate (vhdx-rs)
│   ├── lib.rs                  #   入口，pub use 重导出所有公共类型
│   ├── file.rs                 #   File / OpenOptions / CreateOptions — 文件操作核心
│   ├── io_module.rs            #   IO / Sector / PayloadBlock — 扇区/块级 IO
│   ├── sections.rs             #   Sections / SectionsConfig — 区域聚合容器
│   ├── sections/
│   │   ├── header.rs           #   Header / HeaderStructure / TableHeader — 头部解析
│   │   ├── bat.rs              #   Bat / BatEntry / BatEntryIter — 块分配表
│   │   ├── log.rs              #   Log / LogEntry / LogEntryHeader — 日志解析与回放
│   │   └── metadata.rs         #   Metadata / MetadataTable / FileParameters — 元数据
│   ├── types.rs                #   Guid — 128 位 GUID 类型
│   ├── error.rs                #   Error / Result — 统一错误类型（thiserror）
│   └── common/
│       ├── constants.rs        #   布局常量、签名常量、对齐函数
│       └── mod.rs              #   区域 GUID、辅助工具
├── vhdx-cli/                   # 🛠️ CLI crate (vhdx-tool)
│   └── src/
│       ├── main.rs             #   入口
│       ├── cli.rs              #   clap derive 定义（Commands 枚举）
│       ├── commands/           #   各子命令实现
│       │   ├── mod.rs          #     命令分发
│       │   ├── info.rs         #     info — 显示文件信息
│       │   ├── create.rs       #     create — 创建虚拟磁盘
│       │   ├── check.rs        #     check — 检查完整性
│       │   ├── repair.rs       #     repair — 修复（日志回放）
│       │   ├── sections_cmd.rs #     sections — 查看区域详情
│       │   └── diff.rs         #     diff — 差异磁盘操作
│       └── utils/
│           ├── mod.rs          #     工具函数入口
│           └── size.rs         #     大小解析（parse_size, parse_block_size）
├── tests/
│   └── integration_test.rs     # 集成测试（创建/读写/验证）
├── docs/
│   └── API.md                  # 库 API 参考文档（API 树格式）
├── misc/                       # 规范文档与测试文件（勿修改）
│   ├── MS-VHDX.md              #   VHDX 规范参考
│   ├── docs/                   #   额外文档
│   ├── imitation/              #   参考实现
│   └── *.vhdx                  #   测试用 VHDX 文件
├── Cargo.toml                  # Workspace 根配置
├── rustfmt.toml                # 格式化配置
├── examples/                   # 示例代码（当前为空）
└── README.md                   # 中文 README
```

## 核心架构

### 数据流

```
用户代码
  ↓
File::open() / File::create()
  ↓
Sections（延迟加载容器）
  ├── Header（双头安全机制：Header1 + Header2，取较新者）
  ├── RegionTable（双区域表：RT1 + RT2）
  ├── Bat（块分配表：映射虚拟块 → 物理偏移）
  ├── Metadata（元数据：磁盘类型、大小、块大小等）
  └── Log（日志：崩溃一致性恢复）
  ↓
IO（扇区/块级读写）
  ↓
操作系统文件
```

### 关键设计决策

| 决策 | 说明 |
|------|------|
| **Builder 模式** | `File::open()` 返回 `OpenOptions`，`File::create()` 返回 `CreateOptions`，均通过 `.finish()` 完成操作 |
| **延迟加载** | `Sections` 各区域按需解析，非一次性全部加载 |
| **双头安全** | Header 1 / Header 2 互为备份，取 `sequence_number` 较大者 |
| **CRC-32C 校验** | 所有结构体使用 CRC-32C 校验和验证完整性 |
| **日志回放** | 打开文件时自动检测未完成日志，需要显式回放 |
| **thiserror 错误** | `Error` 枚举 20 个变体，覆盖 IO / 格式 / 状态 / 参数四类错误 |

### VHDX 文件类型

| 类型 | 枚举值 | 特点 |
|------|--------|------|
| Fixed | `disk_type: fixed` | 固定大小，数据连续存储，性能最佳 |
| Dynamic | `disk_type: dynamic`（默认） | 按需分配数据块，节省空间 |
| Differencing | `disk_type: differencing` | 引用父磁盘，支持快照 |

## 编码约定

### 语言规则（严格遵守）

| 规则 | 说明 |
|------|------|
| **注释语言** | 所有 `///` rustdoc 和 `//` 行内注释必须使用**中文** |
| **CLI help 属性** | `#[command(about = "...")]` 和 `#[arg(help = "...")]` 必须使用**英文** |
| **CLI 双注释模式** | 每个 CLI struct/variant/field 需同时有：中文 `///` 注释 + 英文 `about`/`help` 属性 |
| **错误消息** | `#[error("...")]` 保持英文 |
| **输出文本** | `println!` 和 JSON 键名保持英文 |
| **代码标识符** | 变量名、函数名、类型名保持英文 |

```rust
// ✅ 正确：库代码注释风格
/// VHDX 虚拟硬盘文件句柄
///
/// 提供对 VHDX 文件的完整操作能力。
pub struct File { ... }

// ✅ 正确：CLI 双注释模式
/// 显示 VHDX 文件信息
#[command(about = "Display VHDX file information")]
Info {
    /// VHDX 文件路径
    #[arg(help = "Path to the VHDX file")]
    file: PathBuf,
},
```

### 注释深度

- 每个公共类型必须有 `///` 文档注释
- 多字段结构体必须有**逐字段** `///` 注释
- 关键算法和格式细节应引用 MS-VHDX 规范章节（如 `MS-VHDX §2.2.2`）
- 模块级 `//!` 注释概述模块职责和主要类型

### 代码风格

- **Formatter**: `rustfmt` with `edition = "2024"`, Unix 换行符
- **Lint**: `clippy::all = warn`, `clippy::pedantic = warn`
- **Error handling**: 使用 `crate::error::Result<T>`，不使用 `anyhow`
- **No type suppression**: 禁止 `as any`、`@ts-ignore`、`#[allow(clippy::...)]`（除非有充分理由）

### 禁止事项

| 禁止 | 原因 |
|------|------|
| 修改 `misc/` 目录 | 规范文档和参考文件，不可更改 |
| 添加新 dependencies | 保持依赖最小化 |
| 修改代码逻辑（仅注释任务时） | 注释任务不得改变行为 |
| 更改函数签名或重命名公共项 | 破坏 API 兼容性 |
| 删除测试来通过构建 | 必须修复代码而非测试 |

## 依赖关系

### 库 (vhdx-rs)

| 依赖 | 版本 | 用途 |
|------|------|------|
| `uuid` | 1.22 | GUID 生成（v4 feature） |
| `thiserror` | 2.0.18 | 派生 Error trait |
| `byteorder` | 1.5 | 大/小端字节序读写 |
| `crc32c` | 0.6 | CRC-32C 校验和计算 |

### CLI (vhdx-tool)

| 依赖 | 版本 | 用途 |
|------|------|------|
| `vhdx-rs` | path | 库（本地路径依赖） |
| `clap` | 4.6 (derive) | 命令行参数解析 |
| `byte-unit` | 5.2 (byte) | 人类可读大小解析 |

### 开发依赖

**库 (vhdx-rs)**

| 依赖 | 版本 | 用途 |
|------|------|------|
| `tempfile` | 3.27 | 测试用临时文件 |

**CLI (vhdx-tool)**

| 依赖 | 版本 | 用途 |
|------|------|------|
| `assert_cmd` | 2 | CLI 集成测试 |
| `tempfile` | 3 | CLI 测试临时文件 |
| `predicates` | 3 | 测试断言 |

## 公共 API 概览

### 主要类型（36 个公共类型）

```
vhdx_rs::
├── File                    # 文件句柄（open/create/read/write）
├── IO                      # 扇区/块级 IO
├── Guid                    # 128 位 GUID
├── Error / Result          # 错误处理
├── Sections                # 区域聚合容器
├── Header / HeaderStructure / TableHeader
├── RegionTable / RegionTableEntry / RegionTableHeader
├── Bat / BatEntry
├── Metadata / MetadataTable / MetadataItems
├── FileParameters / FileTypeIdentifier
├── Log / LogEntry / LogEntryHeader
├── Sector / PayloadBlock
├── DataDescriptor / DataSector / Descriptor / ZeroDescriptor
├── EntryFlags / PayloadBlockState / SectorBitmapState
├── KeyValueEntry / LocatorHeader / ParentLocator
└── BatState / TableEntry
```

完整 API 参考：[`docs/API.md`](docs/API.md)

### Builder 模式

```rust
// 以只读模式打开（默认）
let file = File::open("disk.vhdx").finish()?;

// 以写入模式打开
let file = File::open("disk.vhdx").write().finish()?;

// 创建 Dynamic 类型（默认）
let file = File::create("new.vhdx")
    .size(10 * 1024 * 1024 * 1024)     // 必须：虚拟磁盘大小（字节）
    .finish()?;

// 创建 Fixed 类型
let file = File::create("fixed.vhdx")
    .size(10 * 1024 * 1024 * 1024)
    .fixed(true)                        // 可选：Fixed 类型
    .block_size(32 * 1024 * 1024)       // 可选：块大小（字节）
    .finish()?;

// 创建 Differencing 类型
let file = File::create("diff.vhdx")
    .size(10 * 1024 * 1024 * 1024)
    .has_parent(true)                   // 可选：标记为差分磁盘
    .finish()?;
```

## 测试

- **位置**: `tests/integration_test.rs`（集成测试），各模块内 `#[cfg(test)] mod tests`（单元测试）
- **数量**: 约 81 个测试
- **模式**: 创建 VHDX 文件 → 执行操作 → 验证结果
- **临时文件**: 使用 `tempfile::tempdir()` + `std::mem::forget()` 防止自动清理

```bash
cargo test --workspace     # 运行全部
cargo test test_create     # 过滤测试名
```

## CLI 命令

```bash
vhdx-tool info <file> [--format json|text]     # 显示文件信息
vhdx-tool create <path> --size <SIZE>           # 创建虚拟磁盘
  [--type dynamic|fixed|differencing]           # 主参数，优先级最高
  [--disk-type dynamic|fixed|differencing]      # 兼容别名，与 --type 同时给出时会被忽略
  [--block-size <SIZE>]
  [--parent <PATH>]
  [--force]                                     # 仅允许覆盖已存在目标文件
vhdx-tool check <file> [--repair] [--log-replay] # 检查完整性
vhdx-tool repair <file> [--dry-run]              # 修复（日志回放）
vhdx-tool sections <file> <section>              # 查看区域详情
  section: header | bat | metadata | log
vhdx-tool diff <file> parent                     # 查看父磁盘定位器信息
vhdx-tool diff <file> chain                      # 查看磁盘链
```

## 已知问题

| 问题 | 位置 | 说明 |
|------|------|------|
| rustdoc warning | `src/error.rs:20` | `Error` 枚举与 `thiserror::Error` 宏同名，`[Error]` 交叉引用有歧义，可改为 `[enum@Error]` 修复 |
| `BatEntryIter` 不可直接导入 | `src/sections/bat.rs` | 迭代器类型在私有模块中，未通过 `pub use` 重导出，但可通过 `Bat` 的迭代方法使用 |
| `SectionsConfig` 不可直接导入 | `src/sections.rs` | 内部配置类型，未在 `lib.rs` 重导出 |
| `OpenOptions` / `CreateOptions` 不可直接导入 | `src/file.rs` | Builder 返回类型，由 `File::open()` / `File::create()` 返回，未 `pub use` 重导出 |
| Dynamic 磁盘读取返回零 | `src/file.rs:186` | 按块读取尚未实现，Dynamic 类型读取时填充零而非实际数据 |
| Dynamic 块分配未完全实现 | `src/file.rs:248-256` | 写入未分配的块或超出 BAT 范围的块会返回错误 |

## Git 历史

最近的工作集中在中文注释和文档：

```
346c56a 文档
52a19a9 注释补全
7d93467 语言问题
946ff33 docs(tests,readme): 添加测试注释并创建中文 README.md
64ab36c docs(cli): 添加 CLI 工具中文注释并中文化帮助文本
5fd7df2 docs(lib): 添加高层封装、IO 与入口模块的中文注释
cf98860 docs(lib): 添加 VHDX 结构解析模块的中文注释
8bcb19e docs(lib): 添加基础类型与常量的中文注释
```

## 文件修改红线

以下文件/目录在任何情况下**不得修改**（除非明确要求）：

- `misc/` — 规范文档和参考文件
- `Cargo.toml` / `vhdx-cli/Cargo.toml` — 依赖配置（除非明确要求添加依赖）
- `rustfmt.toml` — 格式化配置

## 常用参考

| 需要了解 | 去哪里看 |
|----------|----------|
| 公共 API 完整列表 | `docs/API.md` |
| VHDX 文件格式规范 | `misc/MS-VHDX.md` |
| 库入口和重导出 | `src/lib.rs` |
| 错误类型定义 | `src/error.rs` |
| CLI 命令定义 | `vhdx-cli/src/cli.rs` |
| 格式常量和签名 | `src/common/constants.rs` |
| 区域 GUID | `src/common/mod.rs` |
| 库使用示例 | `README.md` |
