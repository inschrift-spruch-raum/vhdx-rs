# 修复实现与计划差异（vhdx-rs / vhdx-tool）

## TL;DR
> **Summary**: 按 `docs/plan/API.md` 与 `misc/MS-VHDX.md` 基线，修复已确认的实现偏差：Dynamic 数据面关键缺口、CLI 已暴露未落地能力、文档契约漂移，并补齐自动化验证。
> **Deliverables**:
> - Dynamic 读路径按 BAT 返回真实数据（已分配块）并保留未分配块零填充语义
> - Dynamic 路径支持 ReplayOverlay 可见性
> - CLI `check`/`sections log`/`diff chain` 从 stub 变为可执行行为
> - README/API/AGENTS 命令与参数说明与实现对齐
> - 覆盖新增回归测试并通过 `cargo test --workspace`
> **Effort**: Medium
> **Parallel**: YES - 4 waves
> **Critical Path**: T1 → T2 → T3 → T9 → Final Verification

## Context
### Original Request
检查实际实现与 `docs/plan/API.md`、`misc/MS-VHDX.md` 计划差异，并按计划修复全部差异。

### Interview Summary
- 用户要求“开始做出修复以上所有内容的计划”。
- 已完成证据审计并按严重级别归档差异。
- 默认采用“计划为准、一次性覆盖所有已确认差距”的策略。

### Metis Review (gaps addressed)
- Guardrail 1: 不扩展到完整 Dynamic 自动分配（文件扩容 + BAT 持久化分配 + bitmap 完整生命周期），避免 scope 爆炸。
- Guardrail 2: `check` 命令保持“检查”语义，不在 `check --repair` 中隐式写盘；修复由 `repair` 命令承担。
- Guardrail 3: 所有 CLI 已声明能力必须要么落地实现、要么文档明确限制，禁止“参数存在但行为空实现”。
- Guardrail 4: Dynamic 修复必须覆盖 `File::read` 与 `IO::sector(...).read()` 间接路径。

## Work Objectives
### Core Objective
使实现行为与计划基线一致，消除“功能已声明但不可用/行为偏离”的核心缺口。

### Deliverables
- 库层（`src/file.rs`, `src/io_module.rs`）Dynamic 路径行为修复。
- CLI 层（`vhdx-cli/src/commands/*.rs`）补齐 `check`、`sections log`、`diff chain`。
- 文档层（`README.md`, `docs/API.md`, `AGENTS.md`）命令与参数契约对齐。
- 测试层（`tests/`, `vhdx-cli/tests/`）新增/更新覆盖并稳定通过。

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 全绿。
- `cargo test -p vhdx-rs` 包含 Dynamic 读路径新增回归测试并通过。
- `cargo test -p vhdx-tool` 包含 CLI 新行为测试并通过。
- `cargo build -p vhdx-tool` 成功。
- `vhdx-tool check <fixture> --log-replay` 行为可执行且输出不再为 not implemented。
- `vhdx-tool sections <fixture> log` 输出条目信息。
- `vhdx-tool diff <child> chain` 输出完整链。

### Must Have
- 修复 Dynamic 已分配块读取真实数据。
- 保留未分配/Zero/Unmapped 块读零语义。
- ReplayOverlay 在 Dynamic 读路径生效。
- CLI stubs 全部移除或替换为真实行为。
- 文档中包名、参数值、选项优先级与实现一致。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不改 `misc/`。
- 不新增依赖，不改 `Cargo.toml` 依赖策略。
- 不引入完整 Dynamic 自动分配功能（本计划明确排除）。
- 不修改公共 API 签名（除非仅文档注释对齐）。
- 不以删除测试绕过失败。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust test framework (`cargo test`)。
- QA policy: 每个任务均包含 Happy/Failure 场景，产出证据文件。
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Extract shared dependencies as Wave-1 tasks for max parallelism.

Wave 1: T1,T2,T3（库层关键路径，串行） + T8（文档对齐可并行）

Wave 2: T4,T5,T6（CLI 行为补齐，部分可并行，按共享文件分组）

Wave 3: T7（文档/API 树补齐） + T9（测试增补）

Wave 4: T10（全量回归与结果归档）

### Dependency Matrix (full, all tasks)
- T1 blocked by: none; blocks: T2,T3,T9,T10
- T2 blocked by: T1; blocks: T9,T10
- T3 blocked by: T1; blocks: T9,T10
- T4 blocked by: none; blocks: T9,T10
- T5 blocked by: T4; blocks: T9,T10
- T6 blocked by: none; blocks: T9,T10
- T7 blocked by: none; blocks: T10
- T8 blocked by: none; blocks: T10
- T9 blocked by: T1,T2,T3,T4,T5,T6; blocks: T10
- T10 blocked by: T7,T8,T9; blocks: Final Verification

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 4 tasks → `deep`, `unspecified-high`, `writing`
- Wave 2 → 3 tasks → `unspecified-high`, `quick`
- Wave 3 → 2 tasks → `writing`, `unspecified-high`
- Wave 4 → 1 task → `unspecified-high`

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [x] 1. 修复 Dynamic 读路径按 BAT 读取已分配块

  **What to do**:
  - 在 `src/file.rs` 中替换当前 Dynamic 分支零填充逻辑，按 `offset/len` 跨块读取。
  - 每块读取时解析 BAT 对应 payload entry：
    - `FullyPresent/PartiallyPresent/Undefined(按现有策略)`：从 `file_offset_mb * MiB + block_inner_offset` 读取真实数据。
    - `NotPresent/Zero/Unmapped`：填充零。
  - 确保 `IO::sector(...).read()` 间接调用路径获得同样行为。
  - 保持固定盘分支行为不变。
  
  **Must NOT do**:
  - 不实现完整 Dynamic 自动分配。
  - 不更改公共 API 签名。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 涉及文件格式语义与跨块读取一致性
  - Skills: `[]` - 使用现有 Rust 代码模式即可
  - Omitted: `playwright` - 非 UI 任务

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 2,3,9,10 | Blocked By: none

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/file.rs:240-257` - 当前 Fixed/Dynamic 分支结构
  - Pattern: `src/file.rs:301-333` - dynamic 写路径对 BAT/状态的既有处理思路
  - API/Type: `src/sections/bat.rs:202-364` - `BatEntry`/`BatState`/`PayloadBlockState`
  - API/Type: `src/io_module.rs:40-123` - `Sector::read` 通过 `read_raw` 间接走 `File::read`
  - External: `misc/MS-VHDX.md` - BAT 与块状态语义基线

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-rs test_read_unallocated_dynamic_block` 通过（未分配块读零回归）
  - [ ] 新增测试验证：已分配 Dynamic 块读取返回真实数据（非全零）并通过 `cargo test -p vhdx-rs`

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - dynamic allocated block read returns actual payload
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_dynamic_read_allocated_block_returns_data -- --nocapture
    Expected: 测试通过，断言读到写入的 payload 字节
    Evidence: .sisyphus/evidence/task-1-dynamic-read-ok.txt

  Scenario: Failure/edge case - unallocated dynamic block remains zero-filled
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_read_unallocated_dynamic_block -- --nocapture
    Expected: 测试通过，读取缓冲区全零
    Evidence: .sisyphus/evidence/task-1-dynamic-read-error.txt
  ```

  **Commit**: YES | Message: `fix(core): implement BAT-backed dynamic reads` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 2. 让 ReplayOverlay 在 Dynamic 读路径生效

  **What to do**:
  - 在 `src/file.rs` Dynamic 读取分支完成真实读取/零填充后，应用 `apply_replay_overlay`（与 Fixed 路径一致）。
  - 保证 readonly + in-memory replay 策略下，Dynamic 读结果可见 overlay 变更。

  **Must NOT do**:
  - 不改变 `LogReplayPolicy` 枚举与外部行为契约。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 逻辑聚焦、改动面小但需语义严谨
  - Skills: `[]` - 无额外技能
  - Omitted: `oracle` - 已有方案明确

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 9,10 | Blocked By: 1

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/file.rs:245-247` - Fixed 路径 overlay 应用位置
  - Pattern: `src/file.rs:685-708` - `apply_replay_overlay` 行为
  - API/Type: `src/file.rs:31-52` - `LogReplayPolicy` 语义

  **Acceptance Criteria** (agent-executable only):
  - [ ] 新增测试覆盖 readonly replay 场景并通过 `cargo test -p vhdx-rs`
  - [ ] Dynamic + pending log 情况下读取结果体现 overlay（非旧值）

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - dynamic read observes in-memory replay overlay
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_dynamic_read_applies_replay_overlay -- --nocapture
    Expected: 测试通过，读取值匹配 overlay 后结果
    Evidence: .sisyphus/evidence/task-2-overlay-ok.txt

  Scenario: Failure/edge case - policy forbids replay and surfaces expected state
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_readonly_no_replay_policy_behavior -- --nocapture
    Expected: 测试通过，结果符合 NoReplay 语义（不应用 overlay）
    Evidence: .sisyphus/evidence/task-2-overlay-error.txt
  ```

  **Commit**: YES | Message: `fix(core): apply replay overlay to dynamic reads` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 3. 修正 Dynamic 写路径对 BAT payload entry 的索引与状态处理

  **What to do**:
  - 在 `src/file.rs::write_dynamic` 中确保按 payload block 语义解析 BAT 条目，避免误用 sector bitmap entry。
  - 维持“未分配块暂不自动分配”的显式错误，但错误文本要准确表明限制边界。
  - 对越界和非法状态路径保持稳定错误类型。

  **Must NOT do**:
  - 不落地完整 block allocation。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: BAT 交错条目语义易出隐性错误
  - Skills: `[]` - 无额外技能
  - Omitted: `artistry` - 常规工程修复

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 9,10 | Blocked By: 1

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/file.rs:301-333` - 当前 `write_dynamic` 逻辑
  - API/Type: `src/sections/bat.rs:157-186` - `is_sector_bitmap_entry_index`
  - API/Type: `src/sections/bat.rs:267-290` - `BatState::from_bits_with_context`
  - Test: `src/sections/bat.rs` 相关测试（bitmap 索引行为）

  **Acceptance Criteria** (agent-executable only):
  - [ ] 新增/更新测试验证 payload/bitmap 索引不混淆
  - [ ] `cargo test -p vhdx-rs` 通过且无既有 Dynamic 写路径回归

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - write to fully present payload block succeeds
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_dynamic_write_fully_present_block -- --nocapture
    Expected: 测试通过，写入后读取一致
    Evidence: .sisyphus/evidence/task-3-write-ok.txt

  Scenario: Failure/edge case - write to unallocated block returns explicit limitation error
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_dynamic_write_unallocated_block_reports_limitation -- --nocapture
    Expected: 测试通过，错误类型/消息与限制一致
    Evidence: .sisyphus/evidence/task-3-write-error.txt
  ```

  **Commit**: YES | Message: `fix(core): correct dynamic write BAT entry handling` | Files: `src/file.rs`, `tests/integration_test.rs`

- [x] 4. 实现 `check` 命令真实校验流程（接入 SpecValidator）

  **What to do**:
  - 在 `vhdx-cli/src/commands/check.rs` 将“仅打开即成功”改为真实校验：调用 `vhdx_file.validator().validate_file()`。
  - 输出结构化校验结果（通过/失败项计数、关键错误摘要）。
  - 失败时设置非零退出码。

  **Must NOT do**:
  - 不仅打印固定“✓”模板文本。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: CLI 逻辑与错误语义对齐
  - Skills: `[]` - 无
  - Omitted: `deep` - 不涉及底层格式重构

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 5,9,10 | Blocked By: none

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `vhdx-cli/src/commands/check.rs:21-61` - 当前 stub 行为
  - API/Type: `src/file.rs:185` - `validator()` 入口
  - API/Type: `src/validation.rs` - `validate_file` 与 issue 结构
  - Test: `vhdx-cli/tests/cli_integration.rs` - check 命令现有测试模式

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-tool test_check_*` 通过
  - [ ] 对损坏样本 check 返回非零且输出具体失败项

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - check validates healthy file
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_check_command_with_valid_file -- --nocapture
    Expected: 测试通过，输出含 validation passed 信号
    Evidence: .sisyphus/evidence/task-4-check-ok.txt

  Scenario: Failure/edge case - corrupted structure triggers non-zero exit
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_check_reports_validation_failures -- --nocapture
    Expected: 测试通过，断言 exit code != 0 且 stderr 包含错误摘要
    Evidence: .sisyphus/evidence/task-4-check-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): run spec validator in check command` | Files: `vhdx-cli/src/commands/check.rs`, `vhdx-cli/tests/cli_integration.rs`

- [x] 5. 落地 `check --log-replay` 与 `check --repair` 语义（去除 not implemented）

  **What to do**:
  - `--log-replay`：在 check 流中执行可验证的 replay 路径（建议使用明确 policy），并报告 replay 结果。
  - `--repair`：在 check 命令内不进行隐式写修复；输出“请使用 repair 子命令”并以清晰状态码反馈。
  - 移除 `not yet implemented` 文案。

  **Must NOT do**:
  - 不在 check 中悄悄写文件（保持命令语义清晰）。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 逻辑改动集中在单文件分支处理
  - Skills: `[]` - 无
  - Omitted: `deep` - 非底层复杂算法

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 9,10 | Blocked By: 4

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `vhdx-cli/src/commands/check.rs:50-58` - 现有 not implemented 分支
  - API/Type: `src/file.rs` - `LogReplayPolicy` / `OpenOptions::log_replay`
  - Pattern: `vhdx-cli/src/commands/repair.rs` - repair 子命令职责边界

  **Acceptance Criteria** (agent-executable only):
  - [ ] check 命令输出中不再出现 not implemented
  - [ ] `--log-replay` 路径具备可验证行为并有测试覆盖
  - [ ] `--repair` 在 check 中返回明确指引与可测试状态

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - check with --log-replay executes replay-aware flow
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_check_command_with_log_replay_flag -- --nocapture
    Expected: 测试通过，输出包含 replay 执行结果（非占位文本）
    Evidence: .sisyphus/evidence/task-5-check-replay-ok.txt

  Scenario: Failure/edge case - check --repair returns explicit guidance without mutation
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_check_command_with_repair_flag -- --nocapture
    Expected: 测试通过，输出指向 repair 子命令且状态符合预期
    Evidence: .sisyphus/evidence/task-5-check-repair-error.txt
  ```

  **Commit**: YES | Message: `fix(cli): define check replay and repair semantics` | Files: `vhdx-cli/src/commands/check.rs`, `vhdx-cli/tests/cli_integration.rs`

- [x] 6. 实现 `sections log` 真实输出

  **What to do**:
  - 在 `vhdx-cli/src/commands/sections_cmd.rs` 中读取 `sections().log()` 并展示：entry 数量、关键 header 字段、descriptor 概要。
  - 对无日志/解析失败分别给出稳定输出。

  **Must NOT do**:
  - 不再输出固定占位文本“not yet implemented”。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 输出型功能，范围单一
  - Skills: `[]` - 无
  - Omitted: `oracle` - 无架构不确定性

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 9,10 | Blocked By: none

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `vhdx-cli/src/commands/sections_cmd.rs:98-103` - 现有 stub
  - API/Type: `src/sections/log.rs:123-237` - `entries()` 与 replay 相关方法
  - API/Type: `src/sections/log.rs:335-441` - `LogEntryHeader` 字段

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-tool test_sections_log_command` 通过
  - [ ] log 子命令输出包含 entry/header 关键信息

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - sections log shows parsed entry summary
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_sections_log_command -- --nocapture
    Expected: 测试通过，输出包含 sequence_number/descriptor_count 等字段
    Evidence: .sisyphus/evidence/task-6-sections-log-ok.txt

  Scenario: Failure/edge case - file without pending logs handled gracefully
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_sections_log_command_on_clean_file -- --nocapture
    Expected: 测试通过，输出明确“无待回放日志”且退出成功
    Evidence: .sisyphus/evidence/task-6-sections-log-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): implement sections log output` | Files: `vhdx-cli/src/commands/sections_cmd.rs`, `vhdx-cli/tests/cli_integration.rs`

- [x] 7. 实现 `diff chain` 真实链路遍历

  **What to do**:
  - 在 `vhdx-cli/src/commands/diff.rs` 中基于 metadata parent locator 递归/迭代打开父盘，打印完整链。
  - 检测循环引用/缺失父盘并给出错误输出与退出码。

  **Must NOT do**:
  - 不仅输出静态提示“not yet implemented”。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及链路解析与错误分支完整性
  - Skills: `[]` - 无
  - Omitted: `deep` - 不涉及格式底层变更

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: 10 | Blocked By: none

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `vhdx-cli/src/commands/diff.rs:62-76` - 当前 chain stub
  - API/Type: `src/sections/metadata.rs:539-558` - `resolve_parent_path`
  - API/Type: `src/validation.rs` - parent chain 校验模式

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-tool test_diff_chain_command` 通过
  - [ ] 三层链路样本输出完整顺序（child -> parent -> base）

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - diff chain traverses full parent lineage
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_diff_chain_command -- --nocapture
    Expected: 测试通过，stdout 包含完整链路路径序列
    Evidence: .sisyphus/evidence/task-7-diff-chain-ok.txt

  Scenario: Failure/edge case - missing parent path returns clear error
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_diff_chain_missing_parent_fails -- --nocapture
    Expected: 测试通过，非零退出并包含缺失父盘提示
    Evidence: .sisyphus/evidence/task-7-diff-chain-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): implement diff chain traversal` | Files: `vhdx-cli/src/commands/diff.rs`, `vhdx-cli/tests/cli_integration.rs`

- [x] 8. 对齐 README 与 CLI 契约文档（包名/参数/值）

  **What to do**:
  - 修正 README 中 `cargo build -p vhdx-cli` 为 `cargo build -p vhdx-tool`。
  - 补充 create 参数契约：`--type` 与 `--disk-type` 的优先级、`--force` 语义。
  - 对 `disk type` 值说明与实际 ValueEnum 一致（`differencing`）。

  **Must NOT do**:
  - 不篡改未实现能力为“已实现”。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档一致性修正
  - Skills: `[]` - 无
  - Omitted: `unspecified-high` - 非复杂代码任务

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 10 | Blocked By: none

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `README.md` CLI 使用章节
  - Pattern: `AGENTS.md` 构建与命令章节
  - API/Type: `vhdx-cli/src/cli.rs:41-69,154-166` - 真实参数与枚举值

  **Acceptance Criteria** (agent-executable only):
  - [ ] README 不再出现 `cargo build -p vhdx-cli`
  - [ ] 文档中 create 参数说明与 `cli.rs` 一致

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - documented build command matches actual package name
    Tool: Bash
    Steps: cargo build -p vhdx-tool
    Expected: 构建成功
    Evidence: .sisyphus/evidence/task-8-doc-build-ok.txt

  Scenario: Failure/edge case - legacy package name rejected (guard against regression)
    Tool: Bash
    Steps: cargo build -p vhdx-cli
    Expected: 命令失败（package not found），用于证明文档修正必要性
    Evidence: .sisyphus/evidence/task-8-doc-build-error.txt
  ```

  **Commit**: YES | Message: `docs(cli): align package and option contracts` | Files: `README.md`, `AGENTS.md`

- [x] 9. 补齐/更新回归测试（库 + CLI）

  **What to do**:
  - 在 `tests/integration_test.rs` 新增 Dynamic 已分配块读取、overlay 可见性、写路径限制明确性测试。
  - 在 `vhdx-cli/tests/cli_integration.rs` 新增/更新 check replay、sections log、diff chain 场景。
  - 保持既有测试命名与夹具模式一致（tempdir/fixture gate）。

  **Must NOT do**:
  - 不依赖人工验证。
  - 不移除旧测试规避失败。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 多文件测试设计与回归边界管理
  - Skills: `[]` - 无
  - Omitted: `writing` - 以可执行测试为主

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: 10 | Blocked By: 1,2,3,4,5,6

  **References** (executor has NO interview context - be exhaustive):
  - Test: `tests/integration_test.rs` - 现有 helper 与注入模式
  - Test: `vhdx-cli/tests/cli_integration.rs` - assert_cmd 测试模式
  - Pattern: `src/file.rs` dynamic/read/replay 分支
  - Pattern: `vhdx-cli/src/commands/*.rs` 新行为路径

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-rs` 全绿且新增测试运行
  - [ ] `cargo test -p vhdx-tool` 全绿且新增测试运行

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - new regression tests pass for library and CLI
    Tool: Bash
    Steps: cargo test -p vhdx-rs && cargo test -p vhdx-tool
    Expected: 两个包测试全部通过
    Evidence: .sisyphus/evidence/task-9-tests-ok.txt

  Scenario: Failure/edge case - intentionally malformed input yields expected failures
    Tool: Bash
    Steps: cargo test -p vhdx-tool test_diff_chain_missing_parent_fails -- --nocapture
    Expected: 该测试通过（命令内部失败分支被正确断言）
    Evidence: .sisyphus/evidence/task-9-tests-error.txt
  ```

  **Commit**: YES | Message: `test(regression): cover dynamic and cli parity scenarios` | Files: `tests/integration_test.rs`, `vhdx-cli/tests/cli_integration.rs`

- [x] 10. 全量回归、构建与证据归档

  **What to do**:
  - 运行 `cargo fmt --check`, `cargo clippy --workspace`, `cargo test --workspace`, `cargo build -p vhdx-tool`。
  - 收集命令输出到 `.sisyphus/evidence/`，并形成最终执行摘要。

  **Must NOT do**:
  - 不跳过失败项，不使用降级参数绕过。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 最终稳定性与质量门禁
  - Skills: `[]` - 无
  - Omitted: `quick` - 需要完整质量检查

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: Final Verification | Blocked By: 7,8,9

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `AGENTS.md` 构建/测试标准命令
  - Test: 整个 workspace 测试基线

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo fmt --check` 通过
  - [ ] `cargo clippy --workspace` 通过
  - [ ] `cargo test --workspace` 通过
  - [ ] `cargo build -p vhdx-tool` 通过

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - full quality gate passes
    Tool: Bash
    Steps: cargo fmt --check && cargo clippy --workspace && cargo test --workspace && cargo build -p vhdx-tool
    Expected: 全部命令成功退出
    Evidence: .sisyphus/evidence/task-10-gate-ok.txt

  Scenario: Failure/edge case - any gate failure blocks completion
    Tool: Bash
    Steps: cargo clippy --workspace
    Expected: 若有 warning->error 策略触发则任务标记失败并回滚到修复环
    Evidence: .sisyphus/evidence/task-10-gate-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: `n/a`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- Commit 1: `fix(core): correct dynamic read path and replay overlay behavior`
- Commit 2: `feat(cli): implement check replay flow, log section view, and diff chain traversal`
- Commit 3: `docs(cli): align package name and option/value contracts with implementation`
- Commit 4: `test(regression): add dynamic path and cli behavior coverage`

## Success Criteria
- 无 Critical/High 差异残留。
- 计划基线与实现行为一致，且有自动化证据可复验。
