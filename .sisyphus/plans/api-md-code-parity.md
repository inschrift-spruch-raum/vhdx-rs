# 让代码严格对齐 docs/plan/API.md（实现侧对齐计划）

## TL;DR
> **Summary**: 以 `docs/plan/API.md` 为唯一目标规范，**严格对齐库 API 树（vhdx-rs）**；CLI 设计段本计划不做全量实现，仅做库改动引发的最小联动修复。允许破坏性变更，不保留旧 API 兼容层。  
> **Deliverables**:
> - 代码 API 面与 `docs/plan/API.md` 一致（仅实现文档中明确项）
> - 对齐后的自动化测试与验收证据
> - 最小必要的 CLI 编译联动修复（不做 CLI 全量对齐）
> **Effort**: Large  
> **Parallel**: YES - 3 waves  
> **Critical Path**: T1 → T3 → T5 → T8 → T13 → T14 → T10

## Context
### Original Request
- 用户纠正目标：不是改 `docs/plan/API.md` 去贴代码，而是**改代码**去匹配 `docs/plan/API.md`。

### Interview Summary
- 兼容策略：允许破坏性变更，不保留旧 API 兼容层。
- CLI 范围：本轮不做 CLI 全量对齐，仅做库改动引发的最小必要修复。
- 对齐级别：严格全量对齐（含结构与签名目标）。
- 测试策略：tests-after。

### Scope Declaration (authoritative)
- **IN-SCOPE**: `docs/plan/API.md` 中“库 API（vhdx-rs root 与 section 树）”条目。
- **OUT-SCOPE**: `docs/plan/API.md` 中 CLI 设计段的全量实现（本轮仅最小联动修复以保证 workspace 可构建/可测试）。
- 上述范围用于 T10/T12/F4 的唯一判定标准。

### Metis Review (gaps addressed)
- 统一判定标准：以 `docs/plan/API.md` 明示条目为准，不以历史实现习惯替代规范。
- 风险分层：先做表面 API 对齐（导出/可见性/命名/签名），后做高风险语义断点（open/create 行为策略）。
- 强制每个断点做可执行验收，避免“接口对齐但语义漂移”。

## Work Objectives
### Core Objective
让 `vhdx-rs` 的实现代码与 `docs/plan/API.md` 的**库 API 部分**严格一致，并通过自动化验证证明一致性。

### Deliverables
- `src/` 中所有与 `API.md` 不一致的公开项完成对齐。
- `tests/` 与模块内测试更新，覆盖新增/改签名 API。
- API 面编译烟雾测试（按 `API.md` 用法导入与调用）通过。
- 新增并导出 `validation`、`LogReplayPolicy`、`ParentChainInfo`、`File::validator()`（若在 API.md 明示）。

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 全通过。
- `cargo clippy --workspace` 无新增 warning。
- `cargo fmt --check` 通过。
- 新增 API 面 smoke 测试可编译并通过：`cargo test -p vhdx-rs api_surface_smoke -- --nocapture`。

### Must Have
- 仅实现 `docs/plan/API.md` 明示 API。
- 保持注释中文/CLI help 英文的现有项目约束。
- 任务内均包含 happy + failure 的 agent 可执行 QA。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不实现 `API.md` 未出现的功能（禁止范围蔓延）。
- 不新增依赖，不修改 `misc/`、`Cargo.toml`、`vhdx-cli/Cargo.toml`、`rustfmt.toml`。
- 不做无关重构，不做 CLI 全量能力扩展。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust 原生 `cargo test`。
- QA policy: 每个任务含可执行场景（happy + failure）。
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`。

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> 先并行完成低耦合“接口表面对齐”，再收敛语义断点。

Wave 1: 导出/命名/可见性基础对齐（T1,T2,T4,T6,T7）  
Wave 2: File/IO/BAT/Log 的签名与行为对齐（T3,T5,T8,T9）  
Wave 3: 缺失核心能力补齐（T13,T14）  
Wave 4: API smoke + 测试补齐 + 最小 CLI 联动修复（T10,T11,T12）

### Dependency Matrix (full, all tasks)
- T1 blocks: T10
- T2 blocks: T10
- T3 blocked by: T2; blocks: T8,T10
- T4 blocks: T10
- T5 blocked by: T3; blocks: T10
- T6 blocks: T10
- T7 blocks: T10
- T8 blocked by: T3; blocks: T11
- T9 blocked by: T3; blocks: T11
- T10 blocked by: T1,T2,T3,T4,T5,T6,T7,T13,T14; blocks: T12
- T11 blocked by: T8,T9; blocks: T12
- T12 blocked by: T10,T11
- T13 blocked by: T3; blocks: T14,T10
- T14 blocked by: T13; blocks: T10,T11

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 5 tasks → `quick`/`unspecified-low`
- Wave 2 → 4 tasks → `unspecified-high`/`deep`
- Wave 3 → 2 tasks → `unspecified-high`/`deep`
- Wave 4 → 3 tasks → `unspecified-high`

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. Root 导出面对齐（lib.rs）

  **What to do**: 在 `src/lib.rs` 对齐 `docs/plan/API.md` 的 root 导出：补齐 `SectionsConfig`、`crc32c_with_zero_field`、常量与 GUID 命名空间可访问路径；确保 `section` 命名空间与 root 导出共存且不冲突。
  **Must NOT do**: 不改业务逻辑；不引入 API.md 未声明新能力。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件导出整理，低复杂度。
  - Skills: `[]` - 无额外技能依赖。
  - Omitted: `review-work` - 原因：此任务由最终验证波覆盖。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T10 | Blocked By: none

  **References**:
  - Pattern: `src/lib.rs` - 当前导出结构与命名空间组织
  - API/Type: `docs/plan/API.md` - root API 树
  - Test: `tests/integration_test.rs` - 外部 crate 导入使用方式

  **Acceptance Criteria**:
  - [ ] `cargo check -p vhdx-rs` 通过
  - [ ] 能在测试中从 root 导入 `SectionsConfig` 与 `crc32c_with_zero_field`

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: [Happy path - root exports compile]
    Tool: Bash
    Steps: 在临时测试中添加 root import 语句并执行 `cargo test -p vhdx-rs --tests`
    Expected: 编译通过，未出现 unresolved import
    Evidence: .sisyphus/evidence/task-1-root-exports.txt

  Scenario: [Failure/edge case - duplicate re-export conflict]
    Tool: Bash
    Steps: 执行 `cargo check -p vhdx-rs` 观察是否产生名称冲突
    Expected: 无 E0252/E0255 冲突错误
    Evidence: .sisyphus/evidence/task-1-root-exports-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align root re-exports with API plan` | Files: `src/lib.rs`

- [ ] 2. File 公共方法可见性与签名补齐

  **What to do**: 在 `src/file.rs` 为 API.md 明示项补齐/公开 `File::read`、`File::write`、`File::flush` 与相关 getter 可见性；保持底层 `*_raw` 内部路径稳定。
  **Must NOT do**: 不改变 IO 语义（仅公开包装与签名对齐），不新增文档外方法。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及核心类型公开接口与可见性。
  - Skills: `[]` - 无。
  - Omitted: `git-master` - 原因：当前不执行 git 历史操作。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T3,T10 | Blocked By: none

  **References**:
  - Pattern: `src/file.rs` - `read_raw/write_raw/flush_raw` 现有实现
  - API/Type: `docs/plan/API.md` - `File` 方法签名
  - Test: `src/file.rs` 内 `#[cfg(test)]` 与 `tests/integration_test.rs`

  **Acceptance Criteria**:
  - [ ] `File::read/write/flush` 公开签名与 API.md 一致
  - [ ] `cargo test -p vhdx-rs` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - public read/write/flush works]
    Tool: Bash
    Steps: 运行库测试并新增一例通过 public API 的读写回环
    Expected: 数据写入后读回一致，测试通过
    Evidence: .sisyphus/evidence/task-2-file-public-io.txt

  Scenario: [Failure/edge case - out-of-bounds write]
    Tool: Bash
    Steps: 添加超界写入用例并运行 `cargo test -p vhdx-rs`
    Expected: 返回 Error（非 panic），错误分支可断言
    Evidence: .sisyphus/evidence/task-2-file-public-io-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): expose File public io methods per plan` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 3. OpenOptions/CreateOptions API 对齐（仅 API.md 明示项）

  **What to do**: 对齐 `OpenOptions`、`CreateOptions` 的公开链式方法与参数契约（包含 API.md 明示的 `strict`/`log_replay` 及相关参数项）；保证 API.md 示例可编译。
  **Must NOT do**: 不加入 API.md 未声明方法。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 构造流程影响 open/create 入口行为与兼容性。
  - Skills: `[]` - 无。
  - Omitted: `playwright` - 非浏览器任务。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T5,T8,T10 | Blocked By: T2

  **References**:
  - Pattern: `src/file.rs` - builder 模式实现
  - API/Type: `docs/plan/API.md` - OpenOptions/CreateOptions 段落
  - Test: `tests/integration_test.rs` 创建/打开样例

  **Acceptance Criteria**:
  - [ ] API.md 中的 builder 链式调用可编译并通过测试
  - [ ] 文档外 builder 方法不作为公开 API 输出

  **QA Scenarios**:
  ```
  Scenario: [Happy path - documented builder chain]
    Tool: Bash
    Steps: 以 API.md 示例链式调用创建/打开文件并运行测试
    Expected: 构建成功、测试通过
    Evidence: .sisyphus/evidence/task-3-options-api.txt

  Scenario: [Failure/edge case - invalid builder parameter]
    Tool: Bash
    Steps: 构造非法 size/block_size 参数并运行单测
    Expected: 返回 InvalidParameter 类错误
    Evidence: .sisyphus/evidence/task-3-options-api-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align open/create options surface with plan` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 4. IO/Sector/PayloadBlock 可见性与命名对齐

  **What to do**: 在 `src/io_module.rs` 对齐 `IO::read_sectors/write_sectors` 可见性、`Sector::block_idx/global_sector/block_sector_idx` 与字段命名，确保 API.md 调用方式成立。
  **Must NOT do**: 不重写块映射算法；仅做接口与轻量封装对齐。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 核心 IO 接口、容易引入回归。
  - Skills: `[]` - 无。
  - Omitted: `oracle` - 本任务执行不需再次架构咨询。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T10 | Blocked By: none

  **References**:
  - Pattern: `src/io_module.rs` - 现有 Sector/PayloadBlock 定义
  - API/Type: `docs/plan/API.md` - IO/Sector/PayloadBlock 定义
  - Test: `tests/integration_test.rs` 中 `io().sector(...).read()` 用法

  **Acceptance Criteria**:
  - [ ] IO/Sector/PayloadBlock 的公开方法名与可见性匹配 API.md
  - [ ] `cargo test -p vhdx-rs` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - sector helper methods]
    Tool: Bash
    Steps: 新增测试调用 block_idx/global_sector/block_sector_idx 并运行测试
    Expected: 返回值一致且测试通过
    Evidence: .sisyphus/evidence/task-4-io-sector.txt

  Scenario: [Failure/edge case - sector out of bounds]
    Tool: Bash
    Steps: 对不存在扇区读写并运行测试
    Expected: 返回 Error::SectorOutOfBounds 或等价错误，不 panic
    Evidence: .sisyphus/evidence/task-4-io-sector-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align io-sector public surface` | Files: `src/io_module.rs`, `tests/integration_test.rs`

- [ ] 5. BAT entries 迭代器签名对齐

  **What to do**: 在 `src/sections/bat.rs` 将 `Bat::entries()` 对齐为 API.md 目标返回签名 `Vec<BatEntry>`，并同步修复调用侧；如内部需要保留迭代器，仅作为内部实现细节。
  **Must NOT do**: 不改变 BAT 解析语义与 state 计算逻辑。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 返回类型改变影响面较广。
  - Skills: `[]` - 无。
  - Omitted: `librarian` - 无外部库调研需求。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T10 | Blocked By: T3

  **References**:
  - Pattern: `src/sections/bat.rs` - `Bat::entries` 当前实现
  - API/Type: `docs/plan/API.md` - `Bat::entries(&self) -> Vec<BatEntry>` 条目
  - Test: `tests/integration_test.rs` BAT 遍历断言

  **Acceptance Criteria**:
  - [ ] `Bat::entries()` 返回签名为 `Vec<BatEntry>`
  - [ ] `Bat::entries()` 调用与现有测试调用点全部对齐

  **QA Scenarios**:
  ```
  Scenario: [Happy path - iterate BAT entries]
    Tool: Bash
    Steps: 新增 BAT 迭代测试并运行 `cargo test -p vhdx-rs`
    Expected: 迭代结果与长度断言一致
    Evidence: .sisyphus/evidence/task-5-bat-iter.txt

  Scenario: [Failure/edge case - empty BAT]
    Tool: Bash
    Steps: 构造最小镜像并验证 empty/len/迭代边界
    Expected: is_empty 为真，迭代不 panic
    Evidence: .sisyphus/evidence/task-5-bat-iter-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align BAT iterator surface with plan` | Files: `src/sections/bat.rs`, `tests/integration_test.rs`

- [ ] 6. LogEntry 命名与导出对齐

  **What to do**: 将 `Entry`（log section）对齐为 `LogEntry`（可通过重命名或 type alias），并保证导出路径符合 API.md。
  **Must NOT do**: 不改变 log descriptor/data 解析语义。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 以命名/导出收敛为主。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 不涉及复杂设计决策。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T10 | Blocked By: none

  **References**:
  - Pattern: `src/sections/log.rs`, `src/sections.rs`, `src/lib.rs`
  - API/Type: `docs/plan/API.md` - `LogEntry` 命名
  - Test: `tests/integration_test.rs` log section 访问

  **Acceptance Criteria**:
  - [ ] 从 root 或文档指定路径可导入 `LogEntry`
  - [ ] 现有 log 测试与新命名兼容

  **QA Scenarios**:
  ```
  Scenario: [Happy path - LogEntry import and use]
    Tool: Bash
    Steps: 添加导入与最小调用编译测试
    Expected: 编译通过，无命名冲突
    Evidence: .sisyphus/evidence/task-6-logentry-name.txt

  Scenario: [Failure/edge case - old symbol leakage]
    Tool: Bash
    Steps: 检查对外 API 是否仍暴露未文档化旧名
    Expected: 对外面只暴露文档目标名或明确别名策略
    Evidence: .sisyphus/evidence/task-6-logentry-name-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align log entry naming to API plan` | Files: `src/sections/log.rs`, `src/sections.rs`, `src/lib.rs`

- [ ] 7. 常量与 GUID 子命名空间路径对齐

  **What to do**: 对齐 `KiB/MiB/...` 与 `region_guids`、`metadata_guids` 的可访问路径，使其与 API.md 描述一致。
  **Must NOT do**: 不改常量值本身。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 导出路径调整。
  - Skills: `[]` - 无。
  - Omitted: `unspecified-high` - 复杂度不高。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T10 | Blocked By: none

  **References**:
  - Pattern: `src/common/constants.rs`, `src/common/mod.rs`, `src/lib.rs`
  - API/Type: `docs/plan/API.md` 常量章节
  - Test: 新增常量导入 smoke 测试

  **Acceptance Criteria**:
  - [ ] API.md 中列出的常量路径全部可导入
  - [ ] 常量值与现实现保持一致

  **QA Scenarios**:
  ```
  Scenario: [Happy path - constant paths compile]
    Tool: Bash
    Steps: 添加常量导入 smoke 测试并执行
    Expected: 编译通过，常量断言通过
    Evidence: .sisyphus/evidence/task-7-constants-paths.txt

  Scenario: [Failure/edge case - wrong module exposure]
    Tool: Bash
    Steps: 执行 `cargo check -p vhdx-rs` 并检查 unresolved path
    Expected: 无 unresolved import/path 错误
    Evidence: .sisyphus/evidence/task-7-constants-paths-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align constant namespace exports` | Files: `src/lib.rs`, `src/common/mod.rs`

- [ ] 8. Sections/子结构签名与可见性结构对齐（高风险）

  **What to do**: 对 `Sections/Header/Bat/Metadata/Log` 及相关结构体按 API.md 目标进行签名与可见性收敛（含必要生命周期/字段可见性模型），确保外部可见 API 一致。
  **Must NOT do**: 不改变已验证的底层解析逻辑输出。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 多模块联动与生命周期/借用模型风险高。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 任务复杂度高，不适合。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T11 | Blocked By: T3

  **References**:
  - Pattern: `src/sections.rs`, `src/sections/header.rs`, `src/sections/metadata.rs`, `src/sections/log.rs`
  - API/Type: `docs/plan/API.md` 各 section 类型定义
  - Test: `tests/integration_test.rs` section 访问模式

  **Acceptance Criteria**:
  - [ ] 各 section 类型的对外签名与 API.md 对齐
  - [ ] `cargo test -p vhdx-rs` 与 `cargo clippy -p vhdx-rs` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - section APIs compile and run]
    Tool: Bash
    Steps: 运行新增 section API 对齐测试集
    Expected: 通过且无借用/生命周期编译错误
    Evidence: .sisyphus/evidence/task-8-sections-signature.txt

  Scenario: [Failure/edge case - malformed section metadata]
    Tool: Bash
    Steps: 使用损坏样本触发 section 解析错误路径
    Expected: 返回对应 Error，且不会 panic
    Evidence: .sisyphus/evidence/task-8-sections-signature-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align section signatures with API plan` | Files: `src/sections.rs`, `src/sections/*.rs`

- [ ] 9. ParentLocator/KeyValueEntry 等细节签名收口

  **What to do**: 对 `ParentLocator`、`LocatorHeader`、`KeyValueEntry` 等细节方法返回签名进行 API.md 级别收口（含 `raw()` 形态差异）。
  **Must NOT do**: 不扩展 parent chain 新功能。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及二进制布局结构，需谨慎。
  - Skills: `[]` - 无。
  - Omitted: `artistry` - 无需非常规解法。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T11 | Blocked By: T3

  **References**:
  - Pattern: `src/sections/metadata.rs`
  - API/Type: `docs/plan/API.md` Parent Locator 相关章节
  - Test: `tests/integration_test.rs` metadata/parent locator 用例

  **Acceptance Criteria**:
  - [ ] metadata 相关类型签名与 API.md 保持一致
  - [ ] parent locator 现有解析测试通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - parent locator accessor behavior]
    Tool: Bash
    Steps: 运行 parent locator 相关测试
    Expected: key/value 读取与 raw 表现符合目标签名
    Evidence: .sisyphus/evidence/task-9-parentlocator-signature.txt

  Scenario: [Failure/edge case - invalid key/value offsets]
    Tool: Bash
    Steps: 构造偏移非法数据并触发读取
    Expected: 返回错误而非越界 panic
    Evidence: .sisyphus/evidence/task-9-parentlocator-signature-error.txt
  ```

  **Commit**: YES | Message: `refactor(api): align parent locator signature details` | Files: `src/sections/metadata.rs`, `tests/integration_test.rs`

- [ ] 13. 补齐 validation 模块与公开校验类型

  **What to do**: 按 API.md 明示项新增 `validation` 模块，提供 `SpecValidator`、`ValidationIssue` 的公开定义与最小可用实现，并在 `src/lib.rs` 完成导出。
  **Must NOT do**: 不扩展为 API.md 未声明的额外校验能力；不引入新依赖。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 新增公开模块且需与现有错误/section 类型协同。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 涉及接口设计与测试闭环。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: T14,T10 | Blocked By: T3

  **References**:
  - Pattern: `src/lib.rs` 导出组织
  - API/Type: `docs/plan/API.md` 中 validation 相关条目
  - Test: `tests/integration_test.rs`（新增校验 API smoke）

  **Acceptance Criteria**:
  - [ ] `SpecValidator`、`ValidationIssue` 可从 API.md 指定路径导入
  - [ ] 至少一条校验调用路径可执行并返回可断言结果

  **QA Scenarios**:
  ```
  Scenario: [Happy path - validation API import/use]
    Tool: Bash
    Steps: 新增测试导入并调用 validation API 后运行 `cargo test -p vhdx-rs`
    Expected: 编译通过，调用返回可断言值
    Evidence: .sisyphus/evidence/task-13-validation-module.txt

  Scenario: [Failure/edge case - invalid input validation]
    Tool: Bash
    Steps: 构造非法输入并执行校验
    Expected: 返回失败结果或 issue 列表，不 panic
    Evidence: .sisyphus/evidence/task-13-validation-module-error.txt
  ```

  **Commit**: YES | Message: `feat(api): add validation module per API plan` | Files: `src/validation.rs`, `src/lib.rs`, `tests/integration_test.rs`

- [ ] 14. 补齐 LogReplayPolicy / ParentChainInfo / File::validator

  **What to do**: 按 API.md 明示项在 `src/file.rs` 新增并公开 `LogReplayPolicy`、`ParentChainInfo`，并补齐 `File::validator()` 返回路径（与 T13 对接）。
  **Must NOT do**: 不改变现有日志回放核心语义（除 API.md 明示策略要求）；不引入文档外策略枚举值。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 触及 File 核心入口与策略语义。
  - Skills: `[]` - 无。
  - Omitted: `librarian` - 不依赖外部文档查询。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: T10,T11 | Blocked By: T13

  **References**:
  - Pattern: `src/file.rs` open/create 与 log 处理路径
  - API/Type: `docs/plan/API.md` 中 `LogReplayPolicy`、`ParentChainInfo`、`File::validator`
  - Test: `tests/integration_test.rs`, `src/file.rs` 内单测

  **Acceptance Criteria**:
  - [ ] 新类型与方法可从 API.md 指定路径导入与调用
  - [ ] 针对策略分支至少覆盖一条 happy + 一条 failure 测试

  **QA Scenarios**:
  ```
  Scenario: [Happy path - policy and validator wiring]
    Tool: Bash
    Steps: 运行新增 file API 测试，覆盖 LogReplayPolicy 与 File::validator
    Expected: 编译与测试通过
    Evidence: .sisyphus/evidence/task-14-file-policy-validator.txt

  Scenario: [Failure/edge case - replay required path]
    Tool: Bash
    Steps: 构造需日志回放场景并验证策略分支行为
    Expected: 返回预期错误/分支结果，行为可断言
    Evidence: .sisyphus/evidence/task-14-file-policy-validator-error.txt
  ```

  **Commit**: YES | Message: `feat(api): add replay policy parent chain info and file validator` | Files: `src/file.rs`, `src/lib.rs`, `tests/integration_test.rs`

- [ ] 10. API Surface Smoke Test（按 API.md 编译验收）

  **What to do**: 新增 API 面 smoke 测试文件，覆盖 API.md 中的导入与最小调用路径（不做重业务断言，侧重“可导入+可调用”）。
  **Must NOT do**: 不写与 API.md 无关的扩展测试。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 汇总性验收，覆盖面广。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 覆盖矩阵较大。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: T12 | Blocked By: T1,T2,T3,T4,T5,T6,T7,T13,T14

  **References**:
  - Pattern: `tests/integration_test.rs`
  - API/Type: `docs/plan/API.md` 全量 API 树
  - Test: 新文件 `tests/api_surface_smoke.rs`

  **Acceptance Criteria**:
  - [ ] `cargo test -p vhdx-rs api_surface_smoke -- --nocapture` 通过
  - [ ] smoke 测试覆盖 API.md 主入口类型

  **QA Scenarios**:
  ```
  Scenario: [Happy path - api smoke compile and pass]
    Tool: Bash
    Steps: 运行 `cargo test -p vhdx-rs api_surface_smoke -- --nocapture`
    Expected: 测试通过，0 compile error
    Evidence: .sisyphus/evidence/task-10-api-smoke.txt

  Scenario: [Failure/edge case - missing export regression]
    Tool: Bash
    Steps: 运行 `cargo check -p vhdx-rs` 并检查 smoke 相关导入
    Expected: 无 unresolved import；若有则任务失败
    Evidence: .sisyphus/evidence/task-10-api-smoke-error.txt
  ```

  **Commit**: YES | Message: `test(api): add API.md surface smoke coverage` | Files: `tests/api_surface_smoke.rs`

- [ ] 11. 回归与最小 CLI 联动修复

  **What to do**: 运行 workspace 回归；若库 API 调整导致 CLI 编译失败，仅做最小必要修复（不扩展 CLI 功能）。
  **Must NOT do**: 不新增 CLI 文档外参数/子命令。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 跨 crate 回归与收敛。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 以修复编译断点为主。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: T12 | Blocked By: T8,T9,T14

  **References**:
  - Pattern: `vhdx-cli/src/**/*.rs`
  - API/Type: `src/lib.rs` 导出面
  - Test: `vhdx-cli/tests/cli_integration.rs`

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` 通过
  - [ ] `cargo build -p vhdx-tool` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - workspace regression pass]
    Tool: Bash
    Steps: 运行 `cargo test --workspace`
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-11-workspace-regression.txt

  Scenario: [Failure/edge case - CLI compile break after API change]
    Tool: Bash
    Steps: 运行 `cargo build -p vhdx-tool` 并定位失败点后最小修复
    Expected: 最终构建通过，且未新增 CLI 功能项
    Evidence: .sisyphus/evidence/task-11-workspace-regression-error.txt
  ```

  **Commit**: YES | Message: `fix(api): apply minimal cli fallout fixes after api alignment` | Files: `vhdx-cli/src/**/*.rs` (if needed)

- [ ] 12. 最终质量闸门（fmt/clippy/test + 证据归档）

  **What to do**: 执行最终三件套验证并归档证据索引，确保计划内全部验收项有结果文件。
  **Must NOT do**: 不在此任务引入额外代码修改（除修复验证失败）。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 最终收敛与质量门控。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 需要全局检查。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: none | Blocked By: T10,T11

  **References**:
  - Pattern: `.sisyphus/evidence/`
  - API/Type: `docs/plan/API.md`
  - Test: workspace 全量命令

  **Acceptance Criteria**:
  - [ ] `cargo fmt --check` 通过
  - [ ] `cargo clippy --workspace` 通过（无新增 warning）
  - [ ] `cargo test --workspace` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - full quality gate]
    Tool: Bash
    Steps: 依次运行 fmt/clippy/test 三条命令
    Expected: 三条命令全部通过
    Evidence: .sisyphus/evidence/task-12-final-gate.txt

  Scenario: [Failure/edge case - clippy new warnings]
    Tool: Bash
    Steps: 运行 `cargo clippy --workspace` 并检查新增 warning
    Expected: 若出现新增 warning 则任务失败并回流修复
    Evidence: .sisyphus/evidence/task-12-final-gate-error.txt
  ```

  **Commit**: YES | Message: `chore(api): finalize verification evidence and quality gates` | Files: `.sisyphus/evidence/*` (and minimal fixes if required)

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle

  **What to do**: 逐条比对 T1-T14 产物与本计划要求，审计是否存在遗漏任务、跳步执行或验收伪通过。
  **Acceptance Criteria**:
  - [ ] 覆盖 T1-T14 全任务审计结论
  - [ ] 标注每条不符合项与对应修复任务

  **QA Scenarios**:
  ```
  Scenario: [Happy path - full plan audit]
    Tool: Bash
    Steps: 汇总任务证据并生成符合性检查表
    Expected: 所有任务均有证据且验收项闭合
    Evidence: .sisyphus/evidence/f1-plan-compliance.txt

  Scenario: [Failure/edge case - missing evidence]
    Tool: Bash
    Steps: 随机抽查任务编号与证据文件映射
    Expected: 如缺证据则判定失败并回流
    Evidence: .sisyphus/evidence/f1-plan-compliance-error.txt
  ```

- [ ] F2. Code Quality Review — unspecified-high

  **What to do**: 对变更文件做质量审查（接口一致性、错误处理、可维护性、无 AI slop 模式）。
  **Acceptance Criteria**:
  - [ ] 无明显代码味道与重复逻辑退化
  - [ ] 关键公共 API 文档与实现一致

  **QA Scenarios**:
  ```
  Scenario: [Happy path - quality review pass]
    Tool: Bash
    Steps: 运行 `cargo clippy --workspace` 并审阅告警
    Expected: 无新增 warning，审查意见为通过
    Evidence: .sisyphus/evidence/f2-code-quality.txt

  Scenario: [Failure/edge case - quality regression]
    Tool: Bash
    Steps: 定位新增 warning 或复杂度回退点
    Expected: 输出问题清单并回流修复
    Evidence: .sisyphus/evidence/f2-code-quality-error.txt
  ```

- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)

  **What to do**: 执行真实命令级验收，覆盖 API smoke、workspace tests、CLI 构建联动。
  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` 通过
  - [ ] `cargo build -p vhdx-tool` 通过
  - [ ] `api_surface_smoke` 通过

  **QA Scenarios**:
  ```
  Scenario: [Happy path - command suite pass]
    Tool: Bash
    Steps: 依次执行 smoke/test/build 命令
    Expected: 全部通过
    Evidence: .sisyphus/evidence/f3-manual-qa.txt

  Scenario: [Failure/edge case - command failure]
    Tool: Bash
    Steps: 捕获失败命令与错误栈
    Expected: 明确失败原因并阻断完成态
    Evidence: .sisyphus/evidence/f3-manual-qa-error.txt
  ```

- [ ] F4. Scope Fidelity Check — deep

  **What to do**: 检查最终变更是否严格限定在“代码对齐 API.md”，确认无越界功能（尤其伪缺口项）。
  **Acceptance Criteria**:
  - [ ] 无 API.md 外功能新增
  - [ ] CLI 仅最小必要联动修复

  **QA Scenarios**:
  ```
  Scenario: [Happy path - scope fidelity pass]
    Tool: Bash
    Steps: 对照 API.md 与变更清单逐项核验
    Expected: 仅包含文档明示项
    Evidence: .sisyphus/evidence/f4-scope-fidelity.txt

  Scenario: [Failure/edge case - scope creep detected]
    Tool: Bash
    Steps: 搜索并标记 API.md 未声明新增项
    Expected: 发现越界即判定失败并回滚/移除
    Evidence: .sisyphus/evidence/f4-scope-fidelity-error.txt
  ```

## Commit Strategy
- 按任务簇提交（Wave 1 / Wave 2 / Wave 3 / Wave 4），每次提交前跑对应最小回归。
- 提交消息格式：`refactor(api): ...` / `test(api): ...` / `fix(api): ...`。

## Success Criteria
- `docs/plan/API.md` 中列出的 API 在实现侧均可导入、可调用、语义符合文档。
- 无文档外扩展项混入本次变更。
- 全量自动化验证通过并留存证据。
