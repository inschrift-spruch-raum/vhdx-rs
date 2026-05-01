## 2026-05-01T12:41:00+08:00 Task: bootstrap
- 先执行 Task 1（基线触点索引），不直接改代码。
- 若 API 形态差异与实现正确性冲突，优先保持正确实现并同步计划文档说明。
- 高风险优先顺序：BAT 正确性 > strict 语义 > 文档契约。

## 2026-05-01 Task 6: API 形态差异决策
- 决策：优先文档改，不改代码。
- 理由：
  1) `SpecValidator<'a>` 生命周期绑定 `&File` 是现有安全契约，且已通过全量测试验证。
  2) `HeaderStructure::create -> Vec<u8>` 与当前创建/写盘流程深度耦合，改为返回视图会引入额外生命周期与所有权复杂度，属于非必要 API 扰动。
  3) 目标是“契约描述与实现一致”，当前实现无明显错误，按规则应同步计划文档而非修改生产逻辑。

## 2026-05-01 Task 8: Error 映射决策
- 决策：仅更新 `docs/plan/API.md` 的 Error 契约表达，不修改 `src/error.rs`。
- 理由：
  1) 实现变体是计划核心集合的兼容超集，删除或重命名任一现有变体会引入不必要风险。
  2) 扩展变体承载更细粒度诊断语义，保留有助于调用方定位错误来源。
  3) 将契约解释为“core + extension”后，兼容性结论清晰，且与现有测试基线一致。

## 2026-05-01 Task 12: validator 契约对齐决策
- 决策：仅修改 docs/plan/API.md 的 validator 条目，不改 src/*.rs。
- 理由：当前实现签名和导出行为一致且已被测试覆盖，本任务目标是消除文档歧义，属于契约文字校准而非实现修复。
## 2026-05-01T14:52:44+08:00 Task 14 决策
- 决策：仅修订 docs/plan/API.md，不改 src/*.rs 生产实现。
- 理由：实现行为已稳定且测试覆盖充分，HeaderStructure::create 的正确契约是“返回可写盘的 4KB 序列化字节缓冲（Vec<u8>）”。
- 落地：在 API 树与详细设计两处同时声明 create(...) -> Vec<u8> 的产物语义，显式排除“结构视图返回值”解释。
- Task 15 decision: prefer direct correction of stale path prefixes (`vhdx` -> `vhdx_rs`) in docs/plan/API.md rather than introducing new examples, to keep scope strictly on path/use contract alignment.
- Task 15 decision: do not touch production Rust code, because current `src/lib.rs` exports already satisfy required paths (`section::Entry`, `section::StandardItems`, root and module `SpecValidator` access).

## 2026-05-01 Task 17 决策
- 决策：验收报告以既有 Task 1 到 Task 16 证据为唯一事实来源，不新增实现层变更。
- 理由：Task 17 目标是“计划一致性验收”，核心是可追溯汇总，不是功能实现。
- 落地：在 `docs/plan/alignment-acceptance-report.md` 中统一采用“状态（已修复/保留）+ 理由 + 证据链接”格式，且每个主要结论至少绑定一个 `.sisyphus/evidence/` 文件。

## 2026-05-01 Task 18 决策
- 决策：本任务仅输出风险审计证据，不修改生产代码或测试代码。
- 理由：Task 18 定义为“代码质量复核（非功能）”，且约束禁止范围外重构；发现的风险均非阻断级别，可通过 disposition 记录进入后续波次。
- 落地：将 Medium/Low 风险标记为 accepted，并在 `.sisyphus/evidence/task-18-code-quality.txt` 给出文件/行号/证据。

## Task 19 - scope fidelity decision
- Decision: keep all mapped implementation/doc/evidence/notepad artifacts because each maps directly to Tasks 1-17 outputs or required orchestration metadata.
- Decision: remove only `.sisyphus/plans/api-alignment-full.md` modifications because plan mutation violates explicit read-only rule and is not required deliverable output.
- Decision: retain `.sisyphus/boulder.json` as orchestration runtime metadata; classify as non-product artifact, not feature scope.

## Task 19 (current run) - decision addendum
- Decision: treat repeated `.sisyphus/plans/api-alignment-full.md` checkbox changes as prohibited deltas and always remove immediately.
- Decision: retain `.sisyphus/boulder.json` and notepad/evidence files as orchestration artifacts, not feature creep, when they directly support Tasks 1-19 traceability.
- Decision: require post-cleanup `cargo test --workspace` evidence whenever scope cleanup modifies tracked content.
## 2026-05-01T15:21:54+08:00 Task 20 决策
- 决策：Task 20 仅产出事实性收口文档，不引入任何实现层或测试层变更。
- 理由：该任务目标是发布前可追溯总结，且明确禁止未验证声明；因此以 Task 1-19 证据为唯一事实源。
- 决策：将“残余风险”限定为 Task 18 已接受的 medium/low 项，不新增推测性风险项。

- [2026-05-01 F1] Plan-compliance gate enforces strict plan immutability: any staged mutation under .sisyphus/plans/*.md is a blocking violation even if all functional deliverables pass.
