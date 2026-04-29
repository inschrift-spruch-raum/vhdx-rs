
## 2026-04-29 task-1 learnings
- 本轮计划判定必须以 `docs/plan/API.md` 为唯一验收基线；`docs/API.md` 仅用于辅助确认当前导出面，不可反向扩大承诺。
- 对外承诺判断以"是否公共导出 + 计划是否明确承诺"双条件判定，避免把 `pub(crate)` 内部函数误算进承诺面。
- 命名差异优先识别"是否有别名兼容路径"（如 `section::Entry = LogEntry`），有兼容路径则归为命名差异，不判能力缺失。

## 2026-04-29 task-2 learnings
- 阻断判定分支执行时，应先完整读取 T1 矩阵 + 源码 + 计划文档，形成交叉验证链，再下结论。避免仅依赖 T1 结论不做复核。
- "非承诺"下放流程：确认可见性(`pub(crate)`) -> 确认计划签名不含 -> 确认无测试/调用依赖 -> 生成 evidence + tech-debt ID。
- `docs/plan/API.md` 的设计约束行（如"唯一数据平面入口"）是比 API 签名更强的判定锚点——签名可能遗漏，但约束语义排除了隐含承诺。

## 2026-04-29 task-3 learnings
- 文档对齐时要先冻结承诺口径：只保留 plan-required，内部导出符号即使存在也不能自动升级为公开契约。
- API 文档中的签名展示应表达承诺语义，不把实现层细节（如 const 修饰）当成计划契约。
- 防越界段落应显式写出历史高风险项（如 IO::write_sectors/read_sectors），便于后续机检。

## 2026-04-29 task-3 learnings
- 文档对齐时要先冻结承诺口径，只保留 plan-required，内部导出符号即使存在也不能自动升级为公开契约。
- API 文档中的签名展示应表达承诺语义，不把实现层细节（如 const 修饰）当成计划契约。
- 防越界段落应显式写出历史高风险项（如 IO::write_sectors/read_sectors），便于后续机检。

## 2026-04-29 task-4 learnings
- CRC-32C 校验的覆盖描述需要精确区分"未实现"和"时机不对"。vhdx-rs 在 `SpecValidator` 路径中完整实现了 Header/RegionTable/Log 三大结构的 CRC-32C 校验，不应表述为"缺失"。
- "校验时机评估"是比"校验能力缺失"更准确的措辞——问题在于 `File::open` 是否应自动触发校验，而非校验功能本身不存在。
- 当不存在可编辑的"差异报告"实体文件时，证据文件本身即可作为措辞降级的正式载体。

## 2026-04-29 task-5 learnings
- 规范增强拆分需要精确区分"功能缺失"和"计划承诺缺失"——BL-DIFF-BITMAP 是真实的功能缺陷（差分写入返回错误数据），但因为不在计划承诺面而不构成阻断。
- 日志写入（BL-LOG-WRITE）与校验时机（T4）形成递进关系：T4 解决"校验是否存在"，BL-LOG-WRITE 解决"元数据变更前是否先记录日志"。
- `allocate_payload_block` 对差分磁盘错误使用 `FullyPresent` 是 BL-DIFF-BITMAP 的核心问题——读取路径已实现 `PartiallyPresent` 逻辑但写入路径从不触发它。
- DataWriteGuid 不更新仅影响差分链校验场景（`parent_linkage` 匹配），对单盘使用无影响。

## 2026-04-29 task-6 learnings
- 全量回归 270 测试全部通过（36 lib + 32 api_surface_smoke + 144 integration + 55 cli_integration + 3 doctests），T1-T5 的文档/证据工作未引入任何回归。
- `cargo clippy --workspace` 零警告通过，说明 T1-T5 的文档修改（`docs/API.md`、`docs/plan/API.md`）不涉及代码质量影响。
- `cargo doc --no-deps` 有 13 个 rustdoc 警告（12 个 broken intra-doc links + 1 个 `Error` enum/macro 歧义），全部是预先存在的文档链接问题，不影响构建。
- 质量闸门三命令退出码均为 0，但 rustdoc 警告是可追踪的技术债项。

## 2026-04-29 task-7 learnings
- 最终报告阶段可用‘分类/证据/优先级/建议’四字段模板，确保每条结论可追溯且可机检。
- 互斥分类冲突检查应基于稳定 ID（如 S-xx、BL-xxx）做重复检测，避免自然语言误判。
