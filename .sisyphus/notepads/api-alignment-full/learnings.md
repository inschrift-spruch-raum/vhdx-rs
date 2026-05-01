## 2026-05-01T12:41:00+08:00 Task: bootstrap
- 计划基准已固定为 docs/plan/API.md 与 docs/Standard/MS-VHDX-只读扩展标准.md。
- 已确认关键差异：strict 参数无效、Bat::new chunk ratio 默认硬编码。
- 已确认无 Rust 语言层 UB；唯一 unsafe 在 log entry 路径，需边界回归防退化。

## 2026-05-01T12:44:00+08:00 Task 1: 差异基线与触点索引
- strict 为死参数：src/file.rs:751 和 :871 两处 `let _ = strict;` 显式丢弃，optional unknown 放宽未实现。
- Bat::new chunk_ratio 硬编码为 calculate_chunk_ratio(512, 32MB)=128，不接受外部 logical_sector_size/block_size。
  生产调用点仅 2 处（sections.rs:269, :286），测试 5 处。
- SectionsConfig 唯一构造点在 file.rs:771（open_file_with_options 内），entry_count 用正确参数计算。
- CLI 不调用 .strict()，使用默认 true；LogReplayPolicy 使用覆盖 Require/Auto/InMemoryOnReadOnly/ReadOnlyNoReplay 全部变体。
- 读写路径 chunk_ratio 独立计算（file.rs:309, :569），使用 File 实际参数，与 Bat::new 硬编码可能不一致。
- API 重导出范围：lib.rs 重导出覆盖计划全部类型，且额外包含 CreateOptions/OpenOptions/LogReplayPolicy/ParentChainInfo/SectionsConfig/constants/validation。
- 关键阻塞：D1(strict)→Task 2, D2(BAT chunk_ratio)→Task 3, D3(CLI strict)→Task 6, D4(读写路径不一致)→Task 3

## 2026-05-01 Task 2: 修复 BAT chunk ratio 默认值硬编码
- Bat::new 签名扩展：`(data, entry_count)` → `(data, entry_count, logical_sector_size, block_size)`，内部替换 `LOGICAL_SECTOR_SIZE_512`/`DEFAULT_BLOCK_SIZE` 为参数。
- SectionsConfig 新增 `logical_sector_size: u32` + `block_size: u32` 字段，Sections 存储并在 bat()/bat_mut() 中透传。
- file.rs:771 唯一构造点已传入真实 `logical_sector_size` 和 `block_size`（来自元数据解析）。
- 测试中 `DEFAULT_BLOCK_SIZE`/`LOGICAL_SECTOR_SIZE_512` 导入已移除（bat.rs），测试用 `TEST_LOGICAL_SECTOR_SIZE=512`/`TEST_BLOCK_SIZE=32MB` 常量。
- 全部 287 个测试通过（36 单元 + 32 API surface + 164 集成 + 55 CLI + 3 doctest）。
- 生产路径与读写路径的 chunk_ratio 计算现在一致：Bat::new 用实际参数，file.rs read/write 也用 File 字段计算。

## 2026-05-01 Task 3: strict=false 语义生效
- 替换两处 `let _ = strict;` 为实际分支：
  - `validate_region_entries(region_table, strict)` — 新函数，strict=true 拒绝所有 unknown，strict=false 仅拒绝 required unknown
  - `validate_metadata_items(metadata, strict)` — 新函数，同上语义
- 旧函数 `validate_required_region_entries_are_known` 和 `validate_required_metadata_items_are_known` 已被替换（不再是独立入口）
- 错误类型契约不变：InvalidRegionTable / InvalidMetadata
- strict=true 的错误消息区分 required 和 optional（"Unknown required region" vs "Unknown optional region (strict mode)"）
- 测试方法：创建标准 VHDX → 注入 unknown entry（修改 entry_count + 追加条目 + 重算 CRC32C）→ 分别用 strict=true/false 打开
- Region 表注入需修改两个副本（RT1 + RT2）并重算 CRC32C
- Metadata 表无 CRC32C 校验（Metadata::new 不验证校验和），可直接修改 entry_count 和条目
- File 不实现 Debug，测试中 match 分支的 panic 消息不能用 `{:?}`，需用 `{e}` 格式化 Error
- 全部 297 测试通过（43 + 32 + 164 + 55 + 3）

## 2026-05-01 Task 4: LogReplayPolicy 回归固化
- 新增 5 个边界回归测试，覆盖 4 种策略 × 可写/只读组合：
  1. `test_require_policy_rejects_when_pending_logs` — Require+pending log → LogReplayRequired（只读和可写均拒绝）
  2. `test_readonly_no_replay_rejects_writable_with_pending_logs` — ReadOnlyNoReplay+可写+pending → InvalidParameter
  3. `test_inmemory_on_readonly_exposes_replayed_data_via_io` — InMemoryOnReadOnly+只读+pending → 内存回放，IO 可读回放数据
  4. `test_readonly_no_replay_allows_structure_reading_with_pending_logs` — ReadOnlyNoReplay+只读+pending → 结构可读(header/metadata)，has_pending_logs=true，数据不回放
  5. `test_auto_policy_writable_replays_to_disk` — Auto+可写+pending → 磁盘回放成功，数据持久化
- 已有测试 `test_inmemory_on_readonly_rejects_writable_with_pending_logs` 覆盖 write()+InMemoryOnReadOnly+pending → InvalidParameter，无需重复
- File 不实现 Debug trait，使用 `expect_err` 会编译失败，需用 `match { Ok(_) => panic!(...), Err(e) => e }` 模式
- 未修改任何生产代码，仅新增测试
- 全部 302 测试通过（43 + 32 + 169 + 55 + 3）

## 2026-05-01 Task 5: UB 安全边界锁定
- 唯一 unsafe 在 Log::entry() const fn (log.rs:118-119): `from_raw_parts(raw.as_ptr().add(offset), data_len)`
- 安全前提由 while 循环不变量保证：`offset + LOG_ENTRY_HEADER_SIZE <= raw.len()`，且 `data_len = raw.len() - offset`
- 新增 12 个 UB 安全边界测试（test_ub_safety_*），覆盖：
  1. 描述符计数/偏移不一致 → LogEntryCorrupted("descriptor parse mismatch")
  2. entry_length 超出可用字节 → descriptors 安全返回空
  3. descriptor_area 超出 entry_length → descriptors 安全返回空
  4. 数据扇区签名无效("xxxx") → LogEntryCorrupted("invalid data sector signature")
  5. 撕裂写入(seq_high≠seq_low) → LogEntryCorrupted("torn data sector detected")
  6. leading_bytes 极值(u64::MAX) → DataDescriptor::new 安全解析，replay 路径 checked_add 拦截
  7. 空日志区域 → entry(0)=None, 不 panic
  8. 数据短于头部(63字节) → entry(0)=None, 不进入 unsafe 路径
  9. 数据扇区数量不匹配 → LogEntryCorrupted("descriptor parse mismatch")
  10. CRC 不匹配阻塞回放 → LogEntryCorrupted("checksum"), 无磁盘写入
  11. descriptor_area size overflow → descriptors 安全返回空
  12. 多条目索引一致性 → entry(N)==entries()[N], 越界返回 None
- 未修改任何生产代码，仅新增测试
- 全部 314 测试通过（43 + 32 + 181 + 55 + 3）
- 注意：hex 字面量中字母后缀会被 Rust 解析为类型后缀（如 0xBAD_CKSM 中 KSM 被解析为后缀），需避免使用含字母后缀的 hex 命名
- 注意：descriptor_count=u32::MAX 会导致 descriptors() 遍历 40 亿次，测试会超时；改用小值验证逻辑等价性

## 2026-05-01 Task 6: API 形态差异对齐（SpecValidator / HeaderStructure::create）
- `File::validator` 实际返回 `SpecValidator<'_>`，文档原写法遗漏生命周期，属于契约表达不完整，不是实现错误。
- `SpecValidator` 实际定义为 `SpecValidator<'a> { file: &'a File }`，文档原写法为无生命周期版本，需改文档以避免误导使用者。
- `HeaderStructure::create` 实际语义是“构造 4KB header 原始字节并写入 CRC32C”，返回 `Vec<u8>`，不是返回 `HeaderStructure<'a>` 视图。
- `HeaderStructure::create` 调用点覆盖创建路径与头部更新路径（`src/file.rs` 多处），当前返回字节缓冲区的形态与写盘流程一致，稳定且正确。
- 本任务采用“仅文档修订”策略，避免对已验证通过的行为进行不必要代码扰动。

## 2026-05-01T13:41:08+08:00 Task 7: Bat::new 新签名传播验证
- 结论：Task2 已完全覆盖本任务目标，无需任何代码变更。
- 验证方法：ast-grep 搜索全部 Bat::new( 调用，7 处全部使用 4 参数签名。
- 生产路径 2 处（sections.rs:280 bat()、sections.rs:302 bat_mut()）均从 Sections 字段透传 logical_sector_size + block_size。
- SectionsConfig 唯一构造点（file.rs:772-783）传入 read_metadata() 解析的真实值。
- 测试路径 5 处使用 TEST_LOGICAL_SECTOR_SIZE=512 + TEST_BLOCK_SIZE=32MB 常量。
- 全部 314 测试通过（43 + 32 + 181 + 0 + 55 + 3），无回归。
- 证据文件：.sisyphus/evidence/task-7-bat-signature.txt

## 2026-05-01 Task 8: Error 语义映射对齐
- `src/error.rs` 当前共有 20 个变体，`docs/plan/API.md` 旧版仅列 12 个核心变体，存在“计划小于实现”的表达差距。
- 通过 `ast-grep` 引用检索确认扩展变体在实现中被真实使用，不是死代码，主要集中在区域表、元数据、日志和 IO 边界校验路径。
- 对外兼容可按“语义分组”理解，新增扩展变体属于诊断细化，不影响既有核心变体匹配逻辑。
- 文档应显式标注 contract core 与 implementation extension，避免调用方误判为破坏性变更。

## 2026-05-01 Task 9: 对照测试矩阵
- 建立 5 类计划条目→自动化测试映射矩阵：
  1. strict 三分法 → 13 个测试（7 unit + 6 integration）
  2. BAT 非默认参数/签名传播 → 23 个测试（12 unit + 7 integration + 1 静态分析 + 3 间接）
  3. API shape (SpecValidator/HeaderStructure/create) → 56 个测试（32 smoke + 5 unit + 38 integration）
  4. LogReplayPolicy 四策略 → 35 个测试（含 12 回放语义 + 14 策略矩阵 + 3 枚举 + 6 会话）
  5. UB 边界 → 17 个测试（12 ub_safety + 5 log unit）
- 全部 314 测试通过，无缺失映射项。
- 缺失映射检测方式：① 命名模式计数 ② 证据文件反向追踪 ③ 注入缺失项验证灵敏度。
- 矩阵结果可直接复用给 Task 17 汇总，无 gap 需补偿。
- 证据文件：.sisyphus/evidence/task-9-test-matrix.txt, task-9-test-matrix-error.txt

## 2026-05-01 Task 10: 补齐 strict 模式测试（三分法）
- strict 行为矩阵（2 strict × 2 unknown-type × 2 section = 8 组合），新增 8 个 `t10_strict_*` 测试：
  1. `t10_strict_true_rejects_required_unknown_region_with_error_variant` — Error::InvalidRegionTable("Unknown required region")
  2. `t10_strict_true_rejects_required_unknown_metadata_with_error_variant` — Error::InvalidMetadata("Unknown required metadata item")
  3. `t10_strict_true_rejects_optional_unknown_region` — Error::InvalidRegionTable("Unknown optional region")
  4. `t10_strict_true_rejects_optional_unknown_metadata` — Error::InvalidMetadata("Unknown optional metadata item")
  5. `t10_strict_false_allows_optional_unknown_region` — Ok（此前已有类似测试）
  6. `t10_strict_false_allows_optional_unknown_metadata` — Ok（此前完全缺失）
  7. `t10_strict_false_rejects_required_unknown_region_with_error_variant` — Error::InvalidRegionTable("Unknown required region")
  8. `t10_strict_false_rejects_required_unknown_metadata_with_error_variant` — Error::InvalidMetadata("Unknown required metadata item")
- 缺口发现：Task3 已覆盖 strict 分支逻辑和部分测试，但缺少以下：
  - strict=true + optional unknown 的拒绝（region 和 metadata 各 1 个）
  - strict=false + optional unknown metadata 的允许（此前完全缺失）
  - 明确错误变体断言（部分已有测试仅用 `is_err()`，无 Error variant 匹配）
- File 不实现 Debug，需使用 `match { Ok(_) => panic!(...), Err(e) => e }` 模式代替 `expect_err()`
- metadata 可选注入使用 flags=0（不含 0x2000_0000 required 位）
- 全部 189 个集成测试通过，workspace 全量通过
- 证据文件：.sisyphus/evidence/task-10-strict-matrix.txt, task-10-strict-matrix-error.txt

## 2026-05-01 Task 11: BAT 非默认参数回归测试
- 新增 5 个单元测试 + 3 个集成测试，覆盖 4096 逻辑扇区 + 可变块大小下 BAT 行为
- 核心回归断言：
  - chunk_ratio 4096+32MB=1024 ≠ 512+32MB=128
  - 索引 128 在 4096 扇区为 Payload，512 扇区为 SectorBitmap
  - 130 payload blocks 总条目数 4096→131 ≠ 512→132
- entry_count 计算：payload_blocks = entry_count - ceil(entry_count/(chunk_ratio+1))，非简单的 entry_count-1
  - entry_count=129 + chunk_ratio=1024 → payload=128, bitmap=1, 索引128=bitmap（不满足需求）
  - entry_count=130 + chunk_ratio=1024 → payload=129, bitmap=1, 索引128=payload, 索引129=bitmap（满足需求）
  - entry_count=130 + chunk_ratio=128 → payload=128, bitmap=2, 索引128=bitmap（满足需求）
- Dynamic 创建是稀疏的，4GB 虚拟大小不分配 4GB 磁盘空间，测试保持快速
- 全部 330 测试通过（48 + 32 + 192 + 55 + 3）
- 证据文件：.sisyphus/evidence/task-11-bat-nondefault.txt, task-11-bat-nondefault-error.txt
## 2026-05-01 Task 12: validator 契约表述校准
- File::validator 实现签名是 `pub fn validator(&self) -> crate::validation::SpecValidator<'_>`，文档应显式保留 `<'_>`，避免误读为无生命周期类型。
- lib.rs 已根模块重导出 `SpecValidator` 与 `ValidationIssue`，因此公共路径存在两种等价写法：`vhdx_rs::SpecValidator<'_>` 与 `vhdx_rs::validation::SpecValidator<'_>`。
- `SpecValidator` 实际定义为 `SpecValidator<'a> { file: &'a File }`，生命周期语义是“借用 File 的只读校验器”，不应在文档中省略。

## 2026-05-01 Task 13: create/open 内部策略一致性
- 修复 `src/file.rs::open_file` 内部默认策略：`Auto -> Require`，与公开 `File::open(...).finish()` 默认契约保持一致。
- 在 `open_file` 与 `create_file` step-10 增加中文意图注释，明确“内部策略对齐是有意设计”。
- 新增回归测试：
  1) `test_task13_public_default_open_policy_is_require`（pending log 下默认打开必须 `LogReplayRequired`）
  2) `test_task13_create_internal_reopen_semantics_on_clean_file`（create 内部 reopen 语义在 clean 文件下稳定成功）
- 全量验证通过：`cargo test --workspace`。
## 2026-05-01T14:52:36+08:00 Task 14: HeaderStructure::create 文档/语义对齐
- src/sections/header.rs 中 HeaderStructure::create 实际签名为 pub fn create(...) -> Vec<u8>，实现会构造固定 4KB 头部字节并写入 CRC32C。
- src/file.rs 调用路径按“字节缓冲写盘”消费返回值，不存在将 create 返回值当作 HeaderStructure<'_> 借用视图继续操作的语义。
- 文档修订应坚持“序列化产物”表述，避免写成“返回结构视图”，与 Task 6/12 的契约对齐策略保持一致。
- Task 15 confirmed docs/plan/API.md had stale crate import path `vhdx::...` while current crate path is `vhdx_rs::...`; updated only example imports and top API tree label to match real exports.
- For export-path clarity, kept validator usage as `file.validator()` and retained documented equivalence between `SpecValidator<'_>` and `validation::SpecValidator<'_>` to avoid mixed guidance.
- Added explicit documentation note that `section::Entry` is a compatibility alias of `section::LogEntry`, reducing ambiguity without changing API behavior.
- Root README.md is absent in this repo snapshot, so README scope was checked and intentionally left unchanged.

## 2026-05-01 Task 16: 全量回归门禁执行
- 按顺序执行 `cargo test --workspace` → `cargo clippy --workspace` → `cargo fmt --check`。
- test 与 clippy 首次即通过；fmt 首次失败为纯格式差异，执行 `cargo fmt` 后 `cargo fmt --check` 复跑通过。
- 本任务修复为最小范围且仅格式化，不包含逻辑变更；最终三项门禁全部全绿。
- 证据落盘：`.sisyphus/evidence/task-16-quality-gates.txt` 与 `.sisyphus/evidence/task-16-quality-gates-error.txt`。

## 2026-05-01 Task 17: 计划一致性验收报告
- 报告采用“差异项 -> 状态 -> 理由 -> 证据链接”固定结构，能直接支撑后续 Task 18/19 审计输入。
- strict、BAT、API 形态、UB、质量门禁均已在报告中建立可追溯证据链，避免无来源结论。
- `cargo test --workspace` 在本任务内复跑通过（48 + 32 + 194 + 55 + 3 = 332 passed, 0 failed）。
- 保留项与残余偏差采用“可接受保留”表达，并附对应 evidence，避免与“未完成修复”混淆。

## 2026-05-01 Task 18: 非功能代码质量复核
- 代码质量审计中，`git diff` 在全部变更已 staged 时会显示空结果；必须使用 `git diff --cached` 才能正确锁定计划阶段变更面。
- BAT 参数化主链已闭环：`SectionsConfig(logical_sector_size, block_size)` → `Sections::bat/bat_mut` → `Bat::new`，生产路径无旧签名残留。
- strict=false 语义与 Task 13 策略边界一致：`required unknown` 仍失败，`optional unknown` 才放宽；默认 open 策略仍是 Require。
- 当前主要维护性风险来自“重复测试与断言强度不一致”，不是功能错误；在范围受限任务中应记录风险并给出 disposition，而非越界重构。

## Task 19 - scope fidelity learning
- Plan files under `.sisyphus/plans/*.md` must be treated as immutable governance artifacts; checkbox/status edits are scope violations even when semantically accurate.
- Scope audits should classify files into implementation/docs/evidence/notepad/orchestration metadata to avoid false positives and ensure 100% traceability.
- When cleanup touches tracked content, rerunning `cargo test --workspace` provides concrete proof that scope-remediation did not regress functionality.

## Task 19 (current run) - learning addendum
- Scope fidelity must be re-checked against *current* worktree each run; previously resolved plan-file violations can recur via later checkbox toggles.
- A complete mapping for Task 19 must include newly added evidence files (e.g., Task 18 artifacts) to preserve 100% traceability.
- `.sisyphus` artifacts should be explicitly categorized (evidence/notepad/orchestration metadata) to separate process data from product-scope changes.
## 2026-05-01T15:21:54+08:00 Task 20: 发布前收口（证据一致性）
- 最终摘要采用“结论后置证据列表”格式，逐条绑定 .sisyphus/evidence 或 notepad 路径，避免无来源陈述。
- 一致性校验可通过脚本化“引用路径存在性检查 + unsupported claim 人工复核”形成可重复闭环。
- 环境工具受限（LSP/rg 缺失）时，收口阶段应明确替代验证基线：Rust 原生门禁结果 + 证据链交叉检查。
