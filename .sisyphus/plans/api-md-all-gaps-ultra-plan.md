# API.md 全量差异修复超长执行计划（Plan-First）

## TL;DR
> **Summary**: 以 `docs/plan/API.md` 为唯一裁决源，修复当前实现中全部已确认偏差，并用 `misc/MS-VHDX.md` 做条款交叉校验，最终通过四路并行终验（F1-F4）。
> **Deliverables**:
> - strict 模式补齐 required unknown region 拒绝语义
> - `LogReplayPolicy::Auto` 只读行为与计划语义对齐
> - differencing 创建流程写入完整 parent locator payload
> - `SpecValidator` 深化（header/region/BAT/metadata/log）
> - `validate_required_metadata_items` 从硬编码升级为 flag 扫描
> - CLI 契约对齐（`--type` / `--disk-type` / `--force`）
> - 规范追踪矩阵 + 全回归 + 原始证据闭环
> **Effort**: XL
> **Parallel**: YES - 5 waves
> **Critical Path**: T1(决策冻结) → T2/T3/T4 → T5/T6/T7 → T8/T9/T10 → T11/T12 → Final Verification

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md` 和 `misc/MS-VHDX.md` 是否还有差别，以计划为准。
- 做出解决以上所有问题的超长规划。

### Interview Summary
- 用户明确要求“计划优先”（`docs/plan/API.md` 为准）。
- 已完成差异审计，确认存在 6 组主要 gap（strict/replay/parent locator/validator/required metadata/CLI）。
- 已确认对齐项：`Log::entry(index)`、`parent_linkage2` 验证路径、`section::StandardItems` 命名空间。

### Metis Review (gaps addressed)
- 补入“决策冻结页”作为所有实现任务前置，避免行为语义漂移。
- 强制加入 Gap→File→Plan Clause→Spec Clause→Test 的追踪矩阵任务。
- 将 CLI 兼容策略显式任务化，防止破坏旧脚本。
- 限定范围：只修复本轮已确认 gap，禁止顺手重构。

### Decision Freeze (locked defaults)
- strict 默认冻结：`strict=true` 时，遇到 required unknown **region 或 metadata** 立即失败；`strict=false` 允许继续打开。
- Auto 只读默认冻结：`LogReplayPolicy::Auto` 在只读下执行**内存回放**（不写盘），行为与 `InMemoryOnReadOnly` 一致。
- CLI 参数兼容冻结：新增 `--type` 为主路径，同时保留 `--disk-type` 作为兼容别名（当前版本不移除）。
- `--force` 语义冻结：仅用于覆盖“目标文件已存在”场景；不绕过任何规范/参数校验（如 parent 不存在仍失败）。

## Work Objectives
### Core Objective
将当前实现与 `docs/plan/API.md` 的已确认偏差全部归零，并保证与 `misc/MS-VHDX.md` 的对应条款不冲突（发生冲突时以计划定义为裁决基准并记录理由）。

### Deliverables
- 代码修复：
  - `src/file.rs`
  - `src/validation.rs`
  - `src/sections/log.rs`
  - `src/sections/bat.rs`（仅在 validator 侧约束映射所需）
  - `vhdx-cli/src/cli.rs`
- 测试与证据：
  - `tests/integration_test.rs`
  - `tests/api_surface_smoke.rs`
  - CLI 集成测试（`vhdx-cli/tests/cli_integration.rs`）
  - `.sisyphus/evidence/api-md-all-gaps-ultra/*.txt`
- 文档产物：
  - 追踪矩阵（计划内章节）
  - 决策冻结页（计划内章节）

### Definition of Done (verifiable conditions with commands)
- `cargo test -p vhdx-rs --test api_surface_smoke` 通过。
- `cargo test -p vhdx-rs --test integration_test` 通过（含新增差异断言）。
- `cargo test -p vhdx-tool --test cli_integration` 通过（CLI 契约对齐）。
- `cargo test --workspace` 通过。
- `cargo build -p vhdx-tool` 通过。
- `cargo clippy --workspace` 不引入新增阻断错误。

### Must Have
- strict=true 必须拒绝 required unknown region / metadata。
- Auto 策略在只读场景必须符合冻结决策且可测试。
- differencing 创建必须写出可被 `ParentLocator` 解析的 payload（含 key/value）。
- validator 各分项不再仅“可读性检查”，需执行计划定义的核心一致性断言。
- required metadata 校验按 flags/known 规则执行，不再仅硬编码项存在性。
- CLI 参数行为与计划契约对齐并处理兼容迁移。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不修改 `docs/plan/API.md`。
- 不修改 `misc/`。
- 不做与本次 gap 无关的重构/依赖变更。
- 不回退已对齐能力（`Log::entry` / `parent_linkage2` / `StandardItems`）。
- 不引入“手工验证”验收项（必须 agent 可执行）。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after（Rust 原生测试 + CLI 集成测试）
- QA policy: 每个任务至少包含 1 个 happy + 1 个 failure/edge 场景
- Evidence: `.sisyphus/evidence/api-md-all-gaps-ultra/task-{N}-{slug}.txt`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Shared dependencies extracted to Wave 1.

Wave 1: 语义冻结与追踪基线
- T1 决策冻结页（strict/auto/CLI/force）
- T2 追踪矩阵（Plan+Spec+Code+Test）

Wave 2: open/create 核心行为
- T3 strict required unknown region 补齐
- T4 Auto 只读回放语义对齐
- T5 differencing parent locator payload 写入

Wave 3: validator 深化
- T6 validate_header/region_table 深化
- T7 validate_bat/validate_log 深化
- T8 validate_required_metadata_items flag 扫描升级

Wave 4: CLI 契约与兼容
- T9 CLI `--type` / `--disk-type` 兼容策略落地
- T10 CLI `--force` 语义落地

Wave 5: 回归与证据闭环
- T11 全量回归执行与失败路径验证
- T12 证据归档与质量闸门

### Dependency Matrix (full, all tasks)
- T1: blocked by none; blocks T3,T4,T9,T10
- T2: blocked by none; blocks T3,T4,T5,T6,T7,T8,T11
- T3: blocked by T1,T2; blocks T11
- T4: blocked by T1,T2; blocks T11
- T5: blocked by T2; blocks T11
- T6: blocked by T2; blocks T11
- T7: blocked by T2; blocks T11
- T8: blocked by T2; blocks T11
- T9: blocked by T1; blocks T11
- T10: blocked by T1; blocks T11
- T11: blocked by T3,T4,T5,T6,T7,T8,T9,T10; blocks T12,F1-F4
- T12: blocked by T11; blocks F1-F4
- F1-F4: blocked by T1-T12

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 2 tasks → deep
- Wave 2 → 3 tasks → deep/unspecified-high
- Wave 3 → 3 tasks → deep
- Wave 4 → 2 tasks → quick/unspecified-high
- Wave 5 → 2 tasks → unspecified-high
- Final verification → 4 tasks → oracle + unspecified-high + deep

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 决策冻结页（行为语义一次性锁定）

  **What to do**:
  - 将本计划中已锁定的 4 项语义落盘为执行清单：
    1) strict=true 对 required unknown region/metadata 硬失败；
    2) Auto 只读=内存回放且不写盘；
    3) `--type` 主路径 + `--disk-type` 兼容别名；
    4) `--force` 仅覆盖“目标文件已存在”。
  - 在计划内部写明默认值与可回滚策略。

  **Must NOT do**:
  - 不直接改代码。
  - 不引入新增业务需求。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 行为契约级决策影响全链路。
  - Skills: `[]` - 不需外部技能。
  - Omitted: `quick` - 风险高，不适合快改。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [3,4,9,10] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:335-336,353-365,265-269`
  - Pattern: `src/file.rs:565-568,1003-1018`
  - Pattern: `vhdx-cli/src/cli.rs:51-59`

  **Acceptance Criteria**:
  - [ ] 决策表以 markdown 固化，含默认值与异常处理。
  - [ ] 任何后续任务不再出现“语义待定”占位，且实现任务直接引用上述锁定语义。

  **QA Scenarios**:
  ```
  Scenario: 决策表完整性校验
    Tool: Bash
    Steps: grep 决策表中 strict/auto/cli/force 四项关键词
    Expected: 四项均存在且每项含“默认行为+例外”
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-1-decision-freeze.txt

  Scenario: 决策缺失拦截
    Tool: Bash
    Steps: 执行计划 lint 脚本（检查 TODO 是否引用冻结语义）
    Expected: 缺失时返回非 0；补齐后返回 0
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-1-decision-freeze-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: `n/a`

- [ ] 2. 建立 Gap→Plan/Spec→Code→Test 追踪矩阵

  **What to do**:
  - 为每个 gap 建立四元映射：Plan Clause、Spec Clause、代码文件/行、测试用例。
  - 标注“计划优先”冲突点（如 Auto 只读语义）。

  **Must NOT do**:
  - 不把 spec-only（计划未要求）项目混入本轮必须修复。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 需要条款级可追踪性。
  - Skills: `[]`.
  - Omitted: `writing` - 需要技术核对而非纯文案。

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [3,4,5,6,7,8,11] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:446-475,625-648`
  - External: `misc/MS-VHDX.md §2.2.3.2, §2.3.1, §2.5.1, §2.6.2.6`
  - Pattern: `src/file.rs:465-483,553-591,1088-1222`
  - Pattern: `src/validation.rs:68-143,145-309`

  **Acceptance Criteria**:
  - [ ] 6 个 gap 均有完整四元映射。
  - [ ] 每个 gap 至少绑定 1 个失败路径测试。

  **QA Scenarios**:
  ```
  Scenario: 追踪矩阵完整
    Tool: Bash
    Steps: 检查矩阵行数是否 >= 6 且每行包含 Plan/Spec/Code/Test 四列
    Expected: 条件全部满足
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-2-traceability.txt

  Scenario: 漏映射拦截
    Tool: Bash
    Steps: 刻意移除一条映射再运行检查
    Expected: 校验失败并指出缺失 gap
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-2-traceability-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: `n/a`

- [ ] 3. strict 模式补齐 required unknown region 拒绝

  **What to do**:
  - 在 open 流程中对 current region table 执行 unknown required region 检查。
  - 与现有 unknown required metadata 检查统一错误风格。
  - strict=false 时允许继续打开。

  **Must NOT do**:
  - 不改 OpenOptions API 形状。
  - 不影响已知 BAT/Metadata 提取路径。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: open 栈核心行为变更。
  - Skills: `[]`.
  - Omitted: `quick` - 风险高。

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [11] | Blocked By: [1,2]

  **References**:
  - Pattern: `docs/plan/API.md:335-336,449-450`
  - Pattern: `src/file.rs:465-483,496-551,1003-1018`
  - Pattern: `src/sections/header.rs:560-618`

  **Acceptance Criteria**:
  - [ ] strict=true + unknown required region → 打开失败（确定错误类型）。
  - [ ] strict=false + 同样样本 → 打开成功。
  - [ ] 现有 strict metadata 测试不回归。

  **QA Scenarios**:
  ```
  Scenario: strict=true 拒绝 unknown required region
    Tool: Bash
    Steps: 运行定向集成测试，构造含 unknown required region 的样本
    Expected: 返回预期 Error，测试通过
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-3-strict-region.txt

  Scenario: strict=false 放行同样样本
    Tool: Bash
    Steps: 同样样本使用 strict(false) 打开
    Expected: finish() 成功，后续结构读取可执行
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-3-strict-region-error.txt
  ```

  **Commit**: YES | Message: `fix(file): enforce strict required-unknown region rejection` | Files: `src/file.rs`, tests

- [ ] 4. `LogReplayPolicy::Auto` 只读语义对齐

  **What to do**:
  - 按冻结决策实现只读场景下 Auto 语义（默认：内存回放，不写盘）。
  - 与 `InMemoryOnReadOnly` 去重：确保可观察行为一致或有明确差异说明。

  **Must NOT do**:
  - 不在只读句柄上执行落盘写入。
  - 不破坏 `Require` 与 `ReadOnlyNoReplay` 既有行为。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 日志策略语义影响读取一致性。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [11] | Blocked By: [1,2]

  **References**:
  - Pattern: `docs/plan/API.md:353-365`
  - Pattern: `src/file.rs:553-591,593-687`
  - Pattern: `src/sections/log.rs:164-237`

  **Acceptance Criteria**:
  - [ ] 只读 + Auto + pending log 路径符合冻结决策。
  - [ ] 可区分 Auto 与 ReadOnlyNoReplay。
  - [ ] replay 相关回归测试通过。

  **QA Scenarios**:
  ```
  Scenario: 只读 Auto 触发预期回放语义
    Tool: Bash
    Steps: 运行含 pending log 的集成测试，使用 log_replay(Auto)
    Expected: 读取结果与决策一致，且无磁盘写回
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-4-auto-readonly.txt

  Scenario: ReadOnlyNoReplay 保持不回放
    Tool: Bash
    Steps: 同样样本改用 ReadOnlyNoReplay
    Expected: 与 Auto 行为差异可观测且符合计划
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-4-auto-readonly-error.txt
  ```

  **Commit**: YES | Message: `fix(file): align auto replay semantics for read-only open` | Files: `src/file.rs`, tests

- [ ] 5. 差分创建写入 Parent Locator payload

  **What to do**:
  - 将 `CreateOptions::parent_path` 信息传递至 metadata 构造函数。
  - 写入符合解析预期的 Parent Locator header + key/value entries + UTF-16LE data。
  - 保持 `resolve_parent_path` 顺序兼容：relative → volume → absolute。

  **Must NOT do**:
  - 不修改 `misc/`。
  - 不破坏非差分盘创建路径。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: create 路径与元数据布局耦合。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [11] | Blocked By: [2]

  **References**:
  - Pattern: `docs/plan/API.md:409-417,466-475`
  - Pattern: `src/file.rs:1088-1119,1129-1222`
  - Pattern: `src/sections/metadata.rs`（ParentLocator 解析行为）
  - External: `misc/MS-VHDX.md §2.6.2.6`

  **Acceptance Criteria**:
  - [ ] 创建差分盘后 `items.parent_locator().is_some()`。
  - [ ] 必需键 `parent_linkage` 存在且可解析。
  - [ ] 至少一个路径键存在并可被 resolve。

  **QA Scenarios**:
  ```
  Scenario: 差分盘 parent locator 实体可读
    Tool: Bash
    Steps: 创建 parent+child，读取 child metadata parent_locator
    Expected: header/entries/key_value_data 均有效，路径可解析
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-5-parent-locator-payload.txt

  Scenario: 缺失父盘路径创建失败
    Tool: Bash
    Steps: parent_path 指向不存在文件执行 create.finish
    Expected: 返回 ParentNotFound
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-5-parent-locator-payload-error.txt
  ```

  **Commit**: YES | Message: `fix(create): materialize parent locator payload for differencing disks` | Files: `src/file.rs`, tests

- [ ] 6. `validate_header` / `validate_region_table` 深化

  **What to do**:
  - 增加 header signature/version/log alignment/checksum 等核心断言。
  - region table 增加 `regi`、checksum、entry_count、required unknown region 规则断言。

  **Must NOT do**:
  - 不引入与计划无关的格式扩展。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 规范条款密集。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: [11] | Blocked By: [2]

  **References**:
  - Pattern: `docs/plan/API.md:446-450`
  - Pattern: `src/validation.rs:68-87`
  - Pattern: `src/sections/header.rs:320-543,545-618`
  - External: `misc/MS-VHDX.md §2.2.2, §2.2.3`

  **Acceptance Criteria**:
  - [ ] header/region 异常样本触发对应错误。
  - [ ] 正常样本通过验证。

  **QA Scenarios**:
  ```
  Scenario: header/region 正常样本通过
    Tool: Bash
    Steps: 定向测试 validate_header + validate_region_table
    Expected: 返回 Ok(())
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-6-header-region-validator.txt

  Scenario: checksum/required unknown region 异常拦截
    Tool: Bash
    Steps: 注入异常样本运行同一验证
    Expected: 返回预期错误类型
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-6-header-region-validator-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): deepen header and region table validation` | Files: `src/validation.rs`, tests

- [ ] 7. `validate_bat` / `validate_log` 深化

  **What to do**:
  - `validate_bat`: 校验状态合法性 + 与磁盘类型匹配（尤其差分/位图关系）。
  - `validate_log`: entry/descriptor/data sector/active sequence/replay 前置校验。

  **Must NOT do**:
  - 不把 replay 执行与 validate 行为耦合。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: BAT+Log 规则复杂且易回归。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: [11] | Blocked By: [2]

  **References**:
  - Pattern: `docs/plan/API.md:452-454,463-465,625-648`
  - Pattern: `src/validation.rs:89-99,139-143`
  - Pattern: `src/sections/bat.rs:157-186,267-290`
  - Pattern: `src/sections/log.rs:164-237`
  - External: `misc/MS-VHDX.md §2.3.1, §2.5.1`

  **Acceptance Criteria**:
  - [ ] bat/log 验证不再是“可读即通过”。
  - [ ] 非法状态与序列异常可被拦截。

  **QA Scenarios**:
  ```
  Scenario: bat/log 合法样本通过
    Tool: Bash
    Steps: 运行 validator 定向测试 validate_bat + validate_log
    Expected: 全部通过
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-7-bat-log-validator.txt

  Scenario: 非法 BAT 状态或日志异常被拒绝
    Tool: Bash
    Steps: 运行异常样本测试
    Expected: 返回 InvalidBlockState 或 LogEntryCorrupted 等预期错误
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-7-bat-log-validator-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): implement bat and log semantic checks` | Files: `src/validation.rs`, tests

- [ ] 8. `validate_required_metadata_items` 升级为 flags 扫描

  **What to do**:
  - 扫描 metadata table entries flags 中 is_required。
  - 对 required 且未知/缺失项按计划返回错误。
  - 保留对核心已知项缺失的清晰错误信息。

  **Must NOT do**:
  - 不仅做“5 项固定字段存在性”检查。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 规则细节来自计划+表项语义。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: [11] | Blocked By: [2]

  **References**:
  - Pattern: `docs/plan/API.md:458-462`
  - Pattern: `src/validation.rs:101-136`
  - Pattern: `src/sections/metadata.rs`（EntryFlags）

  **Acceptance Criteria**:
  - [ ] required unknown 项被检测并报错。
  - [ ] required known 缺失仍报错。

  **QA Scenarios**:
  ```
  Scenario: required unknown metadata 被拦截
    Tool: Bash
    Steps: 构造 required unknown 项样本并运行 validate_required_metadata_items
    Expected: 返回 InvalidMetadata
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-8-required-metadata-flags.txt

  Scenario: required known 缺失被拦截
    Tool: Bash
    Steps: 构造缺失 file_parameters 或 virtual_disk_size 样本
    Expected: 返回缺失项错误
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-8-required-metadata-flags-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): enforce required metadata via table flags` | Files: `src/validation.rs`, tests

- [ ] 9. CLI `--type` / `--disk-type` 契约对齐与迁移

  **What to do**:
  - 落地兼容策略：保留 `--disk-type`，新增 `--type`（推荐主路径）并给出迁移提示。
  - 更新 CLI 测试覆盖两条参数路径。

  **Must NOT do**:
  - 不直接移除旧参数导致脚本破坏（除非冻结决策要求）。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: CLI 兼容细节+测试联动。
  - Skills: `[]`.
  - Omitted: `deep`.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: [11] | Blocked By: [1]

  **References**:
  - Pattern: `docs/plan/API.md:265-269`
  - Pattern: `vhdx-cli/src/cli.rs:51-59`
  - Test: `vhdx-cli/tests/cli_integration.rs`

  **Acceptance Criteria**:
  - [ ] `--type` 可用并通过测试。
  - [ ] `--disk-type` 仍可用（兼容期）并有可测迁移提示（如启用）。

  **QA Scenarios**:
  ```
  Scenario: --type 路径可用
    Tool: Bash
    Steps: 运行 vhdx-tool create ... --type dynamic
    Expected: 命令成功，创建文件成功
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-9-cli-type-alias.txt

  Scenario: --disk-type 兼容路径可用
    Tool: Bash
    Steps: 运行 vhdx-tool create ... --disk-type dynamic
    Expected: 命令成功（如有弃用提示则命中断言）
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-9-cli-type-alias-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): align disk type option with plan while preserving compatibility` | Files: `vhdx-cli/src/cli.rs`, tests

- [ ] 10. CLI `--force` 语义实现

  **What to do**:
  - 在 create 命令实现 `--force`，覆盖“目标文件已存在”场景。
  - 与无 `--force` 的失败语义形成对照测试。

  **Must NOT do**:
  - 不扩展为忽略规范校验。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: CLI 选项增量改动。
  - Skills: `[]`.
  - Omitted: `deep`.

  **Parallelization**: Can Parallel: YES | Wave 4 | Blocks: [11] | Blocked By: [1]

  **References**:
  - Pattern: `docs/plan/API.md:268`
  - Pattern: `vhdx-cli/src/cli.rs`（Create 子命令）
  - Test: `vhdx-cli/tests/cli_integration.rs`

  **Acceptance Criteria**:
  - [ ] 无 `--force` 且文件存在时失败。
  - [ ] 有 `--force` 时可覆盖创建成功。

  **QA Scenarios**:
  ```
  Scenario: --force 成功覆盖
    Tool: Bash
    Steps: 先创建同名文件，再执行 create --force
    Expected: exit 0，目标文件为有效 vhdx
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-10-cli-force.txt

  Scenario: 无 --force 拒绝覆盖
    Tool: Bash
    Steps: 同样前置文件存在，执行 create 不带 --force
    Expected: exit 非 0，报 file exists 相关错误
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-10-cli-force-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): add force overwrite behavior for create` | Files: `vhdx-cli/src/cli.rs`, commands/tests

- [ ] 11. 全量回归与失败路径验证

  **What to do**:
  - 运行库/CLI 全回归，包含新增定向失败路径。
  - 修复本轮改动引入的非预期回归。

  **Must NOT do**:
  - 不通过删除测试规避失败。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 跨模块回归编排。
  - Skills: `[]`.
  - Omitted: `quick`.

  **Parallelization**: Can Parallel: NO | Wave 5 | Blocks: [12,F1,F2,F3,F4] | Blocked By: [3,4,5,6,7,8,9,10]

  **References**:
  - Test: `tests/integration_test.rs`
  - Test: `tests/api_surface_smoke.rs`
  - Test: `vhdx-cli/tests/cli_integration.rs`

  **Acceptance Criteria**:
  - [ ] `cargo test -p vhdx-rs --test api_surface_smoke` 通过。
  - [ ] `cargo test -p vhdx-rs --test integration_test` 通过。
  - [ ] `cargo test -p vhdx-tool --test cli_integration` 通过。
  - [ ] `cargo test --workspace` 通过。
  - [ ] `cargo build -p vhdx-tool` 通过。

  **QA Scenarios**:
  ```
  Scenario: 全回归通过
    Tool: Bash
    Steps: 按顺序执行 smoke/integration/cli/workspace/build
    Expected: 全部 exit code 0
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-11-full-regression.txt

  Scenario: 失败路径断言有效
    Tool: Bash
    Steps: 运行新增 failure case 测试过滤器
    Expected: 测试内断言命中预期错误类型/消息
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-11-full-regression-error.txt
  ```

  **Commit**: YES | Message: `test(parity): verify full regression for plan-first gap closure` | Files: tests

- [ ] 12. 证据归档与质量闸门

  **What to do**:
  - 将 T1-T11 关键命令原始输出落盘到证据目录。
  - 执行证据完整性检查（非空、含命令轨迹、含 exit code）。

  **Must NOT do**:
  - 不提交摘要化证据替代原始输出。

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: 工程化交付闭环。
  - Skills: `[]`.
  - Omitted: `writing`.

  **Parallelization**: Can Parallel: NO | Wave 5 | Blocks: [F1,F2,F3,F4] | Blocked By: [11]

  **References**:
  - Pattern: `.sisyphus/evidence/`（既有命名风格）
  - Pattern: `docs/plan/API.md`（验收命令）

  **Acceptance Criteria**:
  - [ ] 所有任务证据文件存在且非空。
  - [ ] 每个证据文件包含命令与结果痕迹。

  **QA Scenarios**:
  ```
  Scenario: 证据完整性通过
    Tool: Bash
    Steps: 执行证据校验脚本（大小+关键字段）
    Expected: 通过且报告缺失项为 0
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-12-evidence-gate.txt

  Scenario: 空证据拦截
    Tool: Bash
    Steps: 构造空文件并执行同一校验
    Expected: 校验失败并指向空文件
    Evidence: .sisyphus/evidence/api-md-all-gaps-ultra/task-12-evidence-gate-error.txt
  ```

  **Commit**: YES | Message: `chore(evidence): package raw traces for all parity tasks` | Files: `.sisyphus/evidence/api-md-all-gaps-ultra/*`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- 每个任务独立原子提交（T3-T12），便于回滚和审计。
- T1/T2 为计划内部产物，不单独提交代码。
- 提交前必须完成对应 task 的 happy+failure 测试。

## Success Criteria
- 6 个已确认 gap 全部消除，且不破坏已对齐能力。
- 实现与 `docs/plan/API.md` 对齐；`misc/MS-VHDX.md` 无新增冲突。
- F1-F4 全部 APPROVE，并获得用户显式“okay”。
