# Draft: API Alignment Full

## Requirements (confirmed)
- 检查实际实现与 `docs/plan/API.md` 和 `docs/Standard` 计划差别，以计划为准。
- 回复不一致的地方和 UB；没有就直接说完全没有。
- 当前已明确：需要“编写任务”。
- 范围选择：全量对齐计划。
- 交付形式：生成执行计划文件。

## Technical Decisions
- 计划基准：`docs/plan/API.md` + `docs/Standard/MS-VHDX-只读扩展标准.md`。
- 交付策略：单一执行计划，覆盖代码修复、测试、验证、文档对齐。
- 风险优先级：先修数据正确性风险（BAT chunk ratio），再修 strict 语义。
- UB 结论：当前无 Rust 语言层面的 UB；仅有语义/健壮性风险点。

## Research Findings
- `src/file.rs`：`strict` 参数在打开流程被丢弃（`let _ = strict;`）。
- `src/sections/bat.rs`：`Bat::new` 用默认 512/32MiB 计算 chunk ratio，未用实际元数据参数。
- `src/validation.rs`：`SpecValidator<'a>` 实际带生命周期。
- `src/sections/header.rs`：`HeaderStructure::create` 返回 `Vec<u8>`。

## Open Questions
- 无阻塞问题（按“全量对齐计划”执行）。

## Scope Boundaries
- INCLUDE: 计划对齐相关代码修复、测试补齐、验证波次、必要文档同步。
- EXCLUDE: 新功能扩展、依赖新增、`misc/` 修改。
