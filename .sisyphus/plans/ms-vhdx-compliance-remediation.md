# MS-VHDX Compliance Remediation Plan (Implementation vs Plan Baselines)

## TL;DR
> **Summary**: Remediate all confirmed behavior gaps against `misc/MS-VHDX.md` while preserving the already-aligned API surface from `docs/plan/API.md`. Prioritize data correctness first (log replay, BAT mapping/state correctness, differencing read semantics), then capability completeness (dynamic auto-allocation).
> **Deliverables**:
> - Correct dynamic/fixed BAT usage and encoding behavior
> - Spec-aligned log replay validity and sequencing
> - Differencing read correctness (sector bitmap + parent fallback)
> - Dynamic write auto-allocation path
> - Comprehensive automated regression tests
> **Effort**: Large
> **Parallel**: YES - 3 waves
> **Critical Path**: Task 4 → Task 7 → Task 8 → Task 12

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md` 和 `misc/MS-VHDX.md` 计划差别，以计划为准。
- 开始做出修复以上所有内容的计划。

### Interview Summary
- API 计划层面基本已对齐，且实现为超集；`OpenOptions/CreateOptions` 更偏文档树表示差异，不作为高优先缺陷。
- 规范行为层存在实质缺口：日志回放正确性、BAT 索引/状态一致性、差分读取语义、动态分配能力。
- 用户要求“修复以上所有内容”，按全量整改纳入范围。

### Metis Review (gaps addressed)
- 新增并确认高优先缺口：Dynamic 读路径 BAT 索引未按 chunk 交错换算（读写不一致）。
- 要求将“文档表示差异”与“规范行为偏差”分离，避免误报。
- 强化 guardrail：先修数据一致性风险，再扩展能力；每项必须有自动化可验证验收标准。

## Work Objectives
### Core Objective
- 使实现行为与 `misc/MS-VHDX.md` 的关键一致性要求对齐，并保持 `docs/plan/API.md` 的既有 API 可用性。

### Deliverables
- D1: Dynamic 读路径 BAT payload 索引修正（与写路径一致）
- D2: Fixed BAT 生成对 sector-bitmap 条目状态编码修正
- D3: 日志回放：CRC/GUID/entry 边界/active-sequence 有效性链路补齐
- D4: Data Descriptor 回放语义修正（leading/trailing）
- D5: 差分盘读取：PartiallyPresent + sector bitmap + parent fallback
- D6: Dynamic 未分配块自动分配与 BAT 持久化更新
- D7: 覆盖上述场景的自动化测试与回归门禁

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 通过。
- 新增针对以下缺口的测试全部通过：
  - Dynamic 读 BAT 索引跨 chunk 边界
  - Fixed BAT sector-bitmap 条目状态
  - Log replay CRC/GUID/active sequence
  - Differencing read bitmap + parent fallback
  - Dynamic auto-allocation
- `cargo clippy --workspace` 无新增相关警告回归。

### Must Have
- 所有修复均在现有模块内完成：`src/file.rs`, `src/sections/log.rs`, `src/sections/bat.rs`, `src/io_module.rs`, `src/validation.rs`, `tests/integration_test.rs`。
- 所有验证可由 agent 自动执行，不依赖人工检查。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `misc/`。
- 不引入新依赖。
- 不进行与本次缺口无关的大规模重构。
- 不用“跳过日志/强制容错”掩盖规范不一致。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + Rust `cargo test`
- QA policy: Every task includes happy + failure scenario
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.

Wave 1: correctness foundations (1,2,3,4,9)
- BAT index/state correctness, log validation primitives, differencing read primitives

Wave 2: behavior completion (5,6,7,10,11)
- log replay semantics/ordering, parent fallback, dynamic allocation

Wave 3: full regression hardening (8,12)
- replay state refresh + validation/test finalization

### Dependency Matrix (full, all tasks)
- 1 blocks: 12
- 2 blocks: 11,12
- 3 blocks: 12
- 4 blocks: 5,7,12
- 5 blocked by: 4; blocks: 7,12
- 6 blocked by: 4; blocks: 7,12
- 7 blocked by: 4,5,6; blocks: 8,12
- 8 blocked by: 7; blocks: 12
- 9 blocks: 10,12
- 10 blocked by: 9; blocks: 12
- 11 blocked by: 2; blocks: 12
- 12 blocked by: 1,2,3,4,5,6,7,8,9,10,11

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 5 tasks → `unspecified-high` (core logic), `quick` (targeted tests)
- Wave 2 → 5 tasks → `unspecified-high`, `deep` (log sequencing logic)
- Wave 3 → 2 tasks → `unspecified-high` (integration + hardening)

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 建立缺口回归测试基线与夹具扩展

  **What to do**:
  - 在 `tests/integration_test.rs` 增加可复用夹具：构造跨 chunk BAT 场景、log entry 可控损坏场景、差分盘父链测试场景。
  - 补充测试命名规范：每个缺口至少 1 个 fail-first 用例名。

  **Must NOT do**:
  - 不修改业务实现，仅增加测试基础设施与辅助函数。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 多场景夹具需要准确覆盖底层布局。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [12] | Blocked By: []

  **References**:
  - Pattern: `tests/integration_test.rs:121-260` - 现有注入式测试工具函数范式。
  - API/Type: `src/file.rs:438-507` - open/replay 入口。
  - Test: `src/file.rs` 内 `tests` 模块（create/open/write 场景）。

  **Acceptance Criteria**:
  - [ ] 新增夹具编译通过：`cargo test -p vhdx-rs --tests --no-run`
  - [ ] 测试文件无 lint 回归：`cargo clippy -p vhdx-rs --tests`

  **QA Scenarios**:
  ```
  Scenario: Happy path - fixture compiles and links
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs --tests --no-run`
    Expected: Command exits 0
    Evidence: .sisyphus/evidence/task-1-fixtures.txt

  Scenario: Failure/edge case - malformed fixture input
    Tool: Bash
    Steps: Run targeted malformed-fixture test name after implementation
    Expected: Test asserts expected error, not panic
    Evidence: .sisyphus/evidence/task-1-fixtures-error.txt
  ```

  **Commit**: YES | Message: `test(integration): add compliance fixture scaffolding` | Files: `tests/integration_test.rs`

- [ ] 2. 修复 Dynamic 读路径 BAT payload 索引换算

  **What to do**:
  - 在 `src/file.rs:274-277` 读路径引入与写路径一致的 `chunk_ratio` 与 `bat_payload_index`。
  - 将 `bat.entry(block_idx)` 改为 `bat.entry(bat_payload_index)`。
  - 保持 chunk 内分段读取逻辑不变。

  **Must NOT do**:
  - 不改变 Fixed 读路径。
  - 不混入 differencing 父链逻辑（由任务 9/10 处理）。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单点高风险修复，改动集中。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [11,12] | Blocked By: []

  **References**:
  - Pattern: `src/file.rs:356-371` - 写路径 payload BAT 索引换算。
  - API/Type: `src/sections/bat.rs:125-155` - chunk ratio/entry 计算。
  - Test: `tests/integration_test.rs`（扩展跨 chunk 读测试）。

  **Acceptance Criteria**:
  - [ ] 新增跨 chunk 读测试在修复后通过：`cargo test -p vhdx-rs dynamic_read_beyond_chunk_ratio`
  - [ ] 旧有读写测试不回归：`cargo test -p vhdx-rs test_write_and_read_at_offset`

  **QA Scenarios**:
  ```
  Scenario: Happy path - block crossing chunk boundary reads correct payload
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs dynamic_read_beyond_chunk_ratio -- --nocapture`
    Expected: Test passes and asserts non-zero expected payload
    Evidence: .sisyphus/evidence/task-2-dynamic-read-index.txt

  Scenario: Failure/edge case - BAT payload index out of range
    Tool: Bash
    Steps: Run targeted test with crafted oversized block index
    Expected: Returns InvalidParameter / safe zero-fill path per contract, no panic
    Evidence: .sisyphus/evidence/task-2-dynamic-read-index-error.txt
  ```

  **Commit**: YES | Message: `fix(file): align dynamic read BAT index with chunk interleaving` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 3. 修复 Fixed BAT 生成中的 sector-bitmap 条目编码

  **What to do**:
  - 在 `src/file.rs:create_bat_data` 中区分 payload 条目与 sector-bitmap 条目。
  - payload 条目保留 `FullyPresent + payload offset`；sector-bitmap 条目写 `NotPresent + 0 offset`。
  - 使用 `Bat::is_sector_bitmap_entry_index` 与 `calculate_chunk_ratio/calculate_payload_blocks`。

  **Must NOT do**:
  - 不改变 Dynamic BAT 全零初始化策略。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 需准确对齐 BAT 交错与布局公式。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [12] | Blocked By: []

  **References**:
  - Pattern: `src/file.rs:1049-1069` - 现有 BAT 创建流程。
  - API/Type: `src/sections/bat.rs:157-186` - bitmap 索引判定函数。
  - External: `misc/MS-VHDX.md §2.5.1` - fixed/dynamic bitmap entry state expectations。

  **Acceptance Criteria**:
  - [ ] 新增 fixed BAT 条目状态测试通过：`cargo test -p vhdx-rs fixed_bat_sector_bitmap_notpresent`
  - [ ] fixed 创建与读取主流程测试通过：`cargo test -p vhdx-rs test_create_and_read_fixed_disk`

  **QA Scenarios**:
  ```
  Scenario: Happy path - fixed BAT sector-bitmap entries are NotPresent
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs fixed_bat_sector_bitmap_notpresent -- --nocapture`
    Expected: All bitmap entries decoded as SectorBitmap(NotPresent)
    Evidence: .sisyphus/evidence/task-3-fixed-bat-encoding.txt

  Scenario: Failure/edge case - malformed BAT encoding detection
    Tool: Bash
    Steps: Run targeted test injecting wrong bitmap state
    Expected: Validator reports invalid state / mismatch
    Evidence: .sisyphus/evidence/task-3-fixed-bat-encoding-error.txt
  ```

  **Commit**: YES | Message: `fix(file): encode fixed BAT bitmap entries as not-present` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 4. 实现日志条目 CRC 与基础有效性验证（replay 前置）

  **What to do**:
  - 在 `src/sections/log.rs` 增加对 entry-level CRC-32C 校验（checksum 字段置零计算）。
  - replay 流程在处理 descriptor 前必须校验：signature、entry_length 边界、descriptor area 边界、CRC。
  - 与 `src/validation.rs:388-541` 逻辑对齐，避免重复但不依赖“调用者先 validate”。

  **Must NOT do**:
  - 不将校验失败吞掉并继续 replay。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及持久化恢复正确性。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [5,7,12] | Blocked By: []

  **References**:
  - Pattern: `src/sections.rs` 中 `crc32c_with_zero_field` - 可复用零字段 CRC 计算。
  - Pattern: `src/sections/header.rs` checksum verify 实现。
  - Test: `tests/integration_test.rs:121-198` - pending log 注入方式。
  - External: `misc/MS-VHDX.md §2.3.1.1`。

  **Acceptance Criteria**:
  - [ ] invalid checksum log 被拒绝：`cargo test -p vhdx-rs log_replay_rejects_invalid_checksum`
  - [ ] 正常 log replay 场景仍通过：`cargo test -p vhdx-rs log_replay_auto_applies_entry`

  **QA Scenarios**:
  ```
  Scenario: Happy path - valid log entry passes replay prechecks
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_auto_applies_entry -- --nocapture`
    Expected: Replay completes and target bytes updated
    Evidence: .sisyphus/evidence/task-4-log-crc.txt

  Scenario: Failure/edge case - checksum tampered
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_rejects_invalid_checksum -- --nocapture`
    Expected: Returns log corruption error; no write applied
    Evidence: .sisyphus/evidence/task-4-log-crc-error.txt
  ```

  **Commit**: YES | Message: `fix(log): enforce entry crc and bounds before replay` | Files: `src/sections/log.rs`, `tests/integration_test.rs`

- [ ] 5. 实现日志 GUID 一致性与 replay 条目过滤

  **What to do**:
  - 在 replay 入口传入 current header 的 `log_guid`，仅处理匹配条目。
  - `log_guid == nil` 直接视为无可回放日志。
  - mismatch 条目拒绝或跳过策略统一为“拒绝并报错”。

  **Must NOT do**:
  - 不对 GUID mismatch 静默成功。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 逻辑清晰但影响恢复正确性。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [7,12] | Blocked By: [4]

  **References**:
  - Pattern: `src/file.rs:662-694` - handle_log_replay 策略路径。
  - API/Type: `src/sections/log.rs:338-359` - `LogEntryHeader.log_guid`。
  - External: `misc/MS-VHDX.md §2.3.2/§2.3.3`。

  **Acceptance Criteria**:
  - [ ] GUID mismatch 用例失败并报预期错误：`cargo test -p vhdx-rs log_replay_rejects_mismatched_log_guid`

  **QA Scenarios**:
  ```
  Scenario: Happy path - matching guid replays
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_guid_match -- --nocapture`
    Expected: Replay applies data
    Evidence: .sisyphus/evidence/task-5-log-guid.txt

  Scenario: Failure/edge case - mismatched guid
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_rejects_mismatched_log_guid -- --nocapture`
    Expected: Replay aborted with corruption/invalid-log error
    Evidence: .sisyphus/evidence/task-5-log-guid-error.txt
  ```

  **Commit**: YES | Message: `fix(log): enforce log guid matching during replay` | Files: `src/file.rs`, `src/sections/log.rs`, `tests/integration_test.rs`

- [ ] 6. 修正 Data Descriptor 回放语义（leading/trailing）

  **What to do**:
  - 将 Data Descriptor 应用逻辑改为规范语义：使用 data sector 有效载荷并在目标扇区内合并前后片段，而非简单零填充 leading/trailing。
  - 保证对齐到 data sector 粒度，禁止越界写。

  **Must NOT do**:
  - 不继续采用“leading/trailing = 写零”的旧语义。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 语义细节易错且影响数据正确性。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [7,12] | Blocked By: [4]

  **References**:
  - Pattern: `src/sections/log.rs:199-220` - 现有错误处理位置。
  - API/Type: `src/sections/log.rs` `DataDescriptor` / `DataSector`。
  - External: `misc/MS-VHDX.md §2.3.1.2`。

  **Acceptance Criteria**:
  - [ ] 新语义测试通过：`cargo test -p vhdx-rs log_replay_data_descriptor_leading_trailing_semantics`

  **QA Scenarios**:
  ```
  Scenario: Happy path - descriptor applies expected in-sector byte ranges
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_data_descriptor_leading_trailing_semantics -- --nocapture`
    Expected: Replayed bytes exactly match expected merge result
    Evidence: .sisyphus/evidence/task-6-data-descriptor.txt

  Scenario: Failure/edge case - invalid leading+trailing size
    Tool: Bash
    Steps: Run targeted invalid descriptor test
    Expected: replay rejects entry with explicit corruption error
    Evidence: .sisyphus/evidence/task-6-data-descriptor-error.txt
  ```

  **Commit**: YES | Message: `fix(log): apply data descriptor bytes per spec semantics` | Files: `src/sections/log.rs`, `tests/integration_test.rs`

- [ ] 7. 实现 active sequence 选择与 replay 顺序约束

  **What to do**:
  - 在 `src/sections/log.rs` 实现 active sequence 解析：顺序连续、tail/descriptor/data 一致性、有效 entry 链提取。
  - replay 仅应用 active sequence 中条目。
  - 更新 `is_replay_required`，由“存在 entry”改为“存在有效 active sequence”。

  **Must NOT do**:
  - 不对全部 parse 成功条目盲目 replay。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 算法型恢复流程，需严谨边界处理。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [8,12] | Blocked By: [4,5,6]

  **References**:
  - Pattern: `src/validation.rs:396-539` - 序列与结构约束可参考。
  - API/Type: `src/sections/log.rs:80-144` - 当前 entries 扫描流程。
  - External: `misc/MS-VHDX.md §2.3.2/§2.3.3`。

  **Acceptance Criteria**:
  - [ ] 多 entry + 无效夹杂场景仅 replay active sequence：`cargo test -p vhdx-rs log_replay_active_sequence_only`

  **QA Scenarios**:
  ```
  Scenario: Happy path - active sequence chosen correctly
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_active_sequence_only -- --nocapture`
    Expected: Only expected entry chain applied
    Evidence: .sisyphus/evidence/task-7-active-sequence.txt

  Scenario: Failure/edge case - broken sequence continuity
    Tool: Bash
    Steps: Run targeted broken-sequence test
    Expected: replay aborts with sequence/corruption error
    Evidence: .sisyphus/evidence/task-7-active-sequence-error.txt
  ```

  **Commit**: YES | Message: `fix(log): replay only validated active sequence` | Files: `src/sections/log.rs`, `src/validation.rs`, `tests/integration_test.rs`

- [ ] 8. 完成 replay 后文件尺寸约束与 sections 刷新

  **What to do**:
  - 在 `src/file.rs` replay 流程中加入 `flushed_file_offset/last_file_offset` 约束：
    - 若文件长度小于 `flushed_file_offset`，返回错误。
    - replay 后文件长度至少扩展至 `last_file_offset`。
  - replay 完成后刷新/重建 `Sections`，避免 stale 缓存。

  **Must NOT do**:
  - 不保留 replay 后继续使用旧 sections 缓存。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 持久化状态一致性修复。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [12] | Blocked By: [7]

  **References**:
  - Pattern: `src/file.rs:766-814` - replay/apply header 路径。
  - External: `misc/MS-VHDX.md §2.3.3`。

  **Acceptance Criteria**:
  - [ ] 文件长度约束测试通过：`cargo test -p vhdx-rs log_replay_enforces_file_size_offsets`
  - [ ] replay 后 reopened/inline sections 读取一致：`cargo test -p vhdx-rs log_replay_refreshes_sections`

  **QA Scenarios**:
  ```
  Scenario: Happy path - replay expands file to last_file_offset
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs log_replay_enforces_file_size_offsets -- --nocapture`
    Expected: File length >= last_file_offset after replay
    Evidence: .sisyphus/evidence/task-8-replay-size-refresh.txt

  Scenario: Failure/edge case - file shorter than flushed_file_offset
    Tool: Bash
    Steps: Run targeted truncated-file replay test
    Expected: Open/replay fails with deterministic error
    Evidence: .sisyphus/evidence/task-8-replay-size-refresh-error.txt
  ```

  **Commit**: YES | Message: `fix(file): enforce replay offsets and refresh sections cache` | Files: `src/file.rs`, `tests/integration_test.rs`

- [ ] 9. 实现差分盘 PartiallyPresent 的 sector bitmap 判定读取

  **What to do**:
  - 在 Dynamic/Differencing 读路径中，当 payload state 为 `PartiallyPresent` 时，按 sector bitmap 决定每个扇区来源。
  - 新增 bitmap 读取辅助函数，定位对应 bitmap block 与 bit index。

  **Must NOT do**:
  - 不把 `PartiallyPresent` 等同 `FullyPresent` 全块读取。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 位图映射与块布局耦合高。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [10,12] | Blocked By: []

  **References**:
  - Pattern: `src/file.rs:253-303` - 当前 dynamic read 循环。
  - API/Type: `src/sections/bat.rs:157-186` - bitmap entry 判定。
  - External: `misc/MS-VHDX.md §2.5.1.1`。

  **Acceptance Criteria**:
  - [ ] `PartiallyPresent` 位图控制读取测试通过：`cargo test -p vhdx-rs differencing_read_partially_present_uses_bitmap`

  **QA Scenarios**:
  ```
  Scenario: Happy path - bitmap-marked sectors read from child payload
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs differencing_read_partially_present_uses_bitmap -- --nocapture`
    Expected: Only bitmap-present sectors come from child
    Evidence: .sisyphus/evidence/task-9-diff-bitmap-read.txt

  Scenario: Failure/edge case - bitmap block missing
    Tool: Bash
    Steps: Run targeted missing-bitmap test
    Expected: Returns controlled error or parent fallback per contract, no panic
    Evidence: .sisyphus/evidence/task-9-diff-bitmap-read-error.txt
  ```

  **Commit**: YES | Message: `fix(file): honor sector bitmap for partially-present differencing reads` | Files: `src/file.rs`, `src/io_module.rs`, `tests/integration_test.rs`

- [ ] 10. 实现差分盘父链回退读取

  **What to do**:
  - 对 `NotPresent/Zero/Unmapped/bitmap-miss` 扇区，按 parent locator 解析父盘并回退读取。
  - 保证 parent path 解析顺序与现有 `resolve_parent_path` 一致。

  **Must NOT do**:
  - 不在未找到父盘时静默返回零（差分盘语义下应报错或按明确策略处理）。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 跨文件链式读取与错误语义要求严格。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [12] | Blocked By: [9]

  **References**:
  - Pattern: `src/sections/metadata.rs` `ParentLocator::resolve_parent_path`。
  - API/Type: `src/validation.rs:624-709` - parent chain 校验路径。
  - External: `misc/MS-VHDX.md §1.3, §2.5.1.1`。

  **Acceptance Criteria**:
  - [ ] parent fallback 读取正确：`cargo test -p vhdx-rs differencing_read_falls_back_to_parent`
  - [ ] parent 缺失时报错：`cargo test -p vhdx-rs differencing_read_parent_missing_errors`

  **QA Scenarios**:
  ```
  Scenario: Happy path - child miss reads from parent
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs differencing_read_falls_back_to_parent -- --nocapture`
    Expected: Returned bytes equal parent content for missing sectors
    Evidence: .sisyphus/evidence/task-10-parent-fallback.txt

  Scenario: Failure/edge case - broken parent chain path
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs differencing_read_parent_missing_errors -- --nocapture`
    Expected: Returns ParentNotFound/ParentMismatch style error
    Evidence: .sisyphus/evidence/task-10-parent-fallback-error.txt
  ```

  **Commit**: YES | Message: `feat(file): add differencing parent fallback read path` | Files: `src/file.rs`, `src/sections/metadata.rs`, `tests/integration_test.rs`

- [ ] 11. 实现 Dynamic 未分配块自动分配与 BAT 更新

  **What to do**:
  - 在 `write_dynamic` 中为 `NotPresent/Zero/Unmapped` 实现自动分配：
    - 计算新 payload 物理偏移（文件尾 1MiB 对齐策略保持一致）。
    - 写入块数据。
    - 更新 BAT 对应 payload 条目状态与偏移并持久化。
  - 处理跨块写入场景（循环分段）。

  **Must NOT do**:
  - 不破坏已有已分配块写入路径。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 涉及文件增长与元数据一致性。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [12] | Blocked By: [2]

  **References**:
  - Pattern: `src/file.rs:350-414` - 现有 dynamic write 主流程。
  - API/Type: `src/sections/bat.rs` `BatEntry::raw` 编码。
  - External: `misc/MS-VHDX.md §1.3, §2.5.1.1`。

  **Acceptance Criteria**:
  - [ ] 新分配写入场景通过：`cargo test -p vhdx-rs dynamic_write_auto_allocates_payload_block`
  - [ ] 原先“失败”行为测试更新为新语义并通过。

  **QA Scenarios**:
  ```
  Scenario: Happy path - write to unallocated block auto-allocates and persists
    Tool: Bash
    Steps: Run `cargo test -p vhdx-rs dynamic_write_auto_allocates_payload_block -- --nocapture`
    Expected: Subsequent read returns written bytes and BAT entry becomes FullyPresent
    Evidence: .sisyphus/evidence/task-11-dynamic-auto-allocation.txt

  Scenario: Failure/edge case - allocation beyond virtual bounds
    Tool: Bash
    Steps: Run targeted out-of-bounds write test
    Expected: Returns InvalidParameter, no partial BAT corruption
    Evidence: .sisyphus/evidence/task-11-dynamic-auto-allocation-error.txt
  ```

  **Commit**: YES | Message: `feat(file): support dynamic payload auto-allocation on write` | Files: `src/file.rs`, `src/sections/bat.rs`, `tests/integration_test.rs`

- [ ] 12. 全量验证收口（validator + integration regression）

  **What to do**:
  - 对 `src/validation.rs` 增补与新行为一致的校验点（log entry CRC/GUID/sequence，differencing 必要约束）。
  - 汇总新增测试并执行工作区回归。
  - 确保 `docs/plan/API.md` 层面不回退（API 可用性保持）。

  **Must NOT do**:
  - 不以放宽 validator 规则“换取通过”。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 收口验证横跨多模块。
  - Skills: `[]` - 无可用技能。
  - Omitted: `[]` - 无可省略技能。

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [] | Blocked By: [1,2,3,4,5,6,7,8,9,10,11]

  **References**:
  - Pattern: `src/validation.rs:388-541` - 现有 log 校验骨架。
  - Test: `tests/integration_test.rs` - 集成测试汇总入口。

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` 通过。
  - [ ] `cargo clippy --workspace` 通过（无新增相关告警回归）。

  **QA Scenarios**:
  ```
  Scenario: Happy path - full workspace regression pass
    Tool: Bash
    Steps: Run `cargo test --workspace` then `cargo clippy --workspace`
    Expected: Both commands exit 0
    Evidence: .sisyphus/evidence/task-12-full-regression.txt

  Scenario: Failure/edge case - targeted corruption regressions
    Tool: Bash
    Steps: Run all newly added corruption tests with `-- --nocapture`
    Expected: All corruption paths fail safely with deterministic errors
    Evidence: .sisyphus/evidence/task-12-full-regression-error.txt
  ```

  **Commit**: YES | Message: `test(validation): finalize ms-vhdx compliance regression suite` | Files: `src/validation.rs`, `tests/integration_test.rs`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- 原则：每个 TODO 完成后原子提交，信息聚焦“为何修复”。
- 提交类型建议：`fix(file)`, `fix(log)`, `feat(file)`, `test(integration)`, `test(validation)`。
- 禁止将多个高风险修复混成单提交。

## Success Criteria
- 所有 P0/P1 缺口具备通过的自动化用例。
- 日志回放行为满足：仅处理有效 active sequence，CRC/GUID/边界约束完整。
- Dynamic/Differencing 数据读取路径在跨 chunk、bitmap、parent fallback 下结果正确。
- Dynamic 未分配写入自动分配生效且 BAT 持久化一致。
