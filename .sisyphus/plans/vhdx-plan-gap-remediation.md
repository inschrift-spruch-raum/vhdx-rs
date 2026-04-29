# vhdx-rs Plan/API 对齐执行计划

## TL;DR
> **Summary**: 以 `docs/plan/API.md` 为唯一验收基线，对当前实现进行“计划一致性”收敛：先处理计划阻断项，再完成文档对齐，并把规范增强项隔离为后续里程碑。
> **Deliverables**:
> - `docs/API.md` 与 `docs/plan/API.md` 对齐结果
> - 计划阻断项修复（若确认 `IO::write_sectors` 属于计划承诺）
> - 规范增强项独立 backlog（不阻断本轮）
> - 可机检验证据（test/clippy/doc）
> **Effort**: Short
> **Parallel**: YES - 3 waves
> **Critical Path**: T1 决策归类 → T3 实现/文档改动 → T6 回归验证

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md`、`misc/MS-VHDX.md` 的差别，并“以计划为准”。

### Interview Summary
- 用户要求直接编写执行计划。
- 既有调研已完成：`src/file.rs`、`src/io_module.rs`、`src/validation.rs`、`src/sections/*`、`vhdx-cli/src/commands/*`。
- 已确认策略：**计划合规优先**，规范完整性作为次级维度。

### Metis Review (gaps addressed)
- 防止 scope creep：不要把 spec 增强项误判为计划阻断。
- `checksum on open` 需降级为覆盖评估项，不应表述为“完全缺失”。
- `IO::write_sectors` 是否阻断，取决于它是否被计划承诺为外部可用能力。

## Work Objectives
### Core Objective
- 交付“决策完备”的计划一致性整改：计划承诺项 100% 可验证；非承诺项不阻塞当前验收。

### Deliverables
- D1: 差异归类表（Plan-required / Plan-not-required / Optional-spec）。
- D2: 代码/文档改动（仅针对计划承诺差异）。
- D3: 规范增强 backlog（单独列项，不纳入本轮阻断）。
- D4: 自动化验证报告与证据文件。

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` 全通过。
- `cargo clippy --workspace` 无新增 warning。
- `cargo doc --no-deps` 成功。
- `docs/API.md` 与 `docs/plan/API.md` 的“计划承诺面”无冲突条目。

### Must Have
- 以 `docs/plan/API.md` 为唯一验收基线。
- 每个差异必须带“是否计划承诺”的判定与依据。
- 每个实现任务含 happy/failure QA 场景。

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- 不把 log-write crash consistency / DataWriteGuid / differencing bitmap write 作为本轮阻断（除非计划文本明确要求）。
- 不做无证据的“推测性大改”。
- 不以手工主观检查作为验收条件。

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + cargo test/clippy/doc
- QA policy: 每个任务都含 agent 可执行场景
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
Wave 1: 决策归类与范围冻结（analysis）
Wave 2: 计划承诺项整改（implementation/docs）
Wave 3: 回归验证与发布材料（qa/review）

### Dependency Matrix (full, all tasks)
- T1 → T2/T3/T4
- T2 → T5
- T3/T4/T5 → T6
- T6 → T7

### Agent Dispatch Summary (wave → task count → categories)
- Wave1 → 2 tasks → `deep`, `unspecified-low`
- Wave2 → 3 tasks → `quick`, `unspecified-low`
- Wave3 → 2 tasks → `unspecified-low`, `deep`

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. 冻结“计划承诺面”判定矩阵

  **What to do**: 从 `docs/plan/API.md` 提取承诺 API/语义，逐条映射到实际符号；输出 `Plan-required / Plan-not-required / Optional-spec` 三列矩阵。
  **Must NOT do**: 不进入代码改动；不引入 spec-only 新需求。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 需要严格语义判定与范围冻结
  - Skills: `[]` - 无额外技能依赖
  - Omitted: `[]` - N/A

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T2,T3,T4 | Blocked By: None

  **References**:
  - Pattern: `docs/plan/API.md` - 计划承诺基线
  - Pattern: `src/lib.rs` - 实际导出面
  - Pattern: `src/io_module.rs` - IO 承诺能力定位
  - External: `misc/MS-VHDX.md` - 仅作规范锚点

  **Acceptance Criteria**:
  - [ ] 生成矩阵文件并标注每条依据来源（path + symbol）

  **QA Scenarios**:
  ```
  Scenario: 计划承诺矩阵可追溯
    Tool: Bash
    Steps: 读取矩阵并逐条抽查至少10条映射，确认每条有 docs/plan 与源码路径
    Expected: 抽查条目100%可追溯，0条“无依据”
    Evidence: .sisyphus/evidence/task-1-plan-matrix.txt

  Scenario: 错误归类防护
    Tool: Bash
    Steps: 抽查所有被标记为 Optional-spec 的项是否存在 plan 明文承诺
    Expected: 若存在明文承诺则自动改判，最终0条误归类
    Evidence: .sisyphus/evidence/task-1-plan-matrix-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 2. 处理 `IO::write_sectors` 计划阻断判定与整改

  **What to do**: 若 T1 结论为 `IO::write_sectors` 属于计划承诺，则实现可用写入或移除误导承诺路径；若非承诺，记录为技术债并从阻断列表移除。
  **Must NOT do**: 不改变 `IO::sector`/`Sector::write` 既有对外语义。

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: 单点功能修复或降级处理
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T6 | Blocked By: T1

  **References**:
  - Pattern: `src/io_module.rs` - `write_sectors`
  - Pattern: `docs/plan/API.md` - IO 语义承诺
  - Test: `tests/integration_test.rs` - 写路径行为回归

  **Acceptance Criteria**:
  - [ ] 若承诺存在：写路径可执行且通过新增/现有测试
  - [ ] 若承诺不存在：阻断清单移除并在 backlog 记录技术债

  **QA Scenarios**:
  ```
  Scenario: 写路径可执行（承诺场景）
    Tool: Bash
    Steps: 运行针对写路径的集成测试与过滤测试用例
    Expected: 目标测试全部通过，无 panic/InvalidParameter stub 错误
    Evidence: .sisyphus/evidence/task-2-io-write.txt

  Scenario: 非承诺场景降级正确
    Tool: Bash
    Steps: 运行矩阵校验，确认该项从 blocking 移至 tech-debt
    Expected: 阻断列表不再包含该项，且有明确后续追踪编号
    Evidence: .sisyphus/evidence/task-2-io-write-error.txt
  ```

  **Commit**: YES | Message: `fix(io): align write_sectors handling with plan contract` | Files: [src/io_module.rs, tests/*, docs/*]

- [ ] 3. 对齐 `docs/API.md` 到 `docs/plan/API.md`

  **What to do**: 修正文档中与计划承诺冲突/缺漏项（导出面、命名空间、语义说明），保证“以计划为准”。
  **Must NOT do**: 不借机扩展新 API；不改动实现语义。

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: 文档结构化改写
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T6 | Blocked By: T1

  **References**:
  - Pattern: `docs/plan/API.md`
  - Pattern: `docs/API.md`
  - Pattern: `src/lib.rs`

  **Acceptance Criteria**:
  - [ ] 文档中“计划承诺面”无冲突条目
  - [ ] 关键类型/方法路径与现实现一致

  **QA Scenarios**:
  ```
  Scenario: 文档对齐差异检查
    Tool: Bash
    Steps: 对 docs/API.md 与承诺矩阵进行一致性比对
    Expected: 0 条承诺冲突
    Evidence: .sisyphus/evidence/task-3-doc-align.txt

  Scenario: 误改防护
    Tool: Bash
    Steps: 检查是否新增了 plan 未承诺 API 叙述
    Expected: 0 条越界新增
    Evidence: .sisyphus/evidence/task-3-doc-align-error.txt
  ```

  **Commit**: YES | Message: `docs(api): align exported contract with plan baseline` | Files: [docs/API.md]

- [ ] 4. 降级误报：checksum-on-open 从“缺失”改为“覆盖评估”

  **What to do**: 在差异报告中修正表述，明确 `SpecValidator` 路径已提供 CRC 校验；仅评估是否需要在 open 阶段前置。
  **Must NOT do**: 不将其强行升级为阻断。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 结论修正与证据固化
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T6 | Blocked By: T1

  **References**:
  - Pattern: `src/validation.rs`
  - Pattern: `src/sections/header.rs`
  - Pattern: `docs/plan/API.md`

  **Acceptance Criteria**:
  - [ ] 报告中不再出现“checksum 完全缺失”的错误表述

  **QA Scenarios**:
  ```
  Scenario: 结论一致性检查
    Tool: Bash
    Steps: 搜索报告中的 checksum 结论措辞
    Expected: 仅出现“覆盖评估/时机评估”，不出现“完全缺失”
    Evidence: .sisyphus/evidence/task-4-checksum-wording.txt

  Scenario: 证据匹配
    Tool: Bash
    Steps: 抽查引用到 validation/header 的路径与方法名
    Expected: 100%路径有效
    Evidence: .sisyphus/evidence/task-4-checksum-wording-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 5. 规范增强 backlog 拆分（非阻断）

  **What to do**: 将 log-write crash consistency、DataWriteGuid 更新、differencing bitmap 写入整理为单独里程碑与优先级，不进入本轮 DoD。
  **Must NOT do**: 不把 backlog 项重新引入当前阻断。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 范围治理与路线图整理
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T6 | Blocked By: T1

  **References**:
  - Pattern: `misc/MS-VHDX.md`
  - Pattern: `src/file.rs`
  - Pattern: `src/validation.rs`

  **Acceptance Criteria**:
  - [ ] 三项增强均存在独立条目（目标、风险、验收草案）

  **QA Scenarios**:
  ```
  Scenario: 非阻断隔离
    Tool: Bash
    Steps: 检查 blocking 列表与 backlog 列表的交集
    Expected: 交集为空
    Evidence: .sisyphus/evidence/task-5-backlog-split.txt

  Scenario: 里程碑完整性
    Tool: Bash
    Steps: 校验每个 backlog 项包含目标/风险/验收草案字段
    Expected: 字段完整率100%
    Evidence: .sisyphus/evidence/task-5-backlog-split-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 6. 全量回归与质量闸门

  **What to do**: 运行 `test/clippy/doc`，产出最终验收证据与结论摘要。
  **Must NOT do**: 不跳过失败项；不人工裁定通过。

  **Recommended Agent Profile**:
  - Category: `unspecified-low` - Reason: 命令式回归验证
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: T7 | Blocked By: T2,T3,T4,T5

  **References**:
  - Test: `tests/integration_test.rs`
  - Pattern: `Cargo.toml`

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` 通过
  - [ ] `cargo clippy --workspace` 无新增 warning
  - [ ] `cargo doc --no-deps` 成功

  **QA Scenarios**:
  ```
  Scenario: 回归通过
    Tool: Bash
    Steps: 依次运行 cargo test/clippy/doc
    Expected: 三项命令均 exit code 0
    Evidence: .sisyphus/evidence/task-6-regression.txt

  Scenario: 失败可追踪
    Tool: Bash
    Steps: 注入失败分支（例如临时使用错误参数执行一条命令）验证报告模板
    Expected: 失败日志被结构化记录，含命令/错误摘要/重试建议
    Evidence: .sisyphus/evidence/task-6-regression-error.txt
  ```

  **Commit**: NO | Message: `n/a` | Files: []

- [ ] 7. 交付最终差异报告与执行建议

  **What to do**: 输出最终报告（已满足/部分满足/不满足），并给出下一步执行入口。
  **Must NOT do**: 不遗留“未判定”条目。

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: 汇总决策与发布口径
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: None | Blocked By: T6

  **References**:
  - Pattern: `docs/plan/API.md`
  - Pattern: `docs/API.md`
  - External: `misc/MS-VHDX.md`

  **Acceptance Criteria**:
  - [ ] 报告中 0 个“未归类”差异
  - [ ] 每条结论都可追溯到 plan 或源码证据

  **QA Scenarios**:
  ```
  Scenario: 报告完整性
    Tool: Bash
    Steps: 检查报告结构与字段完整性（分类/证据/优先级/建议）
    Expected: 完整率100%
    Evidence: .sisyphus/evidence/task-7-final-report.txt

  Scenario: 归类冲突检查
    Tool: Bash
    Steps: 校验同一差异是否被重复分类到多个互斥组
    Expected: 0 条冲突
    Evidence: .sisyphus/evidence/task-7-final-report-error.txt
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
- 仅在任务明确标注 `Commit: YES` 时提交。
- 推荐两次原子提交：
  1) `fix(io): align write_sectors handling with plan contract`（若触发）
  2) `docs(api): align exported contract with plan baseline`

## Success Criteria
- 计划承诺面 100% 对齐。
- 阻断项清零（或被判定非承诺并下放 backlog）。
- 回归命令全绿，证据齐全。
- 规范增强项独立排期且不污染当前验收范围。
