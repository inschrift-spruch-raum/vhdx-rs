
## 2026-04-29 task-1 decisions
- 判定口径冻结为三类：Plan-required / Plan-not-required / Optional-spec。
- `IO::write_sectors` 最终归类为 **Plan-not-required**：依据是 `src/io_module.rs` 中该函数为 `pub(crate)` 且计划 IO 数据面承诺限定为 `IO::sector -> Sector::read/write`。
- `ReadOnlyNoReplay` 与 `ValidationIssue` 归类为 Optional-spec：两者均有计划文本显式"兼容例外/可选"依据。

## 2026-04-29 task-2 decisions
- **T2 判定：`IO::write_sectors` 不属计划承诺，下放为技术债 TD-IO-BATCH-OPS**。
  - 依据链：`docs/plan/API.md` S9 设计约束限定唯一数据面为 `IO::sector -> Sector::read/write`；`IO` 公开签名仅含 `sector()`；`src/io_module.rs:107` 为 `pub(crate)` + `#[allow(dead_code)]` + stub 错误返回；`src/lib.rs:88` 仅导出类型不导出方法；`tests/integration_test.rs` 无引用。
  - 不改变源码行为；`IO::sector`/`Sector::read`/`Sector::write` 语义完整保留。
  - 阻断列表变更：`IO::write_sectors` 从 blocking 移至 tech-debt。

## 2026-04-29 task-3 decisions
- docs/API.md 采用“计划承诺面优先”策略，根导出树移除 SectionsConfig 与 crc32c_with_zero_field 的承诺叙述。
- 对 validation 与 section::StandardItems 采用“命名空间已导出且计划有锚点”原则，补齐文档条目。
- 保留 IO 内部批量接口的非承诺边界声明，避免回归为公开承诺。

## 2026-04-29 task-4 decisions
- **T4 判定：checksum-on-open 从"缺失"降级为"覆盖评估/时机评估"**。
  - 依据：`SpecValidator::validate_header`（`src/validation.rs:100`）调用 `HeaderStructure::verify_checksum`（`src/sections/header.rs:321`）；`SpecValidator::validate_region_table`（`src/validation.rs:140`）直接计算并比对 CRC-32C；`SpecValidator::validate_log`（`src/validation.rs:437`）对每个日志条目执行 CRC-32C 校验。
  - 通用工具：`crc32c_with_zero_field`（`src/sections.rs:375`）+ `calculate_log_entry_crc32c`（`src/sections/log.rs:51`）。
  - 评估维度：是否需要在 `File::open().finish()` 中自动执行 `SpecValidator::validate_file()`，而非仅在显式调用时校验。
  - 阻断性：非阻断——校验能力完备，仅触发时机可讨论。
- 不修改 `src/**` 实现语义；仅生成证据文件记录正确结论。

## 2026-04-29 task-5 decisions
- **T5 决定：三项规范增强全部归入 backlog，不进入当前阻断**。
  - BL-LOG-WRITE（日志写入）：日志回放能力完备，写入路径属增强。计划未承诺"写入时日志保护"。
  - BL-DATA-WRITE-GUID（DataWriteGuid 更新）：影响差分链校验，但 API 签名不含更新逻辑，且更新频率策略需设计决策。
  - BL-DIFF-BITMAP（差分位图写入）：真实功能缺陷（`FullyPresent` 应为 `PartiallyPresent`），但不在计划承诺面，且修复涉及 chunk 交错 + bitmap 分配 + 日志协同。
- 优先级设定：BL-LOG-WRITE 和 BL-DIFF-BITMAP 为 P2（影响数据一致性），BL-DATA-WRITE-GUID 为 P3（影响链校验但不影响数据读写）。
- 下游依赖：BL-LOG-WRITE 是 BL-DIFF-BITMAP 的前置——bitmap 变更应受日志保护。实现顺序建议：BL-LOG-WRITE → BL-DIFF-BITMAP → BL-DATA-WRITE-GUID。

## 2026-04-29 task-6 decisions
- 质量闸门判定标准：三命令退出码均为 0 即为 PASS。rustdoc 警告不阻断但需记录。
- 证据文件分两个：`task-6-regression.txt`（完整结果）和 `task-6-regression-error.txt`（失败模板）。
- 不修复 13 个 rustdoc 警告——这些是预先存在的链接问题（`src/lib.rs` 模块文档引用子模块类型但路径不正确），属于独立清理任务。

## 2026-04-29 task-7 decisions
- 采用‘已满足/部分满足/不满足 + backlog 非阻断’四段式作为 Task-7 最终交付结构。
- 计划承诺面 100% 对齐判定依据固定为 T1 分类闭环 + T3 文档对齐 + T5 零交集 + T6 质量闸门通过。
