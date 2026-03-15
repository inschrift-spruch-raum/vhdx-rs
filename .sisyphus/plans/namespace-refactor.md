# vhdx-rs 命名空间重构计划（修订版）

## TL;DR

> **Quick Summary**: 将 vhdx-rs 从 `VhdxXxx` 命名风格重构为 `vhdx::Xxx` 风格，保持平铺目录结构，通过 `lib.rs` 控制可见性，简化公共API，CLI 分离到独立 workspace crate。
> 
> **Deliverables**: 
> - 精简的 `lib.rs` 仅暴露核心API（Error, File, Builder, DiskType）
> - **CLI 分离为独立 workspace crate** (`cli/`)，删除 `write` 命令
> - `File::open(read_only)` 参数改为 `File::open(write)`（参数名变更）
> - 添加 `File::check()` 方法用于文件完整性检查
> - 内部模块通过 `mod` 声明但不 `pub` 导出
> - 重命名所有 `VhdxXxx` 类型为 `Xxx`
> - 统一 `DiskType` 定义（消除重复）
> - 更新所有测试和文档
> - **不创建 prelude 模块**
> - **保持 block_io/ 目录名不变**
> 
> **Estimated Effort**: Large (~2-3 days)
> **Parallel Execution**: YES - 5 waves
> **Critical Path**: Wave 1 (Type renames) → Wave 2 (lib.rs exports) → Wave 3 (CLI) → Wave 4 (Tests) → Wave 5 (Final verification)

---

## Context

### Original Request
用户希望重新规划 vhdx-rs 项目的命名空间，具体需求：
1. 让公共API更清晰
2. 从 `VhdxXxx` 命名风格改为 `vhdx::Xxx` 模块路径风格
3. Breaking change，无需考虑向后兼容
4. CLI 分离为独立 workspace crate，删除 write 命令
5. **保持平铺目录结构**（不需要 core/ 子目录）
6. 保持 `block_io/` 目录名不变
7. 不创建 prelude 模块
8. 添加 `File::check()` 方法
9. `File::open(path, read_only)` 参数改为 `File::open(path, write)`（参数名变更，语义不变）

### Interview Summary
**Key Decisions**:
- 命名风格：采用 Rust idiomatic 的模块路径风格，去除 Vhdx 前缀
- 类型重命名：`VhdxError` → `Error`, `VhdxFile` → `File`, `VhdxBuilder` → `Builder`
- 模块组织：**保持平铺结构**，通过 `lib.rs` 控制可见性
- CLI：独立 workspace crate (`cli/`)，命令保留 `info`, `create`, `read`, `check`，删除 `write`
- API 变更：`open(path, read_only)` → `open(path, write)`（参数名变更），添加 `check()`
- 无 prelude 模块
- 保持 `block_io/` 目录名

**Test Strategy**: 重构时同步修复测试
**Documentation**: 同步更新 doc comments

### Current Structure Analysis
当前为单一 Rust crate，9个顶层模块直接暴露：
- `common/`: GUID, CRC, DiskType (与 file/mod.rs 重复定义)
- `header/`, `bat/`, `log/`, `metadata/`, `payload/`: 内部实现细节
- `block_io/`: I/O抽象层
- `file/`: 高层API + CLI逻辑混杂在 main.rs

**当前 lib.rs 导出**:
```rust
pub mod bat;
pub mod block_io;
pub mod common;
pub mod error;
pub mod file;
pub mod header;
pub mod log;
pub mod metadata;
pub mod payload;
```

### Target Structure（平铺）
```
Cargo.toml              # workspace root
├── vhdx-rs/            # 库 crate
│   ├── Cargo.toml      # 无 clap 依赖
│   └── src/
│       ├── lib.rs              # 精简导出：Error, File, Builder, DiskType
│       ├── error.rs            # Error 类型 (原 VhdxError)
│       ├── common/             # 内部模块（不 pub 导出）
│       ├── header/             # 内部模块（不 pub 导出）
│       ├── bat/                # 内部模块（不 pub 导出）
│       ├── log/                # 内部模块（不 pub 导出）
│       ├── metadata/           # 内部模块（不 pub 导出）
│       ├── payload/            # 内部模块（不 pub 导出）
│       ├── block_io/           # 保持原名，内部模块
│       └── file/               # 高层API（pub 导出）
│           ├── mod.rs
│           ├── file.rs         # File 类型，添加 check() 方法
│           └── builder.rs      # Builder 类型
└── cli/                        # CLI crate
    ├── Cargo.toml              # 依赖 clap 和 vhdx-rs
    └── src/
        └── main.rs             # 简化入口，命令：info, create, read, check
```

**目标 lib.rs 导出**:
```rust
// 内部模块（不 pub，仅 crate 内可见）
mod bat;
mod block_io;
mod common;
mod error;
mod file;
mod header;
mod log;
mod metadata;
mod payload;

// 公共导出 - 仅核心类型
pub use error::Error;
pub use file::{DiskType, File, Builder};
```

**目标 File API**:
```rust
impl File {
    /// 打开文件用于读写（原 open(path, read_only: false)）
    pub fn write<P: AsRef<Path>>(path: P) -> Result<Self>;
    
    /// 检查文件完整性
    pub fn check(&self) -> Result<CheckReport>;
    
    // 原有方法保持不变
    pub fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    pub fn virtual_disk_size(&self) -> u64;
    pub fn block_size(&self) -> u32;
    pub fn disk_type(&self) -> DiskType;
    pub fn has_parent(&self) -> bool;
}
```

---

## Work Objectives

### Core Objective
重构 vhdx-rs 的命名空间和模块可见性，从 `VhdxXxx` 风格转变为 `vhdx::Xxx` 风格，CLI 分离为独立 workspace crate，添加 `File::check()` 方法，修改 `open()` 参数名。

### Concrete Deliverables
1. **精简的 `lib.rs`** - 仅导出 `Error`, `File`, `Builder`, `DiskType`
2. **保持平铺目录结构** - 不创建 core/ 子目录
3. **保持 `block_io/` 目录名** - 不重命名为 io/
4. **类型重命名** - 所有 `VhdxXxx` → `Xxx`
5. **模块可见性控制** - 内部模块改为 `mod`（非 `pub mod`）
6. **API 变更**:
   - `VhdxFile::open(path, read_only)` → `File::open(path, write)`（参数名从 `read_only` 改为 `write`，语义不变）
   - 添加 `File::check()` 方法用于文件完整性检查
7. **CLI 分离** - 创建独立 workspace crate (`cli/`)，从库中移除 CLI 代码和 clap 依赖
8. **CLI 简化** - 命令保留 `info`, `create`, `read`, `check`，删除 `write`
9. **无 prelude 模块**
10. **重复定义清理** - 统一 `DiskType` 定义
11. **测试同步修复** - 所有测试适配新API
12. **文档更新** - doc comments 中的类型引用同步更新

### Definition of Done
- [x] `cargo build --workspace` 成功，无警告
- [x] `cargo test --workspace` 全部通过
- [x] `cargo doc --workspace` 生成文档，无 broken links
- [x] `cargo clippy --workspace` 无警告
- [x] CLI 工具功能完整（info, create, read, check）
- [x] 所有 `VhdxXxx` 类型已重命名
- [x] 内部模块不再直接暴露（非 pub）
- [x] `DiskType` 重复定义已消除
- [x] 目录结构保持平铺
- [x] `File::check()` 方法可用
- [x] `File::open(path, write)` API 正确（原 read_only 参数）

### Must Have
- [x] lib.rs 仅导出必要公共API（无 prelude）
- [x] 类型重命名（VhdxXxx → Xxx）
- [x] 内部模块可见性改为非 pub
- [x] CLI 重构为独立 workspace crate
- [x] CLI 删除 write 命令
- [x] `File::open` 参数从 `read_only` 改为 `write`
- [x] 添加 `File::check()` 方法
- [x] DiskType 重复定义统一
- [x] 测试同步修复
- [x] doc comments 更新
- [x] 库 Cargo.toml 移除 clap 依赖

### Must NOT Have (Guardrails)
- [x] 不改变任何内部实现逻辑（只改命名和可见性）
- [x] 不新增或删除功能（除 API 变更外）
- [x] 不创建 core/ 或其他嵌套目录
- [x] 不把简单任务过度拆分
- [x] 不保留旧的 `Vhdx` 前缀别名（breaking change，彻底清理）
- [x] 不重命名 block_io/ 目录
- [x] 不创建 prelude 模块

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed.

### Test Decision
- **Infrastructure exists**: YES (Rust built-in test)
- **Automated tests**: Tests-after (重构时同步修复)
- **Framework**: `cargo test --workspace`

### QA Policy
Every task MUST include agent-executed QA scenarios.

- **Rust代码**: Use `cargo build`, `cargo test`, `cargo doc`, `cargo clippy`
- **CLI测试**: Use Bash to run CLI commands and verify output
- **Evidence**: Terminal output for CLI, build logs

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1: Type Renames (Foundation - MAX PARALLEL):
├── Task 1: Rename VhdxError to Error [quick]
├── Task 2: Rename VhdxFile to File (in file/vhdx_file.rs) [quick]
├── Task 3: Rename VhdxBuilder to Builder (in file/builder.rs) [quick]
├── Task 4: Unify DiskType (remove duplicate from common/) [quick]
├── Task 5: Rename types in header/ (VhdxHeader → Header) [quick]
├── Task 6: Rename types in bat/ [quick]
└── Task 7: Rename types in metadata/ [quick]

Wave 2: lib.rs and Module Visibility (3 tasks):
├── Task 8: Rewrite lib.rs with minimal exports [unspecified-high]
├── Task 9: Update all internal imports to use crate:: [unspecified-high]
└── Task 10: Add File::check() method [quick]

Wave 3: API Changes (2 tasks):
├── Task 11: Rename File::open parameter from read_only to write [unspecified-high]
└── Task 12: Update tests for new API [unspecified-high]

Wave 4: CLI Refactoring (4 tasks):
├── Task 13: Create workspace structure and cli/ crate [quick]
├── Task 14: Migrate CLI logic to cli/ (delete write command) [unspecified-high]
├── Task 15: Remove CLI code and clap from library [quick]
└── Task 16: Update workspace Cargo.toml [quick]

Wave 5: Test Synchronization (depends on Waves 1-4):
├── Task 17: Fix tests in tests/ directory [unspecified-high]
├── Task 18: Fix inline tests in src/ [unspecified-high]
└── Task 19: Run full test suite [unspecified-high]

Wave 6: Documentation and Final Verification (5 tasks):
├── Task 20: Update doc comments [unspecified-high]
├── Task 21: Run cargo doc and fix warnings [quick]
├── Task 22: Run cargo clippy and fix warnings [quick]
├── Task 23: Final build and test verification [quick]
└── Task 24: Git cleanup [quick]

Wave FINAL: Independent Review (4 parallel):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: API surface review (deep)
└── Task F4: Integration test (unspecified-high)

Critical Path: Wave 1 → Wave 2 → Wave 3 → Wave 4 → Wave 5 → Wave 6 → F1-F4
Parallel Speedup: ~50% faster than sequential
Max Concurrent: 7 (Wave 1)
```

### Dependency Matrix

- **1-7**: — — 8-9, 11, 13-16
- **8**: 1-7 — 9, 11
- **9**: 8 — 11, 17-19
- **10**: 1-7 — 17-19 (can be parallel with 11)
- **11**: 8-9 — 12, 17-19
- **12**: 11 — 17-19
- **13**: — — 14-16
- **14**: 13 — 15-16
- **15**: 14 — 16
- **16**: 15 — 17-19
- **17-19**: 1-16 — 20, F1-F4
- **20**: 17-19 — 21-24, F1-F4
- **21-24**: 20 — F1-F4

### Agent Dispatch Summary

- **Wave 1**: 7 tasks → all `quick`
- **Wave 2**: 3 tasks → T8-9 `unspecified-high`, T10 `quick`
- **Wave 3**: 2 tasks → T11-12 `unspecified-high`
- **Wave 4**: 4 tasks → T13,15-16 `quick`, T14 `unspecified-high`
- **Wave 5**: 3 tasks → all `unspecified-high`
- **Wave 6**: 5 tasks → T20 `unspecified-high`, T21-24 `quick`
- **FINAL**: 4 tasks → F1 `oracle`, F2,4 `unspecified-high`, F3 `deep`

---

## TODOs

### Wave 1: Type Renames (7 parallel tasks)

- [x] 1. Rename VhdxError to Error

  **What to do**:
  - In `src/error.rs`, rename `pub enum VhdxError` to `pub enum Error`
  - Update all references to this type in the codebase

  **Must NOT do**:
  - 不改变错误类型的内部逻辑或字段

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 8-9, 11, 17-19
  - **Blocked By**: None

  **References**:
  - File: `src/error.rs`
  - Type: `pub enum VhdxError`

  **Acceptance Criteria**:
  - [ ] `VhdxError` renamed to `Error` in error.rs
  - [ ] All references updated
  - [ ] `cargo check` passes

  **QA Scenarios**:
  ```
  Scenario: Error type renamed
    Tool: Bash
    Steps:
      1. `grep "pub enum Error" src/error.rs`
      2. `cargo check --lib`
    Expected Result: "pub enum Error" found, check passes
    Evidence: .sisyphus/evidence/task-01-error.txt
  ```

  **Commit**: YES
  - Message: `refactor(error): rename VhdxError to Error`

- [x] 2. Rename VhdxFile to File

  **What to do**:
  - In `src/file/vhdx_file.rs`, rename `pub struct VhdxFile` to `pub struct File`
  - Rename the file from `vhdx_file.rs` to `file.rs`
  - Update `src/file/mod.rs` to export `File` instead of `VhdxFile`
  - Update all references across the codebase

  **Must NOT do**:
  - 不改变 File 结构的字段或方法

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] File renamed from vhdx_file.rs to file.rs
  - [ ] `VhdxFile` renamed to `File`
  - [ ] All references updated
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(file): rename VhdxFile to File`

- [x] 3. Rename VhdxBuilder to Builder

  **What to do**:
  - In `src/file/builder.rs`, rename `pub struct VhdxBuilder` to `pub struct Builder`
  - Update `src/file/mod.rs` exports
  - Update all references

  **Must NOT do**:
  - 不改变 Builder 的字段或方法

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] `VhdxBuilder` renamed to `Builder`
  - [ ] All references updated
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(file): rename VhdxBuilder to Builder`

- [x] 4. Unify DiskType definition

  **What to do**:
  - DiskType is defined in both `src/common/disk_type.rs` and `src/file/mod.rs`
  - Keep the definition in `src/file/mod.rs` (as it's part of public API)
  - Remove the duplicate from `src/common/disk_type.rs`
  - Update imports in files that use DiskType from common

  **Must NOT do**:
  - 不修改 DiskType 枚举的定义

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] Only one DiskType definition remains (in file/mod.rs)
  - [ ] All imports updated to use file::DiskType
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(types): unify DiskType definition`

- [x] 5. Rename types in header/

  **What to do**:
  - In `src/header/`, rename types with Vhdx prefix:
    - `VhdxHeader` → `Header`
    - Other VhdxXxx types → Xxx
  - Update `src/header/mod.rs` exports

  **Must NOT do**:
  - 不改变类型的字段或方法

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] All Vhdx-prefixed types renamed
  - [ ] All references updated
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(header): rename Vhdx-prefixed types`

- [x] 6. Rename types in bat/

  **What to do**:
  - In `src/bat/`, rename types with Vhdx prefix
  - Update `src/bat/mod.rs` exports

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] All Vhdx-prefixed types renamed
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(bat): rename Vhdx-prefixed types`

- [x] 7. Rename types in metadata/

  **What to do**:
  - In `src/metadata/`, rename types with Vhdx prefix
  - Update `src/metadata/mod.rs` exports

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1

  **Acceptance Criteria**:
  - [ ] All Vhdx-prefixed types renamed
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `refactor(metadata): rename Vhdx-prefixed types`

### Wave 2: lib.rs and Module Visibility (3 tasks)

- [x] 8. Rewrite lib.rs with minimal exports

  **What to do**:
  - Change all `pub mod X` to `mod X` (make internal modules private)
  - NO prelude module
  - Add explicit exports: `pub use error::Error; pub use file::{File, Builder, DiskType};`
  - This is the key change that hides internal implementation

  **Must NOT do**:
  - 不要删除 mod 声明，只是去掉 pub
  - 不要导出内部模块
  - 不要创建 prelude

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T1-7)
  - **Blocked By**: Tasks 1-7
  - **Blocks**: Tasks 9, 11, 17-19

  **References**:
  - Current: `pub mod bat; pub mod block_io; ...`
  - Target: `mod bat; mod block_io; ... pub use ...`

  **Acceptance Criteria**:
  - [ ] All internal modules changed from `pub mod` to `mod`
  - [ ] Only Error, File, Builder, DiskType exported
  - [ ] No prelude module
  - [ ] `cargo check` passes (may have errors in tests)

  **QA Scenarios**:
  ```
  Scenario: lib.rs exports minimal API
    Tool: Bash
    Steps:
      1. Read src/lib.rs
      2. Check for "pub mod" vs "mod"
      3. `cargo doc --no-deps`
    Expected Result: Only Error, File, Builder, DiskType in public API, NO prelude
    Evidence: .sisyphus/evidence/task-08-lib-exports.txt
  ```

  **Commit**: YES (breaking)
  - Message: `refactor(lib)!: minimize public API exports, remove prelude`

- [x] 9. Update all internal imports

  **What to do**:
  - Update imports in all internal modules to use `crate::` paths
  - Since modules are now private, imports need to be adjusted
  - Example: `use crate::file::VhdxFile;` → `use crate::file::File;`

  **Must NOT do**:
  - 不改变模块逻辑

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T8)
  - **Blocked By**: Task 8
  - **Blocks**: Tasks 11, 17-19

  **Acceptance Criteria**:
  - [ ] All imports updated
  - [ ] No `crate::bat` etc. exposed publicly
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `fix(imports): update internal imports for new visibility`

- [x] 10. Add File::check() method

  **What to do**:
  - Add `pub fn check(&self) -> Result<CheckReport>` method to File
  - CheckReport should contain validation results:
    - headers valid
    - metadata valid
    - BAT valid
    - parent accessible (if differencing)
  - Similar logic to current CLI check command

  **Must NOT do**:
  - 不添加其他新功能

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T8-9, T11)
  - **Blocked By**: Tasks 1-7

  **Acceptance Criteria**:
  - [ ] `File::check()` method added
  - [ ] Returns Result<CheckReport>
  - [ ] Validates headers, metadata, BAT
  - [ ] `cargo check` passes

  **Commit**: YES
  - Message: `feat(file): add File::check() method for integrity validation`

### Wave 3: API Changes (2 tasks)

- [x] 11. Rename File::open parameter from read_only to write

  **What to do**:
  - Change `pub fn open<P: AsRef<Path>>(path: P, read_only: bool)` to `pub fn open<P: AsRef<Path>>(path: P, write: bool)`
  - Update internal logic: `write: true` means read-write mode, `write: false` means read-only mode
  - Update all call sites: change `File::open(path, true)` to `File::open(path, false)` (read-only) and `File::open(path, false)` to `File::open(path, true)` (read-write)
  - This is a parameter rename for better intuitiveness: `write: true` = "I want to write"

  **Must NOT do**:
  - 不改变读写逻辑，只是参数名和调用方变更

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T8-9)
  - **Blocked By**: Tasks 8-9
  - **Blocks**: Tasks 12, 17-19

  **Acceptance Criteria**:
  - [ ] Parameter renamed from `read_only` to `write`
  - [ ] All call sites updated (invert boolean values: `true`↔`false`)
  - [ ] `cargo check` passes

  **QA Scenarios**:
  ```
  Scenario: File::open(write) API works
    Tool: Bash
    Steps:
      1. `grep "pub fn open" src/file/file.rs | grep "write: bool"`
      2. `cargo build --lib`
    Expected Result: File::open has write parameter, build passes
    Evidence: .sisyphus/evidence/task-11-open-api.txt
  ```

  **Commit**: YES (breaking)
  - Message: `refactor(file)!: rename open parameter from read_only to write`

- [x] 12. Update tests for new API

  **What to do**:
  - Update all tests using `VhdxFile::open(path, read_only)` to `File::open(path, write)`
  - Invert boolean values in test calls: `true`→`false`, `false`→`true`
  - Update all tests using old type names

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T11)
  - **Blocked By**: Task 11
  - **Blocks**: Tasks 17-19

  **Acceptance Criteria**:
  - [ ] All tests updated to new API
  - [ ] `cargo test --lib` passes

  **Commit**: YES
  - Message: `fix(tests): update tests for File::open(write) parameter`

### Wave 4: CLI Refactoring (4 tasks)

- [x] 13. Create workspace structure and cli/ crate

  **What to do**:
  - Convert root to workspace: add `[workspace]` to root Cargo.toml
  - Create `cli/` directory
  - Create `cli/Cargo.toml` with dependencies:
    - `vhdx-rs` = { path = "../vhdx-rs" }
    - `clap` = "4.6"
  - Create `cli/src/main.rs` skeleton
  - Move `vhdx-rs` to subdirectory OR keep in root as member

  **Must NOT do**:
  - 不复制 CLI code yet

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: NO (sequential)
  - **Blocks**: Tasks 14-16

  **Acceptance Criteria**:
  - [ ] Workspace structure created
  - [ ] `cli/Cargo.toml` configured
  - [ ] `cargo build --workspace` passes

  **Commit**: YES
  - Message: `chore(workspace): create workspace structure with cli/ crate`

- [x] 14. Migrate CLI logic to cli/ (delete write command)

  **What to do**:
  - Extract CLI logic from `src/main.rs`
  - Migrate to `cli/src/main.rs`
  - **Delete `write` command** - keep only: info, create, read, check
  - Simplify significantly
  - Use library's new API: `File::open(path, write)`, `File::check()`, `Builder::new()`

  **Must NOT do**:
  - 不保留 write 命令
  - 不 copy-paste, redesign

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T13)
  - **Blocked By**: Task 13
  - **Blocks**: Tasks 15-16

  **Acceptance Criteria**:
  - [ ] CLI logic migrated to cli/ crate
  - [ ] Commands: info, create, read, check (no write)
  - [ ] Uses new library API
  - [ ] `cargo build --workspace` passes

  **QA Scenarios**:
  ```
  Scenario: CLI commands work
    Tool: Bash
    Steps:
      1. `cargo run --bin vhdx-tool -- --help`
      2. Check commands listed: info, create, read, check
      3. Check NO write command
    Expected Result: Help shows 4 commands, no write
    Evidence: .sisyphus/evidence/task-14-cli.txt
  ```

  **Commit**: YES
  - Message: `refactor(cli): migrate CLI to cli/ crate, remove write command`

- [x] 15. Remove CLI code and clap from library

  **What to do**:
  - Remove `src/main.rs` from library crate
  - Remove `[[bin]]` section from library Cargo.toml
  - Remove `clap` dependency from library Cargo.toml
  - Clean up any CLI-only code in library

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T14)
  - **Blocked By**: Task 14

  **Acceptance Criteria**:
  - [ ] `src/main.rs` removed from library
  - [ ] Library Cargo.toml has no clap dependency
  - [ ] Library builds as pure library

  **Commit**: YES
  - Message: `refactor(lib): remove CLI code and clap dependency`

- [x] 16. Update workspace Cargo.toml

  **What to do**:
  - Configure workspace members: `["vhdx-rs", "cli"]` (or appropriate paths)
  - Ensure resolver = "2"

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T15)
  - **Blocked By**: Task 15
  - **Blocks**: Tasks 17-19

  **Acceptance Criteria**:
  - [ ] Workspace configured correctly
  - [ ] `cargo build --workspace` passes

  **Commit**: YES
  - Message: `chore(workspace): finalize workspace configuration`

### Wave 5: Test Synchronization (3 tasks)

- [x] 17. Fix tests in tests/ directory

  **What to do**:
  - Update all integration tests in `tests/`
  - Adapt to new API paths and type names
  - Change imports from old public API to new

  **Must NOT do**:
  - 不改变 test logic, only imports and types

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T18)
  - **Blocked By**: Tasks 1-16
  - **Blocks**: Task 19

  **Acceptance Criteria**:
  - [ ] All tests in tests/ updated
  - [ ] `cargo test` passes

  **Commit**: YES
  - Message: `fix(tests): update integration tests for new API`

- [x] 18. Fix inline tests in src/

  **What to do**:
  - Update all `#[cfg(test)]` modules in src/
  - Fix imports and type names

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T17)
  - **Blocked By**: Tasks 1-16

  **Acceptance Criteria**:
  - [ ] All inline tests updated
  - [ ] `cargo test --lib` passes

  **Commit**: YES
  - Message: `fix(tests): update inline tests for new API`

- [x] 19. Run full test suite

  **What to do**:
  - Run complete test suite: `cargo test --workspace`
  - Fix any remaining failures

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on T17-18)
  - **Blocked By**: Tasks 17-18
  - **Blocks**: Tasks 20-24

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` all pass
  - [ ] No test failures

  **QA Scenarios**:
  ```
  Scenario: All tests pass
    Tool: Bash
    Steps:
      1. `cargo test --workspace`
    Expected Result: All tests pass, 0 failures
    Evidence: .sisyphus/evidence/task-19-tests.txt
  ```

  **Commit**: YES
  - Message: `fix(tests): ensure all tests pass`

### Wave 6: Documentation and Final Verification (5 tasks)

- [x] 20. Update doc comments

  **What to do**:
  - Update all doc comments referencing old type names
  - `VhdxError` → `Error`, `VhdxFile` → `File`, etc.
  - Update module path references

  **Must NOT do**:
  - 不修改非 doc 注释

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T21-24)
  - **Blocked By**: Task 19

  **Acceptance Criteria**:
  - [ ] All doc comments updated
  - [ ] No old type names in docs
  - [ ] `cargo doc` generates successfully

  **Commit**: YES
  - Message: `docs: update doc comments for renamed types`

- [x] 21. Run cargo doc and fix warnings

  **What to do**:
  - Run `cargo doc --no-deps --workspace`
  - Fix all doc warnings
  - Check for broken links

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T20, T22-24)
  - **Blocked By**: Task 19

  **Acceptance Criteria**:
  - [ ] `cargo doc` no warnings
  - [ ] No broken links

  **Commit**: YES
  - Message: `fix(docs): resolve cargo doc warnings`

- [x] 22. Run cargo clippy and fix warnings

  **What to do**:
  - Run `cargo clippy --workspace`
  - Fix all clippy warnings

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: YES

  **Acceptance Criteria**:
  - [ ] `cargo clippy` no warnings

  **Commit**: YES
  - Message: `fix(clippy): resolve all clippy warnings`

- [x] 23. Final build and test verification

  **What to do**:
  - Final verification:
    1. `cargo clean`
    2. `cargo build --workspace`
    3. `cargo test --workspace`
    4. `cargo doc --workspace`
    5. `cargo clippy --workspace`

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: NO (final verification)
  - **Blocked By**: Tasks 20-22

  **Acceptance Criteria**:
  - [ ] All commands succeed
  - [ ] No warnings, no errors

  **QA Scenarios**:
  ```
  Scenario: Full verification
    Tool: Bash
    Steps:
      1. `cargo clean`
      2. `cargo build --workspace`
      3. `cargo test --workspace`
      4. `cargo doc --workspace`
      5. `cargo clippy --workspace`
    Expected Result: All succeed, 0 warnings
    Evidence: .sisyphus/evidence/task-23-final.txt
  ```

  **Commit**: YES
  - Message: `chore(verify): final build and test verification`

- [x] 24. Git cleanup

  **What to do**:
  - Clean up git history
  - Remove old empty directories
  - Final git status check

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Parallelization**:
  - **Can Run In Parallel**: NO (final)
  - **Blocked By**: Task 23

  **Acceptance Criteria**:
  - [ ] Clean git history
  - [ ] No residual old files
  - [ ] Clean working directory

  **Commit**: NO (already committed)

---

## Final Verification Wave

> 4 review agents run in PARALLEL. ALL must APPROVE.

- [x] F1. **Plan Compliance Audit** — `oracle`

  **What to do**:
  - Verify all "Must Have" items are implemented
  - Verify all "Must NOT Have" guardrails are followed
  - Check that directory structure remains flat (no core/)
  - Check NO prelude module
  - Check block_io/ not renamed
  - Check CLI has no write command
  - Check File::check() exists
  - Check File::open(path, write) API
  - Check evidence files exist

  **Acceptance Criteria**:
  - [ ] Must Have [11/11] complete
  - [ ] Must NOT Have [7/7] followed
  - [ ] Flat structure maintained
  - [ ] VERDICT: APPROVE

- [x] F2. **Code Quality Review** — `unspecified-high`

  **What to do**:
  - Run `cargo build`, `cargo clippy`, `cargo fmt --check`
  - Check for code quality issues

  **Acceptance Criteria**:
  - [ ] Build PASS
  - [ ] Clippy PASS
  - [ ] Format PASS
  - [ ] VERDICT: APPROVE

- [x] F3. **API Surface Review** — `deep`

  **What to do**:
  - Review public API surface
  - Verify only intended types are public (Error, File, Builder, DiskType)
  - Check no internal modules exposed
  - Check NO prelude
  - Verify naming consistency (no Vhdx prefix)
  - Check File::check() and File::open(write) exist

  **Acceptance Criteria**:
  - [ ] Public API minimal and clear
  - [ ] No Vhdx prefix in public API
  - [ ] Internal modules hidden
  - [ ] No prelude
  - [ ] File::check() exists
  - [ ] File::open(path, write) API correct
  - [ ] VERDICT: APPROVE

- [x] F4. **Integration Test** — `unspecified-high`

  **What to do**:
  - Full integration test from clean state
  - Build, test, doc, clippy
  - Test CLI functionality (info, create, read, check)
  - Test File::check() and File::open(write)

  **Acceptance Criteria**:
  - [ ] Clean build succeeds
  - [ ] All tests pass
  - [ ] Documentation builds
  - [ ] CLI works (4 commands)
  - [ ] Library API works
  - [ ] VERDICT: APPROVE

---

## Commit Strategy

### Commit Pattern

**Wave 1 (Type Renames)**: 7 commits
- `refactor(error): rename VhdxError to Error`
- `refactor(file): rename VhdxFile to File`
- `refactor(file): rename VhdxBuilder to Builder`
- `refactor(types): unify DiskType definition`
- `refactor(header): rename Vhdx-prefixed types`
- `refactor(bat): rename Vhdx-prefixed types`
- `refactor(metadata): rename Vhdx-prefixed types`

**Wave 2 (Visibility & Check)**: 3 commits
- `refactor(lib)!: minimize public API exports, remove prelude`
- `fix(imports): update internal imports for new visibility`
- `feat(file): add File::check() method`

**Wave 3 (API Changes)**: 2 commits
- `refactor(file)!: rename open parameter from read_only to write`
- `fix(tests): update tests for File::open(write) parameter`

**Wave 4 (CLI)**: 4 commits
- `chore(workspace): create workspace structure with cli/ crate`
- `refactor(cli): migrate CLI to cli/ crate, remove write command`
- `refactor(lib): remove CLI code and clap dependency`
- `chore(workspace): finalize workspace configuration`

**Wave 5 (Tests)**: 3 commits
- `fix(tests): update integration tests for new API`
- `fix(tests): update inline tests for new API`
- `fix(tests): ensure all tests pass`

**Wave 6 (Docs & Final)**: 4 commits
- `docs: update doc comments for renamed types`
- `fix(docs): resolve cargo doc warnings`
- `fix(clippy): resolve all clippy warnings`
- `chore(verify): final build and test verification`

**Total**: ~23 commits

---

## Success Criteria

### Verification Commands

```bash
# 1. Build
cargo build --workspace
# Expected: success, 0 warnings

# 2. Test
cargo test --workspace
# Expected: all tests pass

# 3. Doc
cargo doc --workspace --no-deps
# Expected: no warnings, no broken links

# 4. Lint
cargo clippy --workspace -- -D warnings
# Expected: no warnings

# 5. Format
cargo fmt --check
# Expected: no formatting issues

# 6. CLI commands
cargo run --bin vhdx-tool -- info --help
cargo run --bin vhdx-tool -- create --help
cargo run --bin vhdx-tool -- read --help
cargo run --bin vhdx-tool -- check --help
# Expected: NO write command

# 7. Library API check
grep "pub fn write" vhdx-rs/src/file/file.rs
grep "pub fn check" vhdx-rs/src/file/file.rs
# Expected: Both methods exist

# 8. No prelude
grep "pub mod prelude" vhdx-rs/src/lib.rs
# Expected: NOT FOUND

# 9. No clap in library
grep "clap" vhdx-rs/Cargo.toml
# Expected: NOT FOUND
```

### Final Checklist

- [x] All "Must Have" present
  - [x] lib.rs 仅导出必要公共API（Error, File, Builder, DiskType）
  - [x] 无 prelude 模块
  - [x] 类型重命名（VhdxXxx → Xxx）
  - [x] 内部模块可见性改为非 pub
  - [x] CLI 重构为独立 workspace crate
  - [x] CLI 删除 write 命令
- [x] `File::open(path, write)` 参数名变更（原 read_only）
  - [x] 添加 `File::check()` 方法
  - [x] DiskType 重复定义统一
  - [x] 测试同步修复
  - [x] doc comments 更新
  - [x] 库移除 clap 依赖

- [x] All "Must NOT Have" absent
  - [x] 无内部实现逻辑修改
  - [x] 无功能新增/删除（除 API 变更外）
  - [x] 无 core/ 目录创建
  - [x] 无旧 Vhdx 前缀残留
  - [x] 无 block_io/ 重命名
  - [x] 无 prelude 模块
  - [x] 无 CLI write 命令

- [x] Quality Gates
  - [x] `cargo build --workspace` passes
  - [x] `cargo test --workspace` passes
  - [x] `cargo doc --workspace` passes
  - [x] `cargo clippy --workspace` passes
  - [x] CLI works (4 commands: info, create, read, check)
  - [x] File::check() works
  - [x] File::open(path, write) works

### Breaking Changes Summary

**API Changes**:
- `VhdxError` → `Error`
- `VhdxFile` → `File`
- `VhdxBuilder` → `Builder`
- `File::open(path, read_only)` → `File::open(path, write)`（参数名从 `read_only` 改为 `write`，调用时需反转布尔值）
- 新增: `File::check()` 方法

**Module Visibility**:
- All internal modules now private (`mod` instead of `pub mod`)
- Public API reduced to: Error, File, Builder, DiskType (NO prelude)

**CLI Changes**:
- CLI 分离到独立 workspace crate
- 删除 `write` 命令
- 保留: `info`, `create`, `read`, `check`

**Import Changes**:
```rust
// Before:
use vhdx_rs::{VhdxFile, VhdxBuilder, VhdxError};

// After:
use vhdx_rs::{File, Builder, Error, DiskType};

// Opening file:
// Before: VhdxFile::open(path, false)  // read_only=false means writable
// After:  File::open(path, true)       // write=true means writable

// Checking file:
// Before: (no method)
// After:  file.check()?
```

---

*Plan version: Revised - 移除 prelude，保持 block_io/，添加 File::check()，open参数read_only→write，CLI 分离到 workspace*
