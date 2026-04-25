# MS-VHDX Gap Closure Plan (Plan-First)

## TL;DR
> **Summary**: 以 `docs/plan/API.md` 为主契约，修复 3 个已证实偏差：`validate_file` 覆盖不足、RegionTable CRC 校验未接入、`IO::sector` 与字节边界语义潜在不一致。
> **Deliverables**:
> - `SpecValidator::validate_file` 在差分盘条件下纳入 `validate_parent_locator`
> - `validate_region_table` 接入 RegionTable CRC 校验
> - `IO::sector` 尾部可寻址语义修复 + 回归测试
> - CLI `check` 输出补齐 Parent Locator 检查项（仅差分盘）
> **Effort**: Medium
> **Parallel**: YES - 2 waves
> **Critical Path**: 1 → 2 → 3 → 4 → 5 → 6

## Context
### Original Request
- 编写用于抹平 gap 的计划（计划优先）。

### Interview Summary
- 用户要求直接继续，不增加访谈阻塞。
- 以 `docs/plan/API.md` 为准，`misc/MS-VHDX.md` 用于规范语义参照。
- 已完成分析并确认 3 个真实 gap 与 2 个高风险歧义。

### Metis Review (gaps addressed)
- 保持范围收敛：不把 `validate_parent_chain` 自动并入 `validate_file`（避免 I/O 副作用）。
- RegionTable CRC 先接入 validator 路径，不改 open 路径行为。
- `IO::sector` 语义差异纳入本次修复并以测试锁定；不额外扩展到日志序列策略重构。

## Work Objectives
### Core Objective
- 在不引入范围蔓延的前提下，完成 plan-first 合约闭环，使实现行为与 API 计划文档一致并有自动化证据。

### Deliverables
- 代码修复：`src/validation.rs`, `src/io_module.rs`, `vhdx-cli/src/commands/check.rs`
- 测试补强：`tests/integration_test.rs`, `vhdx-cli/tests/cli_integration.rs`
- 证据产物：`.sisyphus/evidence/task-*.txt`

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 通过。
- `cargo clippy --workspace` 通过（无新增回归）。
- 新增/变更测试覆盖三类 gap：validator 覆盖、RegionTable CRC、IO sector 边界。

### Must Have
- 仅在差分盘时触发 parent locator 校验。
- RegionTable CRC 在 `validate_region_table` 中被强制检查。
- `IO::sector` 对尾部场景行为与数据面语义一致。
- CLI `check` 输出增加 Parent Locator 项且语义准确。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `misc/`。
- 不新增依赖。
- 不将 `validate_parent_chain` 自动合入 `validate_file`。
- 不在本计划内改动 open 路径的 RegionTable CRC 失败策略。
- 不进行与 gap 无关的重构/重命名。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust (`cargo test`, `cargo clippy`)
- QA policy: Every task has agent-executed scenarios
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.txt`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Extract shared dependencies as Wave-1 tasks for max parallelism.

Wave 1: validator 与 IO 行为修复（核心代码 + 单元/集成测试）
- Task 1: RegionTable CRC 校验接入（validation）
- Task 2: validate_file 差分盘 parent locator 条件校验接入
- Task 3: IO::sector 尾部语义修复方案落地
- Task 4: 对应集成测试补强（validator + IO）

Wave 2: CLI 表现层与全量回归
- Task 5: CLI check 增加 Parent Locator 检查项（差分盘条件）
- Task 6: CLI 集成测试补强
- Task 7: 工作区全量回归与证据归档

### Dependency Matrix (full, all tasks)
| Task | Depends On | Blocks |
|---|---|---|
| 1 | - | 4,7 |
| 2 | - | 4,5,7 |
| 3 | - | 4,7 |
| 4 | 1,2,3 | 7 |
| 5 | 2 | 6,7 |
| 6 | 5 | 7 |
| 7 | 1,2,3,4,5,6 | Final Wave |

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 4 tasks → `unspecified-high` ×3, `quick` ×1
- Wave 2 → 3 tasks → `quick` ×2, `unspecified-high` ×1

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 在 `validate_region_table` 接入 RegionTable CRC 校验

  **What to do**:
  - 在 `src/validation.rs::validate_region_table` 中，签名检查后立刻调用 `region_table.header().verify_checksum()`。
  - 将 `InvalidChecksum` 映射为可诊断错误信息并保持现有错误风格。
  - 保持 open 路径不改动（本计划仅修 validator 路径）。

  **Must NOT do**:
  - 不修改 `File::open_file_with_options` 的行为。
  - 不引入新的错误类型。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件低耦合修复。
  - Skills: `[]` - 无额外技能依赖。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [4,7] | Blocked By: []

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/validation.rs:120-185` - region table 现有校验骨架。
  - Pattern: `src/validation.rs:96-100` - header checksum 校验风格可复用。
  - API/Type: `src/sections/header.rs:535-543` - `RegionTableHeader::verify_checksum()`。
  - External: `docs/plan/API.md:449-450` - Region Table 校验契约。

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test -p vhdx-rs test_t6_validator_region_table_rejects_required_unknown_region -- --nocapture` 通过。
  - [ ] 新增 RegionTable CRC 负例测试可稳定失败于修复前语义并在修复后通过。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - region table checksum valid
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_t6_validator_header_and_region_table_happy_path -- --nocapture`
    Expected: Exit code 0; Header/Region 验证通过
    Evidence: .sisyphus/evidence/task-1-regiontable-crc-happy.txt

  Scenario: Failure/edge case - region table checksum corrupted
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_validate_region_table_detects_corrupted_crc -- --nocapture`
    Expected: 校验返回 checksum 相关错误并断言命中
    Evidence: .sisyphus/evidence/task-1-regiontable-crc-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): enforce region table crc verification` | Files: `src/validation.rs`, `tests/integration_test.rs`

- [ ] 2. `validate_file` 在差分盘条件下纳入 parent locator 校验

  **What to do**:
  - 在 `src/validation.rs::validate_file` 中，在既有六项校验后增加条件分支：仅 `has_parent=true` 时调用 `validate_parent_locator()`。
  - 非差分盘保持现有行为（无 parent locator 强制）。

  **Must NOT do**:
  - 不自动调用 `validate_parent_chain()`。
  - 不把父链可达性（文件 I/O）并入 `validate_file`。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件轻改动，需严格边界控制。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [4,5,7] | Blocked By: []

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/validation.rs:61-69` - `validate_file` 当前调用链。
  - API/Type: `src/validation.rs:583-657` - `validate_parent_locator` 实现。
  - API/Type: `src/validation.rs:659-747` - `validate_parent_chain`（仅参考，禁止并入）。
  - Test: `tests/integration_test.rs:1850-1948` - validator API 现有测试区。
  - External: `docs/plan/API.md:466-475` - parent locator/chain 职责划分。

  **Acceptance Criteria** (agent-executable only):
  - [ ] 差分盘场景下 `validate_file()` 会覆盖 parent locator 约束。
  - [ ] 非差分盘场景下 `validate_file()` 行为不回归。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - differencing disk validate_file includes parent locator
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_validate_file_includes_parent_locator_for_diff_disk -- --nocapture`
    Expected: Exit 0，且断言 validate_file 在差分盘成功
    Evidence: .sisyphus/evidence/task-2-validate-file-parent-locator-happy.txt

  Scenario: Failure/edge case - invalid parent locator fails via validate_file
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_validation_parent_locator_invalid_returns_error -- --nocapture`
    Expected: 返回 InvalidMetadata 且断言命中
    Evidence: .sisyphus/evidence/task-2-validate-file-parent-locator-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): include parent locator check for differencing disks` | Files: `src/validation.rs`, `tests/integration_test.rs`

- [ ] 3. 修复 `IO::sector` 与字节边界语义潜在不一致

  **What to do**:
  - 在 `src/io_module.rs` 明确并实现边界策略：对尾部非整扇区场景保持一致语义（可选策略二选一并固定在测试中）：
    1) 严格模式：不可返回最后部分扇区（并在文档与测试显式声明）
    2) 兼容模式（推荐）：允许最后扇区定位，但 `Sector::read/write` 对越界部分执行受控行为（读取截断+零填充/写入拒绝）
  - 本计划默认采用**兼容模式**以与 `File::read` 字节边界能力一致。

  **Must NOT do**:
  - 不修改 public API 签名。
  - 不把此任务扩展为 IO 批量写实现（`write_sectors` 保持既有范围）。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及行为定义 + 边界测试设计。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [4,7] | Blocked By: []

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `src/io_module.rs:49-70` - `IO::sector` 当前范围判断。
  - Pattern: `src/io_module.rs:165-206` - `Sector::read/write` 当前长度约束。
  - Pattern: `src/file.rs:276-286` - `File::read` 字节边界语义。
  - Test: `tests/integration_test.rs:1546` - `test_io_sector_out_of_range_returns_none`（需评估回归影响）。
  - External: `docs/plan/API.md:939-974` - IO 数据面契约。

  **Acceptance Criteria** (agent-executable only):
  - [ ] 明确尾部边界行为并有自动化测试锁定。
  - [ ] `IO::sector` 与 `Sector::read/write` 在边界场景行为一致、可预测。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - aligned virtual disk sector addressing unchanged
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_io_sector_out_of_range_returns_none -- --nocapture`
    Expected: 既有对齐场景行为保持通过
    Evidence: .sisyphus/evidence/task-3-io-sector-boundary-happy.txt

  Scenario: Failure/edge case - tail partial-sector behavior locked by new test
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_io_sector_tail_partial_boundary_behavior -- --nocapture`
    Expected: 与选定策略一致（兼容模式下断言命中）
    Evidence: .sisyphus/evidence/task-3-io-sector-boundary-error.txt
  ```

  **Commit**: YES | Message: `fix(io): align sector boundary behavior with byte-level semantics` | Files: `src/io_module.rs`, `tests/integration_test.rs`

- [ ] 4. 补齐 validator 侧回归测试矩阵

  **What to do**:
  - 在 `tests/integration_test.rs` 增加三类回归：
    - RegionTable CRC 破坏检测
    - `validate_file` 差分盘 parent locator 覆盖
    - 非差分盘 `validate_file` 不误触发 parent locator 失败
  - 复用现有注入 helper，避免重复造轮子。

  **Must NOT do**:
  - 不删除已有测试。
  - 不通过放宽断言“制造通过”。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 组合注入 + 稳定断言。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: [7] | Blocked By: [1,2,3]

  **References** (executor has NO interview context - be exhaustive):
  - Test pattern: `tests/integration_test.rs:241-336` - metadata/log 注入 helper。
  - Test pattern: `tests/integration_test.rs:2459-2553` - required unknown region 注入与断言。
  - Test pattern: `tests/integration_test.rs:1877-2065` - parent locator/parent chain 断言风格。
  - API/Type: `src/validation.rs` 相关方法签名。

  **Acceptance Criteria** (agent-executable only):
  - [ ] 新增测试在本机稳定通过且命名清晰。
  - [ ] 新增测试不会引入随机失败（无时间/环境脆弱依赖）。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - validator regression suite pass
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs t12_validator -- --nocapture`
    Expected: 现有 t12 校验套件与新增用例共同通过
    Evidence: .sisyphus/evidence/task-4-validator-regression-happy.txt

  Scenario: Failure/edge case - intentionally corrupted region table crc
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_validate_region_table_detects_corrupted_crc -- --nocapture`
    Expected: 在被测路径中触发 checksum 相关错误断言
    Evidence: .sisyphus/evidence/task-4-validator-regression-error.txt
  ```

  **Commit**: YES | Message: `test(validation): add regression cases for gap closure` | Files: `tests/integration_test.rs`

- [ ] 5. CLI `check` 命令补齐 Parent Locator 检查项（差分盘条件）

  **What to do**:
  - 在 `vhdx-cli/src/commands/check.rs` 的 results 列表中，增加条件检查项：
    - 差分盘：执行 `validator.validate_parent_locator()` 并输出 `Parent Locator`
    - 非差分盘：不计为失败项，可选择标记 `N/A` 或跳过（保持摘要计数语义一致）
  - 保持现有 6 项输出格式风格与错误聚合逻辑。

  **Must NOT do**:
  - 不引入 parent chain I/O 检查。
  - 不修改 `--repair` 语义。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件行为补齐。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [6,7] | Blocked By: [2]

  **References** (executor has NO interview context - be exhaustive):
  - Pattern: `vhdx-cli/src/commands/check.rs:68-95` - 现有检查项列表。
  - Pattern: `vhdx-cli/src/commands/check.rs:97-147` - 统一输出与计数逻辑。
  - API/Type: `src/validation.rs:583-657` - parent locator 校验。

  **Acceptance Criteria** (agent-executable only):
  - [ ] 差分盘 `check` 输出中可见 Parent Locator 检查结果。
  - [ ] 非差分盘 `check` 不产生额外失败。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - differencing disk shows parent locator check
    Tool: Bash
    Steps: Run `cargo test -p vhdx-tool check_differencing_disk_includes_parent_locator_item -- --nocapture`
    Expected: stdout contains `Parent Locator` and command exits 0
    Evidence: .sisyphus/evidence/task-5-cli-parent-locator-happy.txt

  Scenario: Failure/edge case - invalid parent locator reported by check
    Tool: Bash
    Steps: Run `cargo test -p vhdx-tool check_invalid_parent_locator_reports_failure -- --nocapture`
    Expected: Parent Locator item reports failure and non-zero exit
    Evidence: .sisyphus/evidence/task-5-cli-parent-locator-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): include parent locator check in check command for differencing disks` | Files: `vhdx-cli/src/commands/check.rs`, `vhdx-cli/tests/cli_integration.rs`

- [ ] 6. 补齐 CLI 集成测试矩阵

  **What to do**:
  - 在 `vhdx-cli/tests/cli_integration.rs` 增加面向 `check` 的断言：
    - 差分盘场景输出包含 `Parent Locator`
    - 非差分盘场景不误报 parent locator 失败
  - 保证测试可在临时目录完全自举运行。

  **Must NOT do**:
  - 不依赖 `misc/` 固定文件。
  - 不使用脆弱字符串全量匹配（仅匹配关键片段）。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单文件测试扩展。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [7] | Blocked By: [5]

  **References** (executor has NO interview context - be exhaustive):
  - Test pattern: `vhdx-cli/tests/cli_integration.rs:13-46` - create helper。
  - Test pattern: `vhdx-cli/tests/cli_integration.rs` 中 check 子命令断言风格（同文件既有 check 测试区）。
  - Runtime: `vhdx-cli/src/commands/check.rs` 输出逻辑。

  **Acceptance Criteria** (agent-executable only):
  - [ ] 新增 CLI 测试在 Windows/PowerShell 下稳定通过。
  - [ ] 断言覆盖差分盘与非差分盘两类路径。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - CLI check differencing output
    Tool: Bash
    Steps: Run `cargo test -p vhdx-tool cli_check_differencing_parent_locator_output -- --nocapture`
    Expected: exit 0 and output contains Parent Locator check line
    Evidence: .sisyphus/evidence/task-6-cli-regression-happy.txt

  Scenario: Failure/edge case - CLI check invalid locator path
    Tool: Bash
    Steps: Run `cargo test -p vhdx-tool cli_check_invalid_parent_locator_fails -- --nocapture`
    Expected: command exits non-zero with clear Parent Locator failure
    Evidence: .sisyphus/evidence/task-6-cli-regression-error.txt
  ```

  **Commit**: YES | Message: `test(cli): add check command parent locator coverage` | Files: `vhdx-cli/tests/cli_integration.rs`

- [ ] 7. 全量回归收口与证据归档

  **What to do**:
  - 运行工作区完整测试与静态检查。
  - 汇总本计划所有 evidence 文件。
  - 确认无范围外文件改动。

  **Must NOT do**:
  - 不忽略失败测试。
  - 不通过删除测试达成通过。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 跨 crate 回归与收口。
  - Skills: `[]` - 无。
  - Omitted: `[]` - 无。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [Final Wave] | Blocked By: [1,2,3,4,5,6]

  **References** (executor has NO interview context - be exhaustive):
  - Command: `cargo test --workspace`
  - Command: `cargo clippy --workspace`
  - Plan: `.sisyphus/plans/ms-vhdx-gap-closure.md`

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --workspace` exit 0。
  - [ ] `cargo clippy --workspace` exit 0（无新增回归）。
  - [ ] evidence 文件齐全且可追溯到每个 task。

  **QA Scenarios** (MANDATORY - task incomplete without these):
  ```
  Scenario: Happy path - full workspace regression
    Tool: Bash
    Steps: Run `cargo test --workspace` then `cargo clippy --workspace`
    Expected: both exit 0
    Evidence: .sisyphus/evidence/task-7-full-regression-happy.txt

  Scenario: Failure/edge case - targeted gap suite isolation
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs test_validate_region_table_detects_corrupted_crc -- --nocapture`; then `cargo test -p vhdx-rs test_validate_file_includes_parent_locator_for_diff_disk -- --nocapture`; then `cargo test -p vhdx-rs test_io_sector_tail_partial_boundary_behavior -- --nocapture`; then `cargo test -p vhdx-tool check_differencing_disk_includes_parent_locator_item -- --nocapture`; then `cargo test -p vhdx-tool check_invalid_parent_locator_reports_failure -- --nocapture`; then `cargo test -p vhdx-tool cli_check_differencing_parent_locator_output -- --nocapture`; then `cargo test -p vhdx-tool cli_check_invalid_parent_locator_fails -- --nocapture`
    Expected: deterministic pass/fail aligned with assertions
    Evidence: .sisyphus/evidence/task-7-full-regression-error.txt
  ```

  **Commit**: YES | Message: `test(workspace): finalize ms-vhdx gap-closure regression` | Files: `src/validation.rs`, `src/io_module.rs`, `vhdx-cli/src/commands/check.rs`, `tests/integration_test.rs`, `vhdx-cli/tests/cli_integration.rs`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- 原则：每个 TODO 完成后原子提交，提交信息聚焦“为何修复这个 gap”。
- 建议提交序列：
  - `fix(validation): enforce region table crc and parent-locator coverage in validate_file`
  - `fix(io): align io sector addressing with virtual-disk boundary semantics`
  - `test(cli): cover parent-locator check output for differencing disks`
  - `test(workspace): add regression cases for plan-first gap closure`

## Success Criteria
- 三个真实 gap 均有代码修复与自动化证据。
- 新增测试在修复前可复现偏差、修复后稳定通过。
- CLI 与库层对差分盘校验语义一致且可观测。
