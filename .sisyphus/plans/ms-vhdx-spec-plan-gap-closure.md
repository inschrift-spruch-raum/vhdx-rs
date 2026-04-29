# MS-VHDX 规范与 API 计划差距收敛计划

## TL;DR
> **Summary**: 以 `docs/plan/API.md` 为功能基线、以 `misc/MS-VHDX.md` 为规范约束，收敛当前实现中的 P0/P1 差距，优先修复可写打开 Header 生命周期与 Parent Locator 合规性。  
> **Deliverables**:
> - 可写打开 Header 更新语义对齐（Sequence/FileWriteGuid/LogGuid）
> - Parent Locator 写入与校验对齐（LocatorType、entry 约束、必需键）
> - `ReadOnlyNoReplay` 兼容模式明确标记为规范例外并回归覆盖
> - 全量与 targeted 回归证据
> **Effort**: Medium  
> **Parallel**: YES - 3 waves  
> **Critical Path**: Task 1 → Task 2 → Task 3 → Task 5 → Task 9

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md` 和 `misc/MS-VHDX.md` 计划差别，以计划为准。
- 编写执行计划。

### Interview Summary
- API 计划面大体已实现，主要差异在规范一致性。
- 高优先级缺口集中在：
  - 可写打开时 Header 更新语义不完整（`src/file.rs:715-798`, `src/file.rs:1118-1125`）。
  - Parent Locator 构造与校验不满足规范约束（`src/file.rs:1703-1737`，`src/validation.rs` Parent Locator 校验段）。
- `ReadOnlyNoReplay` 属于兼容模式（非严格规范路径），需明确标记与测试守护。

### Metis Review (gaps addressed)
- 增加“规范解释决议”任务，先固定 `parent_linkage2` 与 entry offset/length 解释，再编码。
- 明确范围边界，禁止顺手扩到 BAT/dynamic I/O/CLI 新能力。
- 每个任务补齐 agent-executable acceptance 与 happy/failure QA 场景。

## Work Objectives
### Core Objective
在不扩大范围的前提下，完成 P0/P1 规范差距闭环，使实现与 `docs/plan/API.md` 保持一致并对 MS-VHDX MUST/MUST NOT 关键条款给出可验证行为。

### Deliverables
- `src/file.rs`：可写 open 的 header 会话初始化更新路径；log replay 后 header 更新语义对齐。
- `src/file.rs`：Parent Locator payload 写入 LocatorType/Reserved/entry 约束对齐。
- `src/validation.rs`：Parent Locator 严格校验（type、entry 约束、必需 key/path）。
- `tests/integration_test.rs`：新增/收紧针对 Header 与 Parent Locator 的回归矩阵。
- `README.md` 与/或 `docs/API.md`：明确 `ReadOnlyNoReplay` 为兼容例外（若仓内文档策略要求）。

### Definition of Done (verifiable conditions with commands)
- `cargo test -p vhdx-rs test_open_writable_updates_header_session_fields -- --nocapture` 通过。
- `cargo test -p vhdx-rs test_parent_locator_locator_type_and_entry_constraints -- --nocapture` 通过。
- `cargo test -p vhdx-rs test_parent_locator_rejects_invalid_locator_type -- --nocapture` 通过。
- `cargo test -p vhdx-rs test_readonly_no_replay_is_explicit_compat_mode -- --nocapture` 通过。
- `cargo test --workspace` 通过。
- `cargo clippy --workspace` 通过（允许既有 warning，禁止新增 error）。

### Must Have
- 仅修复 P0/P1：Header 生命周期 + Parent Locator 合规 + 兼容模式说明。
- 每个实现任务必须绑定测试更新与失败路径断言。
- 所有规范映射必须落地到 `misc/MS-VHDX.md` 行号与源码行号。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不改 BAT 分配策略、动态盘未实现读写语义、`pub(crate) IO::write_sectors` 可见性。
- 不新增 public API，不改现有 public 函数签名。
- 不新增 CLI 子命令或扩展 CLI 语义。
- 不使用“人工目测”作为验收条件。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust test harness (`cargo test`)
- QA policy: Every task has agent-executed scenarios
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.txt`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave.

Wave 1 (规范决议 + 基础实现): Task 1, 2, 5, 8, 9  
Wave 2 (依赖 Wave1 的校验与回归): Task 3, 4, 6, 7  
Wave 3 (收敛与全量): Task 10

### Dependency Matrix (full, all tasks)
- Task 1 blocks: 2,3,5,6,7
- Task 2 blocks: 3,4
- Task 5 blocks: 6,7
- Task 8 blocks: 10
- Tasks 3/4/6/7 block: 10
- Task 9 can run after 2/5 with partial checks; final complete at 10

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 5 tasks → `deep`(1), `unspecified-high`(3), `writing`(1)
- Wave 2 → 4 tasks → `unspecified-high`(4)
- Wave 3 → 1 task → `unspecified-high`(1)

## TODOs
> Implementation + Test = ONE task. Never separate.

- [x] 1. 规范解释决议并固化为测试前置约束

  **What to do**:
  - 固化本轮规范解释：
    1) `open(write)` 触发一次 header 会话初始化更新；
    2) `parent_linkage` 必须存在，`parent_linkage2` 在 strict 模式下禁止出现；
    3) Parent Locator entry 偏移/长度采用“metadata item 内相对偏移且 >0”解释。
  - 将决议写入测试命名和断言注释（中文注释规则）。

  **Must NOT do**:
  - 不修改 public API；
  - 不改 CLI 行为。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 需统一规范解释与现有实现约束
  - Skills: `[]` - 无额外技能依赖
  - Omitted: `git-master` - 该任务不涉及 git 操作

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 2,3,5,6,7 | Blocked By: none

  **References**:
  - Pattern: `misc/MS-VHDX.md:422-426,442-453` - Header 会话更新规则
  - Pattern: `misc/MS-VHDX.md:1350,1370-1377,1394-1397` - Parent Locator 规则
  - API/Type: `src/file.rs:715-798` - open path
  - API/Type: `src/file.rs:1703-1737` - locator payload builder

  **Acceptance Criteria**:
  - [ ] 形成可执行断言清单并被 Task 2/5/6 测试直接引用。

  **QA Scenarios**:
  ```
  Scenario: Decision manifest consistency
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_spec_decision_manifest -- --nocapture
    Expected: 所有决议断言通过，输出包含 header-session 和 locator-constraints 关键字
    Evidence: .sisyphus/evidence/task-1-decision-manifest.txt

  Scenario: Strict mode rejects parent_linkage2
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_parent_locator_strict_rejects_parent_linkage2 -- --nocapture
    Expected: 返回 InvalidMetadata 且错误文本包含 parent_linkage2
    Evidence: .sisyphus/evidence/task-1-parent-linkage2-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): lock spec interpretation for header and locator checks` | Files: `tests/integration_test.rs`

- [x] 2. 实现可写打开 Header 会话初始化更新语义

  **What to do**:
  - 在 `open_file_with_options(... writable=true ...)` 中增加“首次会话 header 更新”路径。
  - 更新 non-current header，`SequenceNumber = current + 1`，首次会话设置新 `FileWriteGuid`。
  - 保持读模式行为不变。

  **Must NOT do**:
  - 不改变 `File::open(...).finish()` 只读路径。
  - 不在该任务中扩展 DataWriteGuid 触发范围到全部写路径之外。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 文件核心打开流程变更，需高谨慎
  - Skills: `[]`
  - Omitted: `playwright` - 无 UI

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 3,4 | Blocked By: 1

  **References**:
  - Pattern: `src/file.rs:715-798` - 当前 open 实现
  - Pattern: `src/file.rs:1118-1125` - replay 后 header 写入模式
  - External: `misc/MS-VHDX.md:422,442-453`

  **Acceptance Criteria**:
  - [ ] `open(write)` 后 header 序列号增加（至少 +1）。
  - [ ] 首次会话更新时 `FileWriteGuid` 变化。

  **QA Scenarios**:
  ```
  Scenario: Writable open updates header session fields
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_writable_updates_header_session_fields -- --nocapture
    Expected: 测试断言 sequence 增长且 file_write_guid 变化
    Evidence: .sisyphus/evidence/task-2-header-session-happy.txt

  Scenario: Readonly open keeps header stable
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_readonly_does_not_mutate_header_session_fields -- --nocapture
    Expected: sequence 与 file_write_guid 均不变
    Evidence: .sisyphus/evidence/task-2-header-session-error.txt
  ```

  **Commit**: YES | Message: `fix(file): apply writable-open header session update semantics` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 3. 对齐 log replay 后 Header 更新语义

  **What to do**:
  - 调整 `replay_log_and_clear_guid`，保证 replay 后 header 写入符合 sequence 递增与双头一致性策略。
  - 明确 `LogGuid` 清零时机与副作用。

  **Must NOT do**:
  - 不重写 log 扫描算法。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及崩溃一致性路径
  - Skills: `[]`
  - Omitted: `writing` - 该任务以代码/测试为主

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 4,10 | Blocked By: 2

  **References**:
  - Pattern: `src/file.rs:1110-1131` - 当前 replay+clear
  - External: `misc/MS-VHDX.md:426,836-838`
  - Test: `tests/integration_test.rs` 现有 log replay 测试模式

  **Acceptance Criteria**:
  - [ ] replay 后 active header `LogGuid == nil` 且 sequence 语义符合决议。

  **QA Scenarios**:
  ```
  Scenario: Replay then clear log guid with valid sequence
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_log_replay_clears_guid_and_updates_header_sequence -- --nocapture
    Expected: 回放成功，log_guid 为 nil，header sequence 更新断言通过
    Evidence: .sisyphus/evidence/task-3-log-replay-happy.txt

  Scenario: ReadOnlyNoReplay leaves disk unchanged
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_log_replay_policy_readonly_no_replay_keeps_pending -- --nocapture
    Expected: 文件未写入回放结果，保持 pending 标记
    Evidence: .sisyphus/evidence/task-3-log-replay-error.txt
  ```

  **Commit**: YES | Message: `fix(file): align replay header update and log guid lifecycle` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 4. 增加 Header 生命周期回归矩阵

  **What to do**:
  - 在 `tests/integration_test.rs` 增加可写/只读/重复打开三类矩阵。
  - 锁定 sequence 与 GUID 的最小不变量。

  **Must NOT do**:
  - 不引入 flaky 计时依赖。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 核心行为回归矩阵
  - Skills: `[]`
  - Omitted: `deep` - 已有规范决议

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 10 | Blocked By: 2

  **References**:
  - Test: `tests/integration_test.rs` 现有 validator/log 测试风格
  - API/Type: `src/file.rs:715-798`

  **Acceptance Criteria**:
  - [ ] 新增至少 3 个 header 生命周期测试并全部通过。

  **QA Scenarios**:
  ```
  Scenario: Repeated writable open increments sequence monotonically
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_writable_sequence_monotonicity -- --nocapture
    Expected: 连续两次 writable open 后 sequence 单调增加
    Evidence: .sisyphus/evidence/task-4-header-matrix-happy.txt

  Scenario: Readonly open in between does not mutate sequence
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_readonly_between_writable_keeps_sequence -- --nocapture
    Expected: readonly 不引起 sequence 变化
    Evidence: .sisyphus/evidence/task-4-header-matrix-error.txt
  ```

  **Commit**: YES | Message: `test(validation): add header lifecycle regression matrix` | Files: `tests/integration_test.rs`

- [x] 5. 修复 Parent Locator 写入格式（LocatorType/Reserved/entry）

  **What to do**:
  - `build_parent_locator_payload` 写入 VHDX `LocatorType` GUID，`Reserved=0`。
  - 修正 entry 偏移策略，使 key/value offset/length 满足 strict 规则。
  - 继续保证 `parent_linkage` 与 `relative_path` 写入。

  **Must NOT do**:
  - 不新增额外 parent path key（除非规范决议要求）。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 元数据二进制布局敏感
  - Skills: `[]`
  - Omitted: `writing` - 非文档任务

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 6,7,10 | Blocked By: 1

  **References**:
  - Pattern: `src/file.rs:1703-1737` - 现有 payload builder
  - External: `misc/MS-VHDX.md:1350-1354,1370-1377,1382-1397`
  - API/Type: `src/lib.rs:75-78` - `LOCATOR_TYPE_VHDX`

  **Acceptance Criteria**:
  - [ ] 新建差分盘 metadata 中 locator_type 为 `B04AEFB7-D19E-4A81-B789-25B8E9445913`。
  - [ ] key/value 长度与偏移满足 strict 断言。

  **QA Scenarios**:
  ```
  Scenario: Differencing create emits valid locator header
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_create_diff_parent_locator_has_vhdx_locator_type -- --nocapture
    Expected: locator_type 正确且 reserved 为 0
    Evidence: .sisyphus/evidence/task-5-locator-writer-happy.txt

  Scenario: Invalid entry offsets rejected by validator
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_parent_locator_rejects_zero_offsets_or_lengths -- --nocapture
    Expected: strict validator 返回 InvalidMetadata
    Evidence: .sisyphus/evidence/task-5-locator-writer-error.txt
  ```

  **Commit**: YES | Message: `fix(file): emit spec-compliant parent locator payload` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 6. 加强 Parent Locator 校验（type + entry + key 规则）

  **What to do**:
  - 在 `validate_parent_locator` 增加：
    - `locator_type` 必须为 VHDX locator；
    - key/value offset/length > 0；
    - key 唯一且必含 `parent_linkage`；
    - 至少一个路径键（`relative_path|volume_path|absolute_win32_path`）。

  **Must NOT do**:
  - 不在该任务扩展到链路递归遍历。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 规范校验逻辑集中变更
  - Skills: `[]`
  - Omitted: `deep` - 语义已在 Task1 决议

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 7,10 | Blocked By: 1,5

  **References**:
  - Pattern: `src/validation.rs:595-668` - 现有 parent locator 校验区段
  - Pattern: `src/sections/metadata.rs` ParentLocator/KeyValueEntry 读取方法
  - External: `misc/MS-VHDX.md:1350,1370-1379,1394-1397`

  **Acceptance Criteria**:
  - [ ] 非法 locator_type、零 offset/length、缺少 parent_linkage、无 path key 均被拒绝。

  **QA Scenarios**:
  ```
  Scenario: Valid locator passes strict validation
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_parent_locator_strict_valid -- --nocapture
    Expected: validate_parent_locator 返回 Ok(())
    Evidence: .sisyphus/evidence/task-6-locator-validator-happy.txt

  Scenario: Invalid locator type fails fast
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_parent_locator_rejects_invalid_type -- --nocapture
    Expected: InvalidMetadata 且消息包含 locator_type
    Evidence: .sisyphus/evidence/task-6-locator-validator-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): enforce strict parent locator conformance` | Files: `src/validation.rs`, `tests/integration_test.rs`

- [x] 7. 固化 `validate_parent_chain` 范围并补回归（单跳）

  **What to do**:
  - 保持单跳设计，明确行为：只校验 child->direct parent 的 DataWriteGuid 匹配。
  - 补齐 not-found/mismatch/happy 三类测试。

  **Must NOT do**:
  - 不引入递归全链与循环检测（列入后续 backlog）。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 行为边界固定与回归锁定
  - Skills: `[]`
  - Omitted: `artistry` - 不需要非常规方案

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 10 | Blocked By: 1,6

  **References**:
  - Pattern: `src/validation.rs:674-757` - 当前单跳实现
  - External: `misc/MS-VHDX.md:1394-1397`

  **Acceptance Criteria**:
  - [ ] 单跳成功路径通过；父盘缺失与 GUID mismatch 失败路径稳定。

  **QA Scenarios**:
  ```
  Scenario: Single-hop chain validation happy path
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_parent_chain_single_hop_happy -- --nocapture
    Expected: 返回 ParentChainInfo 且 linkage_matched=true
    Evidence: .sisyphus/evidence/task-7-parent-chain-happy.txt

  Scenario: Parent mismatch returns ParentMismatch
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_parent_chain_single_hop_mismatch -- --nocapture
    Expected: 返回 Error::ParentMismatch
    Evidence: .sisyphus/evidence/task-7-parent-chain-error.txt
  ```

  **Commit**: YES | Message: `test(validation): lock single-hop parent chain behavior` | Files: `tests/integration_test.rs`, `src/validation.rs`

- [x] 8. 明确并守护 ReadOnlyNoReplay 兼容例外

  **What to do**:
  - 在文档与测试中明确：`ReadOnlyNoReplay` 为 compatibility mode，不宣称严格规范一致。
  - 增加回归确保此策略不会误触发写盘回放。

  **Must NOT do**:
  - 不删除该策略。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档语义 + 测试契约
  - Skills: `[]`
  - Omitted: `deep` - 不是架构决策

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 10 | Blocked By: none

  **References**:
  - Pattern: `src/file.rs:115-124` - LogReplayPolicy variants
  - Pattern: `src/file.rs:968-975` - ReadOnlyNoReplay 分支
  - External: `misc/MS-VHDX.md:836-837`

  **Acceptance Criteria**:
  - [ ] 文档显式标注该策略为规范例外。
  - [ ] 对应测试证明不发生回放写入。

  **QA Scenarios**:
  ```
  Scenario: Compatibility mode documented and preserved
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_readonly_no_replay_is_explicit_compat_mode -- --nocapture
    Expected: 测试输出和断言包含 compat mode 语义
    Evidence: .sisyphus/evidence/task-8-compat-happy.txt

  Scenario: Strict replay policy still replays when required
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_log_replay_require_policy_replays_non_empty_log -- --nocapture
    Expected: Require 策略下按预期回放
    Evidence: .sisyphus/evidence/task-8-compat-error.txt
  ```

  **Commit**: YES | Message: `docs(file): mark readonly-no-replay as compatibility exception` | Files: `README.md`, `docs/API.md`, `tests/integration_test.rs`

- [x] 9. 执行 targeted gap suite（可重复）

  **What to do**:
  - 顺序运行本计划新增测试与现有相关回归，输出证据文件。
  - 确认至少连续两次结果一致。

  **Must NOT do**:
  - 不跳过失败测试；失败需修复后重跑。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 回归稳定性门禁
  - Skills: `[]`
  - Omitted: `writing` - 非文档任务

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 10 | Blocked By: 2,5

  **References**:
  - Test: `tests/integration_test.rs`

  **Acceptance Criteria**:
  - [ ] targeted suite 两轮一致通过。

  **QA Scenarios**:
  ```
  Scenario: Targeted suite pass round 1
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_writable_updates_header_session_fields -- --nocapture && cargo test -p vhdx-rs test_validate_parent_locator_strict_valid -- --nocapture
    Expected: 所有命令退出码 0
    Evidence: .sisyphus/evidence/task-9-targeted-round1.txt

  Scenario: Targeted suite pass round 2 (repeatability)
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_open_writable_updates_header_session_fields -- --nocapture && cargo test -p vhdx-rs test_validate_parent_locator_strict_valid -- --nocapture
    Expected: 与 round1 一致，无新增 flaky
    Evidence: .sisyphus/evidence/task-9-targeted-round2.txt
  ```

  **Commit**: NO | Message: `N/A` | Files: `N/A`

- [x] 10. 全量回归与静态检查收口

  **What to do**:
  - 运行 workspace 全量测试和 clippy。
  - 汇总全部证据并确认无新增 blocker。

  **Must NOT do**:
  - 不忽略 clippy error；warning 仅可沿用既有基线。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 最终交付门禁
  - Skills: `[]`
  - Omitted: `deep` - 执行型任务

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: Final Wave | Blocked By: 3,4,6,7,8,9

  **References**:
  - Command: `cargo test --workspace`
  - Command: `cargo clippy --workspace`

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` 通过。
  - [ ] `cargo clippy --workspace` 无 error。

  **QA Scenarios**:
  ```
  Scenario: Full workspace validation happy path
    Tool: Bash
    Steps: cargo test --workspace && cargo clippy --workspace
    Expected: 退出码 0
    Evidence: .sisyphus/evidence/task-10-full-regression-happy.txt

  Scenario: Clippy regression detection
    Tool: Bash
    Steps: cargo clippy --workspace
    Expected: 若出现 error 则任务失败并进入修复循环
    Evidence: .sisyphus/evidence/task-10-full-regression-error.txt
  ```

  **Commit**: NO | Message: `N/A` | Files: `N/A`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.**
- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- 按任务原子提交（2/3/5/6/8 可独立提交）。
- 测试-only 任务（4/7）单独提交，便于回滚。
- 最终门禁（9/10）不提交，仅产出 evidence。

## Success Criteria
- P0 缺口（Header 会话更新、Parent Locator 合规）全部关闭。
- API 计划与实现无新增偏差。
- compatibility exception 明确、可测试、可追踪。
- Final Wave 四项全部 APPROVE。
