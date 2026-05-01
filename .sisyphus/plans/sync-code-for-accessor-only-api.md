# Sync Code for Accessor-only API

## TL;DR
> **Summary**: 将代码与测试全面同步到“结构体字段私有化 + 统一函数访问”模型，确保不遗漏任何跨模块调用点与对外 API 面。  
> **Deliverables**:
> - 访问器补齐与字段私有化改造清单全部落地
> - 所有跨模块/测试字段直访改为方法访问
> - Workspace 全量构建、测试、lint、fmt 校验通过
> **Effort**: Large  
> **Parallel**: YES - 3 waves  
> **Critical Path**: 1 → 2/3/4/5 → 6/7/8/9 → 10

## Context
### Original Request
用户要求“做出同步更新代码的计划：一个都不能露”，目标是让代码与已更新文档一致（结构体字段改为函数访问，枚举保持不变）。

### Interview Summary
- 确认仅输出执行计划，不直接实施代码修改。  
- 约束为“零遗漏”，必须覆盖定义点、跨模块调用点、测试面、验收面。  
- 默认覆盖 workspace 全范围（库 crate + CLI crate + tests）。

### Metis Review (gaps addressed)
- Guardrail 1：先补访问器、后迁移调用点、最后私有化字段，避免中途大量编译断裂。  
- Guardrail 2：对“可外部构造的公开结构体”明确策略（构造器/工厂/保留必要可见性），避免 API 断层。  
- Guardrail 3：以“文件清单 + 验收命令”双重闭环防漏改。

## Work Objectives
### Core Objective
在不改变枚举语义的前提下，将结构体字段访问全面迁移为函数访问，并完成字段私有化，保证仓库全部质量门通过。

### Deliverables
- 访问器补齐（缺失访问器全部新增）。
- 跨模块字段直访点全部迁移。
- integration/smoke 测试中的字段直访与结构体字面量构造策略统一。
- 字段私有化改造完成并通过全量校验。

### Definition of Done (verifiable conditions with commands)
- `cargo build --workspace` 退出码为 0。  
- `cargo test --workspace` 全部通过。  
- `cargo clippy --workspace` 无告警失败。  
- `cargo fmt --check` 无格式漂移。  
- 不存在跨模块 `.<field>` 访问公开结构体字段的残留。

### Must Have
- 按“缺失访问器→调用点迁移→字段私有化→全量验证”顺序执行。  
- 每个任务必须包含 happy path 与 failure/edge QA 场景。  
- 所有改动保持 Rust API 可编译，并与现有导出面一致。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `misc/`、`Cargo.toml` 依赖、`rustfmt.toml`。  
- 不引入与本次目标无关的重构（命名、目录调整、风格性批量改写）。  
- 不改变枚举变体及其语义。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + cargo test workspace  
- QA policy: 每个任务均含 agent 可执行场景。  
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
Wave 1: 访问器与私有化前置基础（任务 1-4）  
Wave 2: 调用点迁移（库/测试拆分并行，任务 5-8）  
Wave 3: 私有化落地与全量回归（任务 9-10）

### Dependency Matrix (full, all tasks)
- 1 blocks 5,6,7,8,9,10  
- 2 blocks 7,8,9,10  
- 3 blocks 9,10  
- 4 blocks 9,10  
- 5 blocks 9,10  
- 6 blocks 9,10  
- 7 blocks 9,10  
- 8 blocks 9,10  
- 9 blocks 10

### Agent Dispatch Summary
- Wave 1 → 4 tasks → quick / unspecified-high  
- Wave 2 → 4 tasks → quick / unspecified-high  
- Wave 3 → 2 tasks → unspecified-high / deep

## TODOs

- [x] 1. 建立“字段→访问器→调用点”总台账（零遗漏基线）

  **What to do**: 汇总以下文件中的公开结构体字段定义与外部访问点，产出可核对台账：`src/sections/header.rs`, `src/sections/bat.rs`, `src/sections/metadata.rs`, `src/sections/log.rs`, `src/io_module.rs`, `src/validation.rs`, `src/file.rs`, `tests/integration_test.rs`, `tests/api_surface_smoke.rs`。
  **Must NOT do**: 不直接改代码；不基于猜测补充不存在条目。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 清单完整性要求高
  - Skills: `[]` - 无额外技能依赖
  - Omitted: `[/refactor]` - 当前不是重构执行

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: [2,3,4,5,6,7,8,9,10] | Blocked By: []

  **References**:
  - Pattern: `src/validation.rs` - 典型跨模块字段直访集中处
  - Pattern: `tests/api_surface_smoke.rs` - 最大字段直访测试面
  - API/Type: `src/sections/bat.rs:BatEntry` - 缺失关键访问器

  **Acceptance Criteria**:
  - [ ] 产出台账包含“定义文件 + 访问文件 + 预计替换方式”三列
  - [ ] 台账覆盖上述所有目标文件

  **QA Scenarios**:
  ```
  Scenario: 台账完整性
    Tool: Bash
    Steps: 生成并检查台账条目总数与目标文件交叉覆盖率
    Expected: 每个目标文件至少出现一次且有替换说明
    Evidence: .sisyphus/evidence/task-1-inventory.md

  Scenario: 防漏检查
    Tool: Bash
    Steps: 对比台账中的定义列表与源码实际定义列表
    Expected: 无缺失定义项
    Evidence: .sisyphus/evidence/task-1-inventory-diff.txt
  ```

  **Commit**: NO | Message: `docs(plan): inventory accessor migration scope` | Files: [analysis only]

- [x] 2. 补齐缺失访问器（BatEntry / TableHeader / KeyValueEntry / DataSector）

  **What to do**: 新增缺失方法并保持返回语义稳定：  
  `BatEntry::state()`, `BatEntry::file_offset_mb()`；  
  `TableHeader::reserved()`, `TableHeader::reserved2()`；  
  `KeyValueEntry::{key_offset,key_length,value_offset,value_length}()`；  
  `DataSector::signature()`。
  **Must NOT do**: 不修改已有方法语义；不改枚举定义。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单模块定点新增方法
  - Skills: `[]` - 直接实现即可
  - Omitted: `[/ai-slop-remover]` - 非本任务重点

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [7,8,9,10] | Blocked By: [1]

  **References**:
  - API/Type: `src/sections/bat.rs:BatEntry`
  - API/Type: `src/sections/metadata.rs:TableHeader, KeyValueEntry`
  - API/Type: `src/sections/log.rs:DataSector`

  **Acceptance Criteria**:
  - [ ] 缺失访问器全部新增且可被外部模块调用
  - [ ] 不引入 clippy/pedantic 新告警

  **QA Scenarios**:
  ```
  Scenario: 新访问器可编译
    Tool: Bash
    Steps: cargo build --workspace
    Expected: 编译通过
    Evidence: .sisyphus/evidence/task-2-build.txt

  Scenario: 兼容性边界
    Tool: Bash
    Steps: 仅运行相关模块测试（bat/metadata/log）
    Expected: 相关测试不回退
    Evidence: .sisyphus/evidence/task-2-targeted-tests.txt
  ```

  **Commit**: YES | Message: `feat(api): add missing field accessor methods` | Files: [src/sections/bat.rs, src/sections/metadata.rs, src/sections/log.rs]

- [x] 3. 设计并落地“可外部构造结构体”迁移策略

  **What to do**: 对 `ParentChainInfo`, `ValidationIssue`, `PayloadBlock`, `BatEntry`, `KeyValueEntry` 在测试/对外场景中的构造方式做统一决策：优先新增 `new(...)` 或测试夹具构造函数，避免依赖公开字段字面量。
  **Must NOT do**: 不破坏现有公开导出路径；不引入行为变化。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及 API 稳定性决策
  - Skills: `[]`
  - Omitted: `[/refactor]` - 避免泛化改造

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: [7,8,9,10] | Blocked By: [1]

  **References**:
  - Pattern: `tests/api_surface_smoke.rs` - 结构体字面量构造集中处
  - API/Type: `src/file.rs:ParentChainInfo`
  - API/Type: `src/validation.rs:ValidationIssue`

  **Acceptance Criteria**:
  - [ ] 每个需外部构造的结构体都有明确替代构造路径
  - [ ] smoke 测试可迁移到新构造路径

  **QA Scenarios**:
  ```
  Scenario: 构造路径可用
    Tool: Bash
    Steps: cargo test --test api_surface_smoke
    Expected: 构造相关用例通过
    Evidence: .sisyphus/evidence/task-3-smoke.txt

  Scenario: API 导出不回退
    Tool: Bash
    Steps: 运行导出面 smoke 检查
    Expected: 关键类型仍可导入与实例化（按新路径）
    Evidence: .sisyphus/evidence/task-3-api-surface.txt
  ```

  **Commit**: YES | Message: `refactor(api): add constructor paths for accessor migration` | Files: [src/file.rs, src/validation.rs, src/io_module.rs, src/sections/*, tests/api_surface_smoke.rs]

- [x] 4. 将字段改造范围限制写入执行 guardrail

  **What to do**: 在执行说明中固定“只改访问模式，不改业务逻辑，不改枚举语义，不改磁盘格式计算流程”。
  **Must NOT do**: 不新增额外优化项。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 文本约束补强
  - Skills: `[]`
  - Omitted: `[/remove-ai-slops]` - 非必要

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [9,10] | Blocked By: [1]

  **References**:
  - Pattern: `src/file.rs`, `src/sections/*` - 核心路径

  **Acceptance Criteria**:
  - [ ] guardrail 在执行说明中可追溯

  **QA Scenarios**:
  ```
  Scenario: 范围约束存在
    Tool: Bash
    Steps: 审核执行说明中的范围约束条目
    Expected: 约束条目齐全
    Evidence: .sisyphus/evidence/task-4-guardrail.md

  Scenario: 范围外改动检测
    Tool: Bash
    Steps: 对改动文件列表做白名单比对
    Expected: 无范围外文件
    Evidence: .sisyphus/evidence/task-4-file-whitelist.txt
  ```

  **Commit**: NO | Message: `chore(plan): enforce migration guardrails` | Files: [execution notes]

- [x] 5. 迁移 `src/validation.rs` 全部跨模块字段直访

  **What to do**: 将所有 `entry.state` / `entry.file_offset_mb` / `table_header.reserved*` / `sector.signature` / `entry.key_*` 与 `value_*` 访问改为对应方法调用。
  **Must NOT do**: 不改变校验逻辑与错误语义。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件集中替换
  - Skills: `[]`
  - Omitted: `[/refactor]` - 避免结构化重写

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [9,10] | Blocked By: [1,2]

  **References**:
  - Pattern: `src/validation.rs`
  - API/Type: `src/sections/bat.rs:BatEntry::state/file_offset_mb`

  **Acceptance Criteria**:
  - [ ] `src/validation.rs` 无跨模块字段直访残留
  - [ ] 对应校验单元/集成测试通过

  **QA Scenarios**:
  ```
  Scenario: 替换生效
    Tool: Bash
    Steps: cargo test -p vhdx-rs
    Expected: vhdx-rs 测试通过
    Evidence: .sisyphus/evidence/task-5-lib-tests.txt

  Scenario: 回归错误语义
    Tool: Bash
    Steps: 运行包含 invalid metadata/bat/log 的测试过滤集
    Expected: 失败路径错误类型不变
    Evidence: .sisyphus/evidence/task-5-error-paths.txt
  ```

  **Commit**: YES | Message: `refactor(validation): replace direct field access with accessors` | Files: [src/validation.rs]

- [x] 6. 迁移 `src/file.rs` 中 BatEntry 直访

  **What to do**: 将 `be.state` 改为 `be.state()` 等方法访问。
  **Must NOT do**: 不改读写流程、BAT 逻辑或错误映射。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单点替换
  - Skills: `[]`
  - Omitted: `[/ai-slop-remover]` - 无必要

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [9,10] | Blocked By: [1,2]

  **References**:
  - Pattern: `src/file.rs` around BatEntry usage

  **Acceptance Criteria**:
  - [ ] `src/file.rs` 不再跨模块直访 BatEntry 字段

  **QA Scenarios**:
  ```
  Scenario: 编译回归
    Tool: Bash
    Steps: cargo build -p vhdx-rs
    Expected: 编译通过
    Evidence: .sisyphus/evidence/task-6-build-lib.txt

  Scenario: IO 路径无回退
    Tool: Bash
    Steps: 运行与文件读写相关集成测试过滤集
    Expected: 读写行为测试通过
    Evidence: .sisyphus/evidence/task-6-io-tests.txt
  ```

  **Commit**: YES | Message: `refactor(file): use BatEntry accessor in file path` | Files: [src/file.rs]

- [x] 7. 迁移 `tests/integration_test.rs` 字段直访

  **What to do**: 将 `entry.state` 等直访改为访问器调用，确保断言语义不变。
  **Must NOT do**: 不降低断言强度；不删测试。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 小范围测试适配
  - Skills: `[]`
  - Omitted: `[/refactor]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [9,10] | Blocked By: [1,2,3]

  **References**:
  - Test: `tests/integration_test.rs`

  **Acceptance Criteria**:
  - [ ] integration_test 全部通过

  **QA Scenarios**:
  ```
  Scenario: 集成测试回归
    Tool: Bash
    Steps: cargo test --test integration_test
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-7-integration.txt

  Scenario: BAT 相关断言有效
    Tool: Bash
    Steps: 运行 BAT 相关过滤测试
    Expected: 断言命中且通过
    Evidence: .sisyphus/evidence/task-7-bat-tests.txt
  ```

  **Commit**: YES | Message: `test(integration): switch field assertions to accessors` | Files: [tests/integration_test.rs]

- [x] 8. 迁移 `tests/api_surface_smoke.rs` 全量字段直访与字面量构造

  **What to do**: 批量将字段读取改为访问器；将依赖公开字段构造的用例改为新构造路径（task 3 决策）。
  **Must NOT do**: 不删除 smoke 覆盖项；不放宽导出面检查。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 改动面广且易漏
  - Skills: `[]`
  - Omitted: `[/remove-ai-slops]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [9,10] | Blocked By: [1,2,3]

  **References**:
  - Test: `tests/api_surface_smoke.rs`
  - API/Type: `src/lib.rs` re-export surface

  **Acceptance Criteria**:
  - [ ] smoke 文件不存在字段直访残留
  - [ ] api_surface_smoke 全通过

  **QA Scenarios**:
  ```
  Scenario: smoke 通过
    Tool: Bash
    Steps: cargo test --test api_surface_smoke
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-8-smoke.txt

  Scenario: 导出面未缩水
    Tool: Bash
    Steps: 运行类型导入/最小可用性用例
    Expected: 关键类型仍可导入并使用
    Evidence: .sisyphus/evidence/task-8-surface.txt
  ```

  **Commit**: YES | Message: `test(api): migrate smoke checks to accessor model` | Files: [tests/api_surface_smoke.rs]

- [x] 9. 私有化公开字段并修复编译断点

  **What to do**: 将目标结构体 `pub field` 改为私有字段，仅保留函数访问模型；必要时补充最小构造入口。
  **Must NOT do**: 不改变对外导出类型名与枚举项。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 影响面全局
  - Skills: `[]`
  - Omitted: `[/refactor]`

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [10] | Blocked By: [2,3,4,5,6,7,8]

  **References**:
  - Pattern: `src/sections/header.rs`
  - Pattern: `src/sections/bat.rs`
  - Pattern: `src/sections/metadata.rs`
  - Pattern: `src/sections/log.rs`
  - Pattern: `src/io_module.rs`
  - Pattern: `src/file.rs`
  - Pattern: `src/validation.rs`

  **Acceptance Criteria**:
  - [ ] 目标结构体字段全部私有化完成
  - [ ] 全仓无依赖公开字段的调用残留

  **QA Scenarios**:
  ```
  Scenario: 全量编译
    Tool: Bash
    Steps: cargo build --workspace
    Expected: 编译通过
    Evidence: .sisyphus/evidence/task-9-build-workspace.txt

  Scenario: 私有化遗漏检测
    Tool: Bash
    Steps: 搜索目标结构体 pub 字段残留
    Expected: 无残留
    Evidence: .sisyphus/evidence/task-9-private-audit.txt
  ```

  **Commit**: YES | Message: `refactor(api): privatize struct fields after accessor migration` | Files: [src/**/*]

- [x] 10. 执行最终质量门与证据归档

  **What to do**: 依次执行 build/test/clippy/fmt 四道门，收集结果证据并给出结论。
  **Must NOT do**: 不跳过任何一道门。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 最终验收与结论汇总
  - Skills: `[]`
  - Omitted: `[/review-work]` - 此处按本计划内最终波次执行

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [] | Blocked By: [9]

  **References**:
  - Test: `tests/integration_test.rs`
  - Test: `tests/api_surface_smoke.rs`
  - Test: `vhdx-cli/tests/cli_integration.rs`

  **Acceptance Criteria**:
  - [ ] `cargo build --workspace` 通过
  - [ ] `cargo test --workspace` 通过
  - [ ] `cargo clippy --workspace` 通过
  - [ ] `cargo fmt --check` 通过

  **QA Scenarios**:
  ```
  Scenario: 完整质量门
    Tool: Bash
    Steps: 顺序执行四道门命令
    Expected: 全部 exit code 0
    Evidence: .sisyphus/evidence/task-10-quality-gates.txt

  Scenario: 失败回放
    Tool: Bash
    Steps: 若任一失败，记录失败命令与错误摘要并重跑对应子集
    Expected: 有明确失败定位与修复验证记录
    Evidence: .sisyphus/evidence/task-10-failure-replay.txt
  ```

  **Commit**: NO | Message: `chore(qa): run full workspace gates` | Files: [evidence only]

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- 原子提交，按任务 2/3/5/6/7/8/9 分批提交。  
- 每次提交前确保对应最小测试集通过；任务 10 前不做 squash。

## Success Criteria
- 字段访问模型统一为函数访问；枚举无语义变化。  
- 没有遗漏迁移点（定义、跨模块、测试、导出面四层均闭环）。  
- Workspace 全量质量门全部通过。
