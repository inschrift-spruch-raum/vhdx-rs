# Full Alignment Plan: Implementation vs docs/plan/API.md & docs/Standard

## TL;DR
> **Summary**: Align repository behavior and API contracts to planning documents with plan-as-source-of-truth, prioritizing data correctness and strict-mode semantics, then complete signature/document parity and verification.
> **Deliverables**:
> - strict 模式语义对齐实现
> - BAT chunk-ratio 真实参数化实现
> - API 形态对齐（代码或计划文档）
> - 覆盖新增测试 + 回归验证证据
> **Effort**: Medium
> **Parallel**: YES - 4 waves
> **Critical Path**: 2 (BAT correctness) → 3 (strict semantics) → 6 (API shape parity) → F-wave

## Context
### Original Request
对照 `docs/plan/API.md` 与 `docs/Standard`，检查实现差异与 UB；随后要求“编写任务”，并选择“全量对齐计划 + 生成执行计划”。

### Interview Summary
- 用户明确采用计划为准（plan-as-source-of-truth）。
- 需要输出可执行任务，不做即时实现。
- 已完成三路并行审计与复核（explore/deep/oracle）。

### Metis Review (gaps addressed)
- 高风险差异：`Bat::new` 使用硬编码默认参数计算 chunk ratio。
- 功能差异：`strict` 参数被忽略，`strict=false` 未体现 optional unknown 放宽语义。
- 计划/实现形态偏差：`SpecValidator` 生命周期与 `HeaderStructure::create` 返回类型描述。
- 额外 guardrail：避免把文档差异误当代码错误；优先处理数据正确性。

## Work Objectives
### Core Objective
将实现与计划文档对齐到“可验证一致”，并保留现有稳定行为，补足回归测试与证据链。

### Deliverables
- D1: `strict` 行为符合 Standard §3.1。
- D2: BAT 解析基于真实 `logical_sector_size + block_size`，不再硬编码默认值。
- D3: API 形态差异完成对齐（优先不破坏正确代码：必要时更新计划文档）。
- D4: 通过完整测试与质量检查，产出证据文件。

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 全部通过。
- `cargo clippy --workspace` 无新增问题。
- `cargo fmt --check` 通过。
- 新增/更新的针对性测试通过（strict + BAT 非默认参数场景）。

### Must Have
- 修复 `src/sections/bat.rs` 默认参数 chunk-ratio 缺陷。
- 修复 `src/file.rs` 中 `strict` 参数无效问题。
- 形成清晰决策：文档改代码 / 代码改文档，并可追溯。
- 每个任务均附带可执行 QA 场景（happy + failure/edge）。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `misc/`。
- 不新增依赖。
- 不引入无关重构（命名清洗、风格化大改、接口重命名）。
- 不以“跳过测试”方式完成交付。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust (`cargo test`, `clippy`, `fmt --check`)
- QA policy: Every task includes agent-executed scenarios
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Extract shared dependencies as Wave-1 tasks for max parallelism.

Wave 1: baseline & dependency mapping (analysis-only coding prep)
- Task 1, 4, 5, 8, 9

Wave 2: correctness fixes
- Task 2, 3, 7, 10, 11

Wave 3: API parity and test expansion
- Task 6, 12, 13, 14, 15

Wave 4: hardening and release prep
- Task 16, 17, 18, 19, 20

### Dependency Matrix (full, all tasks)
- 1 blocks 2,3,6,12
- 2 blocks 7,10,11,13
- 3 blocks 10,12,14
- 6 blocks 15
- 10 blocks 16
- 11 blocks 16
- 12,13,14,15 block 17
- 17 blocks 18,19
- 18,19 block 20

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 5 tasks → quick / deep / unspecified-low
- Wave 2 → 5 tasks → quick / unspecified-high
- Wave 3 → 5 tasks → quick / writing / unspecified-high
- Wave 4 → 5 tasks → unspecified-high / deep

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 建立差异基线与触点索引

  **What to do**: 列出全部受影响符号与调用链（`strict` 流程、`Bat::new` 调用点、`SectionsConfig` 参数流、API 公开重导出），形成 executor 用索引表。
  **Must NOT do**: 不修改实现逻辑；不改对外接口。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单次聚焦梳理任务。
  - Skills: `[]` - 仅需仓库检索与注释化总结。
  - Omitted: `review-work` - 尚未进入实现阶段。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [2,3,6,12] | Blocked By: []

  **References**:
  - Pattern: `src/file.rs` - open/options/strict/log policy path
  - Pattern: `src/sections/bat.rs` - chunk ratio calc path
  - Pattern: `src/sections.rs` - bat lazy load + config flow
  - API/Type: `docs/plan/API.md` - contract baseline
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md` - strict/log semantics

  **Acceptance Criteria**:
  - [ ] 索引文档化并可用于后续任务逐项定位。

  **QA Scenarios**:
  ```
  Scenario: Baseline mapping ready
    Tool: Bash
    Steps: 运行符号检索与调用链确认命令，输出包含 strict/BAT/API 触点的列表
    Expected: 输出中包含 file.rs, sections/bat.rs, sections.rs, validation.rs, lib.rs
    Evidence: .sisyphus/evidence/task-1-baseline-map.txt

  Scenario: Missing critical touchpoint
    Tool: Bash
    Steps: 检查索引是否遗漏 Bat::new 调用点
    Expected: 若遗漏则任务失败并补全后重跑
    Evidence: .sisyphus/evidence/task-1-baseline-map-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 2. 修复 BAT chunk ratio 默认值硬编码

  **What to do**: 让 `Bat::new` 接收真实 `logical_sector_size` 与 `block_size`；移除默认常量硬编码计算；确保 payload/bitmap 条目分类逻辑使用真实参数。
  **Must NOT do**: 不改 BAT 状态语义；不改变 BAT on-disk 格式。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 影响解析正确性与调用链。
  - Skills: `[]` - 需谨慎改动并回归验证。
  - Omitted: `writing` - 主要是实现+测试。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [7,10,11,13] | Blocked By: [1]

  **References**:
  - Pattern: `src/sections/bat.rs` - `Bat::new` / `calculate_chunk_ratio`
  - Pattern: `src/sections.rs` - `Sections::bat` / `bat_mut`
  - Pattern: `src/file.rs` - `SectionsConfig` 构造
  - API/Type: `src/sections.rs:SectionsConfig` - 参数扩展点

  **Acceptance Criteria**:
  - [ ] 非默认扇区/块参数下 BAT 条目分类正确。
  - [ ] 默认参数路径行为不回归。

  **QA Scenarios**:
  ```
  Scenario: Non-default BAT classification
    Tool: Bash
    Steps: 运行新增单元/集成测试，使用 logical_sector_size=4096, block_size=1MiB
    Expected: sector bitmap 索引与 chunk_ratio 计算一致，测试通过
    Evidence: .sisyphus/evidence/task-2-bat-chunk-ratio.txt

  Scenario: Regression on default settings
    Tool: Bash
    Steps: 运行现有 BAT 相关测试集
    Expected: 默认 512/32MiB 路径测试全部通过
    Evidence: .sisyphus/evidence/task-2-bat-chunk-ratio-error.txt
  ```

  **Commit**: YES | Message: `fix(bat): compute chunk ratio from actual metadata parameters` | Files: [src/sections/bat.rs, src/sections.rs, src/file.rs, tests/**]

- [ ] 3. 使 strict=false 语义生效（仅放宽 optional unknown）

  **What to do**: 在 open/metadata 读取路径实现 strict 分支：required unknown 始终失败；optional unknown 在 strict=false 下可忽略。
  **Must NOT do**: 不放宽 required unknown；不改变 strict=true 现有行为。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 语义敏感，需与标准逐条对齐。
  - Skills: `[]` - 侧重正确性。
  - Omitted: `artistry` - 不需要非常规方法。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [10,12,14] | Blocked By: [1]

  **References**:
  - Pattern: `src/file.rs` - `open_file_with_options` / `read_metadata`
  - Pattern: `src/validation.rs` - required/known 校验模式
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md §3.1`

  **Acceptance Criteria**:
  - [ ] strict=true：required unknown 失败。
  - [ ] strict=false：optional unknown 允许；required unknown 仍失败。

  **QA Scenarios**:
  ```
  Scenario: strict=false allows optional unknown
    Tool: Bash
    Steps: 运行新增测试构造 optional unknown entry 后以 strict(false) 打开
    Expected: open 成功
    Evidence: .sisyphus/evidence/task-3-strict-optional.txt

  Scenario: strict=false still rejects required unknown
    Tool: Bash
    Steps: 运行新增测试构造 required unknown entry 后以 strict(false) 打开
    Expected: 返回 InvalidRegionTable/InvalidMetadata 错误
    Evidence: .sisyphus/evidence/task-3-strict-required-error.txt
  ```

  **Commit**: YES | Message: `fix(open): implement strict mode optional-unknown relaxation` | Files: [src/file.rs, tests/**]

- [ ] 4. 固化日志策略边界回归（Require/Auto/InMemory/ReadOnlyNoReplay）

  **What to do**: 为四种策略补充回归测试，确认只读/可写组合下行为与标准一致。
  **Must NOT do**: 不引入新策略；不改变既有错误类型契约。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 以测试为主，逻辑变更少。
  - Skills: `[]` - 无额外技能。
  - Omitted: `deep` - 非复杂设计问题。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [17] | Blocked By: []

  **References**:
  - Pattern: `src/file.rs` - `handle_log_replay`
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md §3.2, §5.2`

  **Acceptance Criteria**:
  - [ ] 四策略行为可复现并通过自动测试。

  **QA Scenarios**:
  ```
  Scenario: Writable + InMemoryOnReadOnly
    Tool: Bash
    Steps: 测试以 write() + InMemoryOnReadOnly 打开含 pending log 文件
    Expected: InvalidParameter 错误
    Evidence: .sisyphus/evidence/task-4-policy-writable-error.txt

  Scenario: ReadOnlyNoReplay structural access
    Tool: Bash
    Steps: 只读 + ReadOnlyNoReplay 打开后访问 header/metadata
    Expected: 结构读取成功
    Evidence: .sisyphus/evidence/task-4-policy-readonly-structure.txt
  ```

  **Commit**: YES | Message: `test(open): add replay-policy behavior matrix coverage` | Files: [tests/**, src/**(if minor hooks needed)]

- [ ] 5. 锁定 UB 结论的可重复检查

  **What to do**: 加入/更新审计测试或断言，说明唯一 unsafe 使用的前置边界条件，防止后续改动破坏安全假设。
  **Must NOT do**: 不新增 unsafe；不放宽边界检查。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 局部可证明性增强。
  - Skills: `[]` - 无需外部依赖。
  - Omitted: `unspecified-high` - 非大改。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [16] | Blocked By: []

  **References**:
  - Pattern: `src/sections/log.rs` - `unsafe from_raw_parts` in `entry()`

  **Acceptance Criteria**:
  - [ ] 有自动化检查覆盖 unsafe 入口的边界条件。

  **QA Scenarios**:
  ```
  Scenario: boundary-safe log entry access
    Tool: Bash
    Steps: 运行针对 log entry 偏移边界的测试组
    Expected: 无 panic, 无 UB 症状，测试通过
    Evidence: .sisyphus/evidence/task-5-unsafe-boundary.txt

  Scenario: malformed short entry
    Tool: Bash
    Steps: 构造 entry_length 小于头部的日志条目并访问
    Expected: 返回 None/错误路径，不触发 panic
    Evidence: .sisyphus/evidence/task-5-unsafe-boundary-error.txt
  ```

  **Commit**: YES | Message: `test(log): enforce unsafe-entry boundary invariants` | Files: [src/sections/log.rs, tests/**]

- [ ] 6. API 形态差异决策并执行（代码改 or 文档改）

  **What to do**: 对 `SpecValidator<'a>` 与 `HeaderStructure::create` 形态偏差执行最终对齐：优先保证代码正确性与稳定 API，再同步 `docs/plan/API.md` 描述。
  **Must NOT do**: 不破坏现有对外可用路径；不引入不必要 breaking change。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档契约对齐为主，可能少量代码适配。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 需要审慎表达与契约检查。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [15] | Blocked By: [1]

  **References**:
  - Pattern: `src/validation.rs` - `SpecValidator<'a>`
  - Pattern: `src/sections/header.rs` - `HeaderStructure::create`
  - API/Type: `docs/plan/API.md` corresponding sections

  **Acceptance Criteria**:
  - [ ] 计划文档与公开 API 签名语义一致。

  **QA Scenarios**:
  ```
  Scenario: public API compile contract
    Tool: Bash
    Steps: 运行文档示例/编译检查（如 doctest 或示例片段编译）
    Expected: API 用法与文档一致可编译
    Evidence: .sisyphus/evidence/task-6-api-shape.txt

  Scenario: accidental breaking change
    Tool: Bash
    Steps: 运行工作区测试与编译
    Expected: 无新增公开 API breakage
    Evidence: .sisyphus/evidence/task-6-api-shape-error.txt
  ```

  **Commit**: YES | Message: `docs(api): align API plan contracts with implementation shape` | Files: [docs/plan/API.md, src/**(if required)]

- [ ] 7. 传播 Bat::new 新签名到 SectionsConfig/Sections

  **What to do**: 扩展 `SectionsConfig` 与 `Sections` 字段，把真实参数传入 BAT 懒加载构造路径。
  **Must NOT do**: 不破坏其它 section lazy load 语义。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 有限影响面的结构传播。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 无需额外研究。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [16] | Blocked By: [2]

  **References**:
  - Pattern: `src/sections.rs` - `SectionsConfig`, `bat`, `bat_mut`
  - Pattern: `src/file.rs` - `Sections::new(SectionsConfig { ... })`

  **Acceptance Criteria**:
  - [ ] 所有 `Bat::new` 调用点参数完整且一致。

  **QA Scenarios**:
  ```
  Scenario: all callsites updated
    Tool: Bash
    Steps: 运行编译并搜索 Bat::new 调用签名
    Expected: 无旧签名调用残留
    Evidence: .sisyphus/evidence/task-7-bat-signature.txt

  Scenario: sections lazy load regression
    Tool: Bash
    Steps: 运行 sections 相关测试
    Expected: header/bat/metadata/log 懒加载测试通过
    Evidence: .sisyphus/evidence/task-7-bat-signature-error.txt
  ```

  **Commit**: YES | Message: `refactor(sections): propagate real sector and block size into BAT init` | Files: [src/sections.rs, src/file.rs, src/sections/bat.rs]

- [ ] 8. 对齐 Error 语义映射与计划描述

  **What to do**: 检查计划中的 Error 枚举与实现超集关系，补充计划文档说明“实现超集但兼容”。
  **Must NOT do**: 不删除现有错误变体。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档契约表达。
  - Skills: `[]` - 无。
  - Omitted: `unspecified-high` - 不涉及复杂实现。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [17] | Blocked By: []

  **References**:
  - Pattern: `src/error.rs`
  - API/Type: `docs/plan/API.md` Error section

  **Acceptance Criteria**:
  - [ ] 文档明确“计划变体 + 实现扩展变体”关系。

  **QA Scenarios**:
  ```
  Scenario: error mapping documented
    Tool: Bash
    Steps: 检查文档中 Error 映射表
    Expected: 包含计划项与扩展项说明
    Evidence: .sisyphus/evidence/task-8-error-map.txt

  Scenario: removed error variants by mistake
    Tool: Bash
    Steps: 编译并运行错误相关测试
    Expected: 无 API/测试回归
    Evidence: .sisyphus/evidence/task-8-error-map-error.txt
  ```

  **Commit**: YES | Message: `docs(error): document planned vs implemented error variants` | Files: [docs/plan/API.md]

- [ ] 9. 建立对照测试矩阵（计划条目→测试）

  **What to do**: 新建/更新测试矩阵注释，保证关键计划条目有至少一条自动测试覆盖。
  **Must NOT do**: 不把手工验证写成验收条件。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 组织化工作。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 不需要重推理。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [17] | Blocked By: []

  **References**:
  - Pattern: `tests/**`
  - API/Type: `docs/plan/API.md`

  **Acceptance Criteria**:
  - [ ] 关键差异项均映射到测试 ID。

  **QA Scenarios**:
  ```
  Scenario: matrix completeness
    Tool: Bash
    Steps: 运行脚本检查 strict/BAT/API-shape 均有测试映射
    Expected: 覆盖率检查通过
    Evidence: .sisyphus/evidence/task-9-test-matrix.txt

  Scenario: missing mapping
    Tool: Bash
    Steps: 刻意验证差异项是否可被检测为未映射
    Expected: 检查器报错并定位缺项
    Evidence: .sisyphus/evidence/task-9-test-matrix-error.txt
  ```

  **Commit**: YES | Message: `test(meta): map plan contracts to automated coverage` | Files: [tests/**, docs/plan/API.md(optional section)]

- [ ] 10. 补齐 strict 模式测试（三分法）

  **What to do**: 增加 strict=true / strict=false(optional unknown) / strict=false(required unknown) 三类场景测试。
  **Must NOT do**: 不依赖外部人工构造文件。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 覆盖关键行为矩阵。
  - Skills: `[]` - 无。
  - Omitted: `writing` - 以测试实现为主。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [16] | Blocked By: [2,3]

  **References**:
  - Pattern: `tests/integration_test.rs`
  - Pattern: `src/file.rs` strict logic
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md §3.1`

  **Acceptance Criteria**:
  - [ ] 三类 strict 场景全部通过。

  **QA Scenarios**:
  ```
  Scenario: strict matrix passes
    Tool: Bash
    Steps: 运行 strict 相关测试过滤集
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-10-strict-matrix.txt

  Scenario: regression in strict=true
    Tool: Bash
    Steps: 在 strict=true 下构造 required unknown
    Expected: 必然失败且错误类型正确
    Evidence: .sisyphus/evidence/task-10-strict-matrix-error.txt
  ```

  **Commit**: YES | Message: `test(open): add strict behavior matrix coverage` | Files: [tests/**]

- [ ] 11. 补齐 BAT 非默认参数回归测试

  **What to do**: 增加 `logical_sector_size=4096` 与不同 block size 的 BAT 分类/读路径测试。
  **Must NOT do**: 不仅验证 happy path，必须验证旧错误路径被拦截。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 数据正确性核心测试。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 范围较大。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [16] | Blocked By: [2]

  **References**:
  - Pattern: `src/sections/bat.rs`
  - Pattern: `src/file.rs` dynamic read path

  **Acceptance Criteria**:
  - [ ] 非默认参数下 BAT 分类与读取一致正确。

  **QA Scenarios**:
  ```
  Scenario: 4096-sector BAT pass
    Tool: Bash
    Steps: 运行新增 BAT 非默认参数测试
    Expected: chunk_ratio 与 bitmap 索引断言通过
    Evidence: .sisyphus/evidence/task-11-bat-nondefault.txt

  Scenario: old hardcoded behavior detection
    Tool: Bash
    Steps: 验证旧逻辑在该测试中会失败（回归防护）
    Expected: 防回归断言有效
    Evidence: .sisyphus/evidence/task-11-bat-nondefault-error.txt
  ```

  **Commit**: YES | Message: `test(bat): cover non-default sector and block configurations` | Files: [tests/**]

- [ ] 12. 校准 validator/plan 契约表述

  **What to do**: 同步 `SpecValidator` 生命周期与模块导出描述，避免 API 文档歧义。
  **Must NOT do**: 不弱化已有校验职责。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 契约表达精确化。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 不涉及新算法。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [17] | Blocked By: [3]

  **References**:
  - Pattern: `src/validation.rs`
  - Pattern: `src/lib.rs`
  - API/Type: `docs/plan/API.md`

  **Acceptance Criteria**:
  - [ ] 文档与实现签名一致、无二义性。

  **QA Scenarios**:
  ```
  Scenario: doc-signature parity
    Tool: Bash
    Steps: 运行文档校验/示例编译
    Expected: 文档签名可用且一致
    Evidence: .sisyphus/evidence/task-12-validator-parity.txt

  Scenario: stale docs mismatch
    Tool: Bash
    Steps: 自动比对导出符号与文档声明
    Expected: 无 mismatch
    Evidence: .sisyphus/evidence/task-12-validator-parity-error.txt
  ```

  **Commit**: YES | Message: `docs(validation): align validator signature and exports` | Files: [docs/plan/API.md]

- [ ] 13. 确认 create/open 内部策略一致性（含 create 后 reopen）

  **What to do**: 审核并修正 `open_file` 内部默认策略使用，避免与计划语义冲突（必要时显式注释说明）。
  **Must NOT do**: 不改变外部 `OpenOptions` 默认策略（Require）。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 局部一致性修复。
  - Skills: `[]` - 无。
  - Omitted: `unspecified-high` - 变更范围有限。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [17] | Blocked By: [2]

  **References**:
  - Pattern: `src/file.rs` - `open_file`, `open_file_with_options`, `create_file`

  **Acceptance Criteria**:
  - [ ] 内部策略行为可解释且有测试覆盖。

  **QA Scenarios**:
  ```
  Scenario: create->reopen policy consistency
    Tool: Bash
    Steps: 创建文件后触发 reopen 流程并验证策略路径
    Expected: 行为与注释/标准一致
    Evidence: .sisyphus/evidence/task-13-policy-consistency.txt

  Scenario: accidental policy drift
    Tool: Bash
    Steps: 回归测试策略矩阵
    Expected: 无漂移
    Evidence: .sisyphus/evidence/task-13-policy-consistency-error.txt
  ```

  **Commit**: YES | Message: `fix(file): make internal open policy semantics explicit and consistent` | Files: [src/file.rs, tests/**]

- [ ] 14. 对齐 HeaderStructure::create 文档/语义说明

  **What to do**: 统一计划文档中 create 的返回语义说明（序列化字节 vs 结构视图）。
  **Must NOT do**: 不改动稳定写入行为。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档语义校准。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 需要精确表述。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [17] | Blocked By: [3]

  **References**:
  - Pattern: `src/sections/header.rs` - `HeaderStructure::create`
  - API/Type: `docs/plan/API.md` Header section

  **Acceptance Criteria**:
  - [ ] 文档准确反映 create 返回值用途。

  **QA Scenarios**:
  ```
  Scenario: create semantics documented
    Tool: Bash
    Steps: 检查 API.md 对 create 返回值描述
    Expected: 明确为序列化字节或一致方案
    Evidence: .sisyphus/evidence/task-14-header-create-doc.txt

  Scenario: mismatch resurfaced
    Tool: Bash
    Steps: 文档与代码自动对照
    Expected: 不再出现 create 返回类型冲突
    Evidence: .sisyphus/evidence/task-14-header-create-doc-error.txt
  ```

  **Commit**: YES | Message: `docs(header): align create return semantics with implementation` | Files: [docs/plan/API.md]

- [ ] 15. 更新公开 API 示例与导出路径校验

  **What to do**: 校准 README/API 示例里 `section::Entry`、`StandardItems`、validator 使用路径。
  **Must NOT do**: 不引入与实现不一致的新示例。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 示例契约质量。
  - Skills: `[]` - 无。
  - Omitted: `unspecified-high` - 非核心逻辑。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [17] | Blocked By: [6]

  **References**:
  - Pattern: `src/lib.rs` re-exports
  - API/Type: `docs/plan/API.md` examples

  **Acceptance Criteria**:
  - [ ] 示例可编译并符合当前导出路径。

  **QA Scenarios**:
  ```
  Scenario: doc examples compile
    Tool: Bash
    Steps: 运行示例/文档片段编译检查
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-15-doc-examples.txt

  Scenario: stale import path
    Tool: Bash
    Steps: 检测无效导入路径
    Expected: 无失效路径
    Evidence: .sisyphus/evidence/task-15-doc-examples-error.txt
  ```

  **Commit**: YES | Message: `docs(examples): align import paths and API usage examples` | Files: [docs/plan/API.md, README.md(optional)]

- [ ] 16. 全量回归：workspace test + clippy + fmt

  **What to do**: 执行统一质量门禁并修复由本次改动引入的问题。
  **Must NOT do**: 不跳过失败项；不删测试。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 全量验证闭环。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 范围广。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: [18,19] | Blocked By: [5,7,10,11]

  **References**:
  - Test: `cargo test --workspace`
  - Test: `cargo clippy --workspace`
  - Test: `cargo fmt --check`

  **Acceptance Criteria**:
  - [ ] 三项门禁全部通过。

  **QA Scenarios**:
  ```
  Scenario: quality gates pass
    Tool: Bash
    Steps: 依次运行 test/clippy/fmt
    Expected: 全部 0 失败
    Evidence: .sisyphus/evidence/task-16-quality-gates.txt

  Scenario: gate fails
    Tool: Bash
    Steps: 记录失败并修复后重跑
    Expected: 最终通过
    Evidence: .sisyphus/evidence/task-16-quality-gates-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 17. 生成“计划一致性验收报告”

  **What to do**: 汇总每个差异项的修复状态、测试证据、残余偏差（若有）与理由。
  **Must NOT do**: 不留“待人工确认”闭环。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 汇总输出任务。
  - Skills: `[]` - 无。
  - Omitted: `deep` - 非新问题探索。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: [18,19] | Blocked By: [4,8,9,12,13,14,15]

  **References**:
  - Pattern: `.sisyphus/evidence/task-*`
  - API/Type: `docs/plan/API.md`

  **Acceptance Criteria**:
  - [ ] 每个差异项都有“修复/保留+理由+证据”记录。

  **QA Scenarios**:
  ```
  Scenario: report completeness
    Tool: Bash
    Steps: 检查报告是否覆盖 strict/BAT/API-shape/UB 结论
    Expected: 全覆盖
    Evidence: .sisyphus/evidence/task-17-alignment-report.txt

  Scenario: missing evidence link
    Tool: Bash
    Steps: 验证每个结论均有证据文件
    Expected: 无缺失
    Evidence: .sisyphus/evidence/task-17-alignment-report-error.txt
  ```

  **Commit**: YES | Message: `docs(report): add implementation-to-plan alignment verification report` | Files: [.sisyphus/evidence/**, docs/**]

- [ ] 18. 代码质量复核（非功能）

  **What to do**: 复核新增逻辑是否引入重复、隐藏假设、不可维护分支。
  **Must NOT do**: 不追加范围外重构。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 非功能质量审计。
  - Skills: `[]` - 无。
  - Omitted: `quick` - 需要系统性复核。

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: [20] | Blocked By: [16,17]

  **References**:
  - Pattern: changed files diff

  **Acceptance Criteria**:
  - [ ] 无关键可维护性风险。

  **QA Scenarios**:
  ```
  Scenario: complexity stable
    Tool: Bash
    Steps: 运行静态检查并审阅 diff
    Expected: 无新增高风险代码味道
    Evidence: .sisyphus/evidence/task-18-code-quality.txt

  Scenario: hidden assumption found
    Tool: Bash
    Steps: 触发审计规则检查
    Expected: 若发现则修复并复审通过
    Evidence: .sisyphus/evidence/task-18-code-quality-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 19. 作用域忠实性复核（防 scope creep）

  **What to do**: 核查变更仅覆盖计划差异项与必要测试，不含额外功能。
  **Must NOT do**: 不扩大到新 feature。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 范围审计。
  - Skills: `[]` - 无。
  - Omitted: `writing` - 以审计为主。

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: [20] | Blocked By: [16,17]

  **References**:
  - Pattern: commit diff vs plan checklist

  **Acceptance Criteria**:
  - [ ] 无 scope creep；所有改动可映射到计划任务。

  **QA Scenarios**:
  ```
  Scenario: scope mapping pass
    Tool: Bash
    Steps: 对照改动文件与计划任务编号
    Expected: 100% 映射
    Evidence: .sisyphus/evidence/task-19-scope-fidelity.txt

  Scenario: unrelated change detected
    Tool: Bash
    Steps: 运行变更分类检查
    Expected: 检出并移除无关改动
    Evidence: .sisyphus/evidence/task-19-scope-fidelity-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 20. 发布前收口：变更摘要 + 执行说明

  **What to do**: 输出执行结果摘要、剩余风险（若有）、后续建议（可选项单列）。
  **Must NOT do**: 不声称完成未验证项。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 交付整理。
  - Skills: `[]` - 无。
  - Omitted: `unspecified-high` - 无高复杂度实现。

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: [] | Blocked By: [18,19]

  **References**:
  - Pattern: all evidence + final diff

  **Acceptance Criteria**:
  - [ ] 摘要与证据一致且可追溯。

  **QA Scenarios**:
  ```
  Scenario: final summary consistency
    Tool: Bash
    Steps: 校验摘要中的每条结论均有证据链接
    Expected: 全部可追溯
    Evidence: .sisyphus/evidence/task-20-final-summary.txt

  Scenario: unsupported claim
    Tool: Bash
    Steps: 扫描摘要中的未证实陈述
    Expected: 0 条
    Evidence: .sisyphus/evidence/task-20-final-summary-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- 原子提交，按任务簇提交：
  1) BAT correctness
  2) strict semantics
  3) tests matrix
  4) docs parity
  5) final report
- 每次提交前必须跑对应最小测试集；关键节点跑全量 workspace 测试。

## Success Criteria
- 不一致项全部闭环（修复或保留并给出计划依据）。
- “无 Rust UB”结论在自动化检查下可重复。
- 所有质量门禁通过，且无范围外改动。
