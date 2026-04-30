# VHDX 规范一致性差距修复计划（以计划文档为准）

## TL;DR
> **Summary**: 按 `docs/plan/API.md` 与 `docs/Standard/MS-VHDX-只读扩展标准.md` 为契约源，修复实现与文档的 P0/P1 行为偏差，并补齐回归测试与 CLI 显式策略，避免默认行为漂移。
> **Deliverables**:
> - `File::open()` 默认策略与 strict 语义对齐
> - `InMemoryOnReadOnly` 门禁对齐
> - `validate_file()` 编排覆盖 parent chain
> - CLI 显式 log 策略与测试更新
> **Effort**: Medium
> **Parallel**: YES - 2 waves
> **Critical Path**: 1 → 2 → 3 → 5 → 6

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md`、`docs/Standard` 差别，以计划为准。
- 做出计划。

### Interview Summary
- 已完成并行审计与 Oracle 复核，确认 4 项核心行为差异（默认策略、strict=false、只读门禁、validate_file 编排）。
- 文档已统一“默认 log_replay=Require”的表述。

### Metis Review (gaps addressed)
- 关键风险：改默认策略会影响 CLI 行为；strict=false 修复会导致既有测试预期翻转。
- 防扩散策略：只改 `src/file.rs`、`src/validation.rs`、`vhdx-cli/src/commands/*` 与对应测试；不做 API 大重构。

## Work Objectives
### Core Objective
让实现行为与计划/标准文档一致，优先修复 MUST 级别偏差。

### Deliverables
- 默认 `File::open().finish()` 在有 pending log 时返回 `LogReplayRequired`（默认 `Require`）。
- `strict=false` 时仍拒绝 required unknown（region/metadata）。
- `InMemoryOnReadOnly` 仅在只读场景可执行。
- `validate_file()` 覆盖 parent chain 校验路径（对差分盘）。
- CLI 使用显式 log 策略，保持工具体验稳定。

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace`
- `cargo test -p vhdx-rs`
- `cargo test -p vhdx-tool`
- `cargo clippy --workspace`
- `cargo fmt --check`

### Must Have
- 行为对齐基准：`docs/plan/API.md` + `docs/Standard/MS-VHDX-只读扩展标准.md`。
- 每项变更都有对应测试（happy + edge/failure）。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `misc/`。
- 不新增依赖。
- 不做无关重构（命名迁移/模块重排/接口大改）。
- 不通过删测例“修复”。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after（Rust `cargo test`）
- QA policy: 每个任务都包含 agent 可执行场景。
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
Wave 1（基础行为修复）
- T1 默认策略对齐
- T2 strict 语义拆分修复
- T3 InMemoryOnReadOnly 只读门禁

Wave 2（依赖 Wave1 的联动修复）
- T4 validate_file 编排补齐
- T5 CLI 显式策略
- T6 测试更新与新增
- T7 文档注释与契约同步
- T8 全量验证与证据归档

### Dependency Matrix (full, all tasks)
- T1: Blocked By: none | Blocks: T5, T6, T8
- T2: Blocked By: none | Blocks: T6, T8
- T3: Blocked By: none | Blocks: T6, T8
- T4: Blocked By: none | Blocks: T6, T8
- T5: Blocked By: T1 | Blocks: T8
- T6: Blocked By: T1,T2,T3,T4 | Blocks: T8
- T7: Blocked By: T1,T2,T3,T4 | Blocks: T8
- T8: Blocked By: T1..T7 | Blocks: Final Wave

### Agent Dispatch Summary (wave → task count → categories)
- Wave1 → 3 tasks → `deep` / `ultrabrain`
- Wave2 → 5 tasks → `quick` / `unspecified-high`

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 默认日志策略改为 Require

  **What to do**: 将 `OpenOptions` 默认 `log_replay` 从 `Auto` 改为 `Require`；确保 `File::open(path).finish()` 在 pending log 场景返回 `Error::LogReplayRequired`。
  **Must NOT do**: 不修改 `open_file(...)` 内部创建路径的强制 `Auto`（创建流程依赖）。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 涉及默认行为变更与多路径回归影响。
  - Skills: `[]` - 无额外技能。
  - Omitted: `review-work` - 非最终验收阶段。

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T5,T6,T8 | Blocked By: none

  **References**:
  - Pattern: `src/file.rs:146-152` - `OpenOptions` 默认值定义。
  - Pattern: `src/file.rs:1528-1567` - `OpenOptions` builder API。
  - API/Type: `docs/plan/API.md:338-370` - 默认应为 `Require`。
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md:41-49` - `Require` 语义。

  **Acceptance Criteria**:
  - [ ] `File::open(path).finish()` 在 pending log 文件上返回 `LogReplayRequired`。
  - [ ] 无 pending log 时默认打开成功。

  **QA Scenarios**:
  ```
  Scenario: 默认打开遇到 pending log
    Tool: Bash
    Steps: cargo test test_default_open_rejects_pending_logs
    Expected: 测试通过，断言错误为 LogReplayRequired
    Evidence: .sisyphus/evidence/task-1-default-require.txt

  Scenario: 默认打开无 pending log
    Tool: Bash
    Steps: cargo test test_open_with_require_policy_no_pending_logs
    Expected: 测试通过
    Evidence: .sisyphus/evidence/task-1-default-require-happy.txt
  ```

  **Commit**: YES | Message: `fix(file): default log replay policy to require` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 2. 修复 strict=false 下 required unknown 必须失败

  **What to do**: 分离“required unknown 校验”和“optional unknown 放宽”逻辑；`strict=false` 仅放宽 optional unknown，required unknown 始终失败。
  **Must NOT do**: 不改变 strict=true 现有失败语义；不扩大到非 region/metadata 路径。

  **Recommended Agent Profile**:
  - Category: `ultrabrain` - Reason: 条件分支细粒度语义修复，易误伤。
  - Skills: `[]` - 无额外技能。
  - Omitted: `playwright` - 非 UI。

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T6,T8 | Blocked By: none

  **References**:
  - Pattern: `src/file.rs:750-753` - region unknown 分支。
  - Pattern: `src/file.rs:870-873` - metadata unknown 分支。
  - Test: `tests/integration_test.rs`（strict 相关测试）- 修正预期。
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md:34-40` - strict 规范。

  **Acceptance Criteria**:
  - [ ] `strict(false)` + required unknown region -> 失败。
  - [ ] `strict(false)` + optional unknown region -> 成功。
  - [ ] `strict(false)` + required unknown metadata -> 失败。

  **QA Scenarios**:
  ```
  Scenario: strict=false required unknown region
    Tool: Bash
    Steps: cargo test test_open_strict_false_rejects_required_unknown_region
    Expected: 测试通过，返回错误
    Evidence: .sisyphus/evidence/task-2-strict-required.txt

  Scenario: strict=false optional unknown region
    Tool: Bash
    Steps: cargo test test_open_strict_false_allows_optional_unknown_region
    Expected: 测试通过，打开成功
    Evidence: .sisyphus/evidence/task-2-strict-optional.txt
  ```

  **Commit**: YES | Message: `fix(file): enforce required-unknown checks in non-strict mode` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 3. 限制 InMemoryOnReadOnly 仅只读场景

  **What to do**: 在策略分发处增加写模式门禁；当 `write=true` 且需要执行 in-memory replay 时返回 `InvalidParameter`。
  **Must NOT do**: 不改变 `ReadOnlyNoReplay` 既有门禁；不引入落盘回放副作用。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 回放策略与打开模式耦合。
  - Skills: `[]` - 无额外技能。
  - Omitted: `artistry` - 常规逻辑修复。

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T6,T8 | Blocked By: none

  **References**:
  - Pattern: `src/file.rs:1014-1022` - `InMemoryOnReadOnly` 当前分支。
  - Pattern: `src/file.rs:1029-1033` - `ReadOnlyNoReplay` 只读门禁参考。
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md:53-58`。

  **Acceptance Criteria**:
  - [ ] 可写 + `InMemoryOnReadOnly` + pending log -> 返回参数错误。
  - [ ] 只读 + `InMemoryOnReadOnly` + pending log -> 以内存视图成功打开。

  **QA Scenarios**:
  ```
  Scenario: writable + InMemoryOnReadOnly
    Tool: Bash
    Steps: cargo test test_inmemory_on_readonly_rejects_writable_with_pending_logs
    Expected: 测试通过，返回 InvalidParameter
    Evidence: .sisyphus/evidence/task-3-inmemory-gate.txt

  Scenario: readonly + InMemoryOnReadOnly
    Tool: Bash
    Steps: cargo test test_inmemory_on_readonly_does_not_write_back_to_disk
    Expected: 测试通过
    Evidence: .sisyphus/evidence/task-3-inmemory-readonly.txt
  ```

  **Commit**: YES | Message: `fix(file): gate in-memory replay to read-only opens` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 4. `validate_file()` 编排补齐 parent chain

  **What to do**: 在差分盘场景下，将 `validate_parent_chain()` 纳入 `validate_file()` 编排；遇到链路不匹配返回可区分错误。
  **Must NOT do**: 不在非差分盘触发父盘 I/O；不吞掉 `ParentNotFound`/`ParentMismatch`。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 校验总入口语义变更。
  - Skills: `[]` - 无额外技能。
  - Omitted: `quick` - 涉及 I/O 和测试联动。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T6,T8 | Blocked By: none

  **References**:
  - Pattern: `src/validation.rs:64-75` - `validate_file` 当前编排。
  - API/Type: `docs/plan/API.md:36-45` - 计划中的 validator 编排。
  - External: `docs/Standard/MS-VHDX-只读扩展标准.md:71-82`。

  **Acceptance Criteria**:
  - [ ] 差分盘调用 `validate_file()` 时触发 parent chain 校验。
  - [ ] 非差分盘行为不变。

  **QA Scenarios**:
  ```
  Scenario: differencing validate_file with mismatched parent
    Tool: Bash
    Steps: cargo test test_validate_file_includes_parent_chain_mismatch
    Expected: 测试通过，返回 ParentMismatch
    Evidence: .sisyphus/evidence/task-4-validate-chain.txt

  Scenario: fixed/dynamic validate_file
    Tool: Bash
    Steps: cargo test test_validate_file_non_differencing_unchanged
    Expected: 测试通过
    Evidence: .sisyphus/evidence/task-4-validate-nondiff.txt
  ```

  **Commit**: YES | Message: `fix(validation): include parent chain in validate_file flow` | Files: `src/validation.rs`, `tests/integration_test.rs`

- [ ] 5. CLI 显式指定 log 策略，避免默认策略变更引发回归

  **What to do**: 为 `info/sections/diff/repair --dry-run` 显式设置策略：
  - `info/sections/diff` 使用 `Auto`（保持当前用户体验）
  - `repair --dry-run` 使用 `InMemoryOnReadOnly`（只读内存回放视图，不落盘）
  **Must NOT do**: 不改变 CLI 参数接口；不新增命令。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 点状调用链调整。
  - Skills: `[]` - 无额外技能。
  - Omitted: `deep` - 无架构重设计。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T8 | Blocked By: T1

  **References**:
  - Pattern: `vhdx-cli/src/commands/info.rs:23`
  - Pattern: `vhdx-cli/src/commands/sections_cmd.rs:25`
  - Pattern: `vhdx-cli/src/commands/diff.rs:22`
  - Pattern: `vhdx-cli/src/commands/repair.rs:26`

  **Acceptance Criteria**:
  - [ ] 上述命令不再依赖默认策略。
  - [ ] `repair --dry-run` 明确走 `InMemoryOnReadOnly`。
  - [ ] 既有 CLI 测试通过。

  **QA Scenarios**:
  ```
  Scenario: CLI info with pending logs
    Tool: Bash
    Steps: cargo test -p vhdx-tool info
    Expected: 子测试通过，不因默认 Require 直接失败
    Evidence: .sisyphus/evidence/task-5-cli-info.txt

  Scenario: CLI repair dry-run
    Tool: Bash
    Steps: cargo test -p vhdx-tool repair
    Expected: 子测试通过，按 dry-run 语义执行
    Evidence: .sisyphus/evidence/task-5-cli-repair.txt
  ```

  **Commit**: YES | Message: `fix(cli): set explicit log replay policies per command` | Files: `vhdx-cli/src/commands/*.rs`, `vhdx-cli/tests/*`

- [ ] 6. 修复并新增回归测试（覆盖 P0/P1）

  **What to do**: 更新 strict 相关既有测试预期；新增默认策略、门禁、validate_file 编排测试。
  **Must NOT do**: 不删除核心测试；不重写无关测试模块。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 测试矩阵较多且需避免误改。
  - Skills: `[]` - 无额外技能。
  - Omitted: `quick` - 非单点改动。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T8 | Blocked By: T1,T2,T3,T4

  **References**:
  - Test: `tests/integration_test.rs` - strict/log replay 相关用例。
  - Pattern: `src/file.rs` 与 `src/validation.rs` 对应分支。

  **Acceptance Criteria**:
  - [ ] 新增/修复的 8~12 个关键用例全部通过。
  - [ ] 无 flaky 行为。

  **QA Scenarios**:
  ```
  Scenario: 定向回归测试集
    Tool: Bash
    Steps: cargo test test_open_strict_false_rejects_required_unknown_region && cargo test test_default_open_rejects_pending_logs
    Expected: 全通过
    Evidence: .sisyphus/evidence/task-6-targeted-tests.txt

  Scenario: 全库测试
    Tool: Bash
    Steps: cargo test --workspace
    Expected: 全通过
    Evidence: .sisyphus/evidence/task-6-workspace-tests.txt
  ```

  **Commit**: YES | Message: `test(conformance): cover strict and log replay policy contracts` | Files: `tests/integration_test.rs`, `vhdx-cli/tests/*`

- [ ] 7. 文档同步（仅必要注释/说明）

  **What to do**: 对齐代码注释与行为说明（含中文注释规范）；确保 API/标准文档不再与实现冲突。
  **Must NOT do**: 不引入新规范条目；不修改 `misc/`。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档一致性清理。
  - Skills: `[]` - 无额外技能。
  - Omitted: `deep` - 非逻辑开发主任务。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T8 | Blocked By: T1,T2,T3,T4

  **References**:
  - Pattern: `docs/plan/API.md`。
  - Pattern: `docs/Standard/MS-VHDX-只读扩展标准.md`。

  **Acceptance Criteria**:
  - [ ] 文档与实现一致，无“默认策略”冲突描述。

  **QA Scenarios**:
  ```
  Scenario: 文档关键段落人工机检
    Tool: Bash
    Steps: cargo test test_log_replay_policy_variants_accessible
    Expected: 测试通过且策略说明与行为一致
    Evidence: .sisyphus/evidence/task-7-doc-sync.txt

  Scenario: 变更后总体验证
    Tool: Bash
    Steps: cargo test --workspace && cargo clippy --workspace
    Expected: 全通过
    Evidence: .sisyphus/evidence/task-7-quality.txt
  ```

  **Commit**: YES | Message: `docs(conformance): align policy semantics with implementation` | Files: `docs/plan/API.md`, `docs/Standard/*.md`

- [ ] 8. 收口验证与发布前审计

  **What to do**: 执行 fmt/check/test/clippy；汇总证据文件并生成变更说明。
  **Must NOT do**: 不跳过失败项；不带 warning 交付。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 流程性验证。
  - Skills: `[]` - 无额外技能。
  - Omitted: `ultrabrain` - 不需要复杂推理。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: Final Wave | Blocked By: T1,T2,T3,T4,T5,T6,T7

  **References**:
  - Commands: `cargo fmt --check`, `cargo test --workspace`, `cargo clippy --workspace`。

  **Acceptance Criteria**:
  - [ ] 三条质量命令全部通过。
  - [ ] 证据文件齐全。

  **QA Scenarios**:
  ```
  Scenario: 最终质量门禁
    Tool: Bash
    Steps: cargo fmt --check && cargo test --workspace && cargo clippy --workspace
    Expected: 全部通过
    Evidence: .sisyphus/evidence/task-8-final-gate.txt

  Scenario: CLI 独立验证
    Tool: Bash
    Steps: cargo test -p vhdx-tool
    Expected: 全通过
    Evidence: .sisyphus/evidence/task-8-cli-gate.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: `n/a`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- 原子提交，按任务编号提交（T1/T2/T3/T4/T5/T6/T7）。
- 禁止把不相关变更混入同一提交。

## Success Criteria
- P0/P1 差异全部关闭。
- 文档与行为一致，不再出现默认策略冲突。
- `cargo fmt --check`、`cargo test --workspace`、`cargo clippy --workspace` 全绿。
