# Align Implementation with API/Standard Baseline

## TL;DR
> **Summary**: Close the three confirmed documentation-vs-implementation gaps with minimal, evidence-driven changes and explicit guardrails for read-only validation semantics.
> **Deliverables**:
> - `ValidationIssue` API parity with planned surface
> - Differencing-disk required metadata parity (`parent_locator`)
> - Decisioned path for stale parent-path writeback requirement
> **Effort**: Short
> **Parallel**: YES - 2 waves
> **Critical Path**: Task 1 → Task 2 → Task 4

## Context
### Original Request
编写计划（基于已确认的 3 处不一致）。

### Interview Summary
- 基线以 `docs/plan/API.md` 与 `docs/Standard/*` 为准。
- 已确认 3 处不一致：
  1) `ValidationIssue` 缺少 `message()` / `spec_ref()` 访问器；
  2) `validate_required_metadata_items()` 未在差分盘场景强制 `parent_locator`；
  3) 成功解析父链后缺少 stale path 回写行为。

### Metis Review (gaps addressed)
- Guardrail: 不将写入逻辑放入 `SpecValidator`（其职责为只读校验）。
- Risk: 第 3 项存在架构张力（标准建议回写 vs 只读校验器约束）。
- Action: 在计划中将第 3 项标记为 **[DECISION NEEDED]**，并给出默认落地方案。

## Work Objectives
### Core Objective
使实现行为与计划/标准基线一致，并保持 API 向后兼容与最小改动。

### Deliverables
1. 在 `ValidationIssue` 上补齐缺失访问器。
2. 在 required metadata 校验中补齐差分盘 `parent_locator` 约束。
3. 对 stale path 回写形成可执行实现路径（含测试与验收），待决策项显式化。

### Definition of Done (verifiable conditions with commands)
- `ValidationIssue` 暴露 `message()` 与 `spec_ref()`，并有测试覆盖。
- 差分盘缺失 `parent_locator` 时，`validate_required_metadata_items()` 返回 `Error::InvalidMetadata`。
- 全量测试通过。
- 验证命令：
  - `cargo test -p vhdx-rs`
  - `cargo test --workspace`

### Must Have
- 保持 `SpecValidator` 只读语义。
- 变更最小化，优先复用现有错误类型与断言模式。
- 每项任务包含可执行 QA 场景与证据路径。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不重构无关模块。
- 不新增“便利”API（如 `ValidationIssue::severity`）。
- 不修改 `misc/`、依赖配置与非范围文件。
- 不在 `SpecValidator` 内执行写回。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after（Rust `cargo test`）
- QA policy: Every task has agent-executed scenarios
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
Wave 1: API parity and validation parity
- Task 1 (quick): `ValidationIssue` getter parity
- Task 2 (quick): differencing required metadata parity

Wave 2: decisioned integration and hardening
- Task 3 (deep): stale path writeback design/implementation path
- Task 4 (quick): regression tests + docs parity assertions

### Dependency Matrix (full, all tasks)
- Task 1: Blocked By none; Blocks Task 4
- Task 2: Blocked By none; Blocks Task 4
- Task 3: Blocked By [DECISION NEEDED-1]; Blocks Task 4
- Task 4: Blocked By Tasks 1,2,3

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 2 tasks → `quick`
- Wave 2 → 2 tasks → `deep`, `quick`

## TODOs

- [x] 1. Add missing `ValidationIssue` accessors

  **What to do**:
  - 在 `src/validation.rs` 的 `impl ValidationIssue` 中新增：
    - `pub fn message(&self) -> String`
    - `pub const fn spec_ref(&self) -> &'static str`
  - 保持现有 `new/section/code` 签名不变。
  - 新增/更新测试以断言上述两个 getter 的行为。

  **Must NOT do**:
  - 不改字段类型（如 `String` → `&str`/`Cow`）。
  - 不改 derive 与可见性。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件小改+测试
  - Skills: `[]` - 无额外技能依赖
  - Omitted: `review-work` - 该阶段先完成实现与基础验证

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: Task 4 | Blocked By: none

  **References**:
  - Pattern: `src/validation.rs:48-80` - 现有 getter 风格
  - API/Type: `docs/plan/API.md:493-498` - 目标 API 面
  - Test: `tests/api_surface_smoke.rs` - 公共 API 冒烟断言模式

  **Acceptance Criteria**:
  - [ ] `ValidationIssue` 可调用 `message()` 与 `spec_ref()`。
  - [ ] 相关测试通过：`cargo test -p vhdx-rs`。

  **QA Scenarios**:
  ```
  Scenario: Accessor happy path
    Tool: Bash
    Steps: 运行 `cargo test -p vhdx-rs -- test_validation_api_import_and_validate_file`
    Expected: 测试通过，getter 可访问
    Evidence: .sisyphus/evidence/task-1-validationissue-getters.txt

  Scenario: API surface regression
    Tool: Bash
    Steps: 运行 `cargo test -p vhdx-rs -- smoke_validation_mod_import`
    Expected: 冒烟测试通过，无 API 破坏
    Evidence: .sisyphus/evidence/task-1-validationissue-getters-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): align ValidationIssue accessors with API baseline` | Files: `src/validation.rs`, `tests/*`

- [x] 2. Enforce differencing `parent_locator` in required metadata validation

  **What to do**:
  - 在 `validate_required_metadata_items()` 中，读取 `file_parameters` 并在 `has_parent()==true` 时强制 `PARENT_LOCATOR` 存在。
  - 缺失时返回 `Error::InvalidMetadata("Missing required metadata item: parent_locator")`（风格与现有 required 检查一致）。
  - 增加至少两个测试：
    1) 差分盘+完整元数据 => 通过；
    2) 差分盘+缺失 `parent_locator` => 失败。

  **Must NOT do**:
  - 不改变 `validate_parent_locator()` 与 `validate_parent_chain()` 语义。
  - 不引入新错误枚举。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 局部逻辑补齐
  - Skills: `[]`
  - Omitted: `oracle` - 非架构重设计

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: Task 4 | Blocked By: none

  **References**:
  - Pattern: `src/validation.rs:562-607` - 现有 required 项校验写法
  - API/Type: `docs/Standard/MS-VHDX-解读.md:214-224` - required known items
  - API/Type: `docs/Standard/MS-VHDX-只读扩展标准.md:73-89` - validator 责任边界

  **Acceptance Criteria**:
  - [ ] `has_parent=true` 且缺少 `parent_locator` 时必定报错。
  - [ ] 非差分盘不受该新增约束影响。

  **QA Scenarios**:
  ```
  Scenario: Differencing required metadata happy path
    Tool: Bash
    Steps: 运行新增差分盘 required metadata 正向测试
    Expected: 返回 Ok，测试通过
    Evidence: .sisyphus/evidence/task-2-required-parentlocator.txt

  Scenario: Missing parent_locator failure path
    Tool: Bash
    Steps: 运行新增缺失 parent_locator 负向测试
    Expected: 返回 Error::InvalidMetadata，断言通过
    Evidence: .sisyphus/evidence/task-2-required-parentlocator-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): enforce parent_locator for differencing required metadata` | Files: `src/validation.rs`, `tests/*`

- [x] 3. [DECISION NEEDED-1] Implement stale parent-path writeback strategy

  **What to do**:
  - 在不破坏 `SpecValidator` 只读语义前提下实现 stale path 回写。
  - 推荐默认方案（若用户未指定）：
    - 在 `File` 层新增显式可写方法（例如 `update_stale_parent_paths`），仅在写模式执行。
    - `validate_parent_chain()` 保持只读，调用方在校验成功后显式调用回写方法。
  - 为回写流程补充测试：成功回写、只读拒绝、父路径不存在错误。

  **Must NOT do**:
  - 不在 `SpecValidator` 中直接写盘。
  - 不把回写做成隐式副作用。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 涉及职责边界与 API 行为设计
  - Skills: `[]`
  - Omitted: `quick` - 需要先锁定架构决策

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: Task 4 | Blocked By: [DECISION NEEDED-1]

  **References**:
  - Pattern: `src/validation.rs:956-1040` - 当前父链只读校验路径
  - Pattern: `src/sections/metadata.rs:552-571` - 仅解析不回写的现状
  - External: `docs/Standard/MS-VHDX-解读.md:242-251` - stale entry writeback 要求

  **Acceptance Criteria**:
  - [ ] 回写能力存在且不在 `SpecValidator` 内。
  - [ ] 只读打开调用回写时返回受控错误。

  **QA Scenarios**:
  ```
  Scenario: Writeback happy path after successful chain validation
    Tool: Bash
    Steps: 构造差分盘链路、触发路径陈旧、执行校验+显式回写、再读回验证
    Expected: 路径键更新成功，校验通过
    Evidence: .sisyphus/evidence/task-3-stale-writeback.txt

  Scenario: Read-only rejection
    Tool: Bash
    Steps: 只读打开后调用回写入口
    Expected: 返回 ReadOnly 或 InvalidParameter，测试断言通过
    Evidence: .sisyphus/evidence/task-3-stale-writeback-error.txt
  ```

  **Commit**: YES | Message: `feat(file): add explicit stale parent path writeback flow` | Files: `src/file.rs`, `src/sections/metadata.rs`, `tests/*`

- [x] 4. Consolidate regression verification and docs-baseline assertions

  **What to do**:
  - 运行并收集任务 1-3 相关测试与全量回归结果。
  - 明确输出“与 `docs/plan/API.md`、`docs/Standard/*` 的一致性复核结果”。
  - 若 Task 3 因决策未落地，则在报告中标注 pending decision 并锁定其余两项已完成状态。

  **Must NOT do**:
  - 不跳过失败测试。
  - 不用人工主观判定替代命令证据。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 验证整合与结果汇总
  - Skills: `[]`
  - Omitted: `deep` - 不涉及新设计

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: none | Blocked By: Tasks 1,2,3

  **References**:
  - Test: `cargo test -p vhdx-rs`
  - Test: `cargo test --workspace`
  - Baseline: `docs/plan/API.md`, `docs/Standard/MS-VHDX-只读扩展标准.md`, `docs/Standard/MS-VHDX-解读.md`

  **Acceptance Criteria**:
  - [ ] 全量测试命令成功。
  - [ ] 形成二进制（pass/fail）的一致性结论。

  **QA Scenarios**:
  ```
  Scenario: Workspace-wide regression
    Tool: Bash
    Steps: 运行 `cargo test --workspace`
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-4-regression.txt

  Scenario: Targeted validation suite
    Tool: Bash
    Steps: 运行 `cargo test -p vhdx-rs`
    Expected: validation 相关测试通过
    Evidence: .sisyphus/evidence/task-4-regression-error.txt
  ```

  **Commit**: YES | Message: `test(validation): add coverage for docs-baseline parity` | Files: `tests/*`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- 每个任务单独原子提交，避免混合语义。
- 建议顺序：Task1 → Task2 → Task3 → Task4。
- 若 Task3 未决策，不阻塞 Task1/2/4 的提交，但需在 PR/报告中标记未决项。

## Success Criteria
- 与基线不一致项 1、2 清零。
- 不一致项 3 形成已实施或已决策待实施的明确状态。
- 所有测试与验证证据可追溯。
