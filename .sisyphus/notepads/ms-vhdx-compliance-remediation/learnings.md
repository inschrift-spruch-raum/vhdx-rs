## 2026-04-24T07:11:00Z Task: session-bootstrap
Initialized notepad for plan execution. No prior learnings yet.

## 2026-04-24T07:58:00Z Task: 1
Task 1 accepted: `tests/integration_test.rs` gained reusable fixture scaffolding for cross-chunk BAT, controllable log corruption, and differencing parent-chain scenarios, plus 11 fail-first `#[ignore]` gap tests mapped to tasks 2-11.
Acceptance commands passed: `cargo test -p vhdx-rs --tests --no-run`, `cargo clippy -p vhdx-rs --tests`.

## 2026-04-24T15:30:00Z Task 1: 缺口回归测试基线与夹具扩展

### 关键发现

- **chunk_ratio 公式**：`chunk_ratio = (2^23 * logical_sector_size) / block_size`（MS-VHDX §2.5.1），不是简单的 `block_size / logical_sector_size`。例如 `block_size=32MiB, logical_sector_size=512` → `chunk_ratio=128`。
- **BAT 交错布局**：每 `chunk_ratio` 个 payload 条目后插入 1 个 sector bitmap 条目。payload BAT index = `block_idx + block_idx / chunk_ratio`。
- **`.block_size()` 接受 `u32`**：`CreateOptions::block_size()` 签名为 `u32`，需要在 helper 函数中使用 `u32::try_from(block_size)` 转换。
- **`HEADER_SECTION_SIZE` 是 `usize`**：不能直接 `u64::from(...)`，需要 `u64::try_from(...)`。
- **`sections().header()` 返回临时值**：必须先绑定到变量再调用 `.header(0)`，否则触发 E0716（临时值在借用期间被释放）。
- **`File` 没有 `write(offset, data)` 公共方法**：需要通过 `io().sector(n).write(&data)` 或 `write_raw_bytes` 写入数据。差分测试中父盘数据写入使用 `write_raw_bytes(path, header_section_size, &data)`。
- **日志条目头部偏移**（MS-VHDX §2.3.1.1）：`[0..4]` signature, `[4..8]` checksum, `[8..12]` entry_length, `[12..16]` tail, `[16..24]` sequence_number, `[24..28]` descriptor_count, `[28..32]` reserved, `[32..48]` log_guid。注意 log_guid 在 `[32..48]` 而非 `[20..36]`。
- **Helper 防御性策略**：所有新 helper 在打开文件读取 metadata 时使用 `ReadOnlyNoReplay` 策略，避免在有 pending log 时触发自动回放。

### 夹具清单

| 夹具 | 支持的 Task | 用途 |
|------|-------------|------|
| `create_dynamic_disk_for_cross_chunk_test` | T2/T3 | 创建 chunk_ratio>1 的动态磁盘 |
| `payload_bat_index` | T2/T3 | 计算 payload BAT 索引 |
| `bitmap_bat_index_for_chunk` | T2/T3 | 计算 bitmap BAT 索引 |
| `inject_cross_chunk_payload_bat_entries` | T2/T3 | 批量注入 payload 条目 |
| `build_controllable_log_entry_bytes` | T4/T5/T6/T7 | 构建可控日志条目 |
| `inject_controllable_log_entry` | T4/T5/T6/T7 | 注入可控日志条目 |
| `corrupt_log_entry_checksum` | T4 | 篡改 checksum |
| `corrupt_log_entry_signature` | T4 | 篡改 signature |
| `inject_multi_entry_log_sequence` | T7 | 注入多条目 active sequence |
| `create_differencing_pair` | T9/T10 | 创建差分磁盘对 |
| `create_three_level_chain` | T9/T10 | 创建三级差分链 |
| `set_block_partially_present` | T9 | 设置 PartiallyPresent 状态 |
| `inject_sector_bitmap_bits` | T9 | 写入扇区位图 |

## Task 2: Dynamic 读路径 BAT payload 索引换算 (completed)

- **问题**: File::read() 的 Dynamic 分支直接用 at.entry(block_idx) 查找 BAT 条目，忽略了 chunk 交错规则。
- **修复**: 引入与 write_dynamic() 一致的 at_payload_index = block_idx + (block_idx / chunk_ratio) 计算逻辑。
- **chunk_ratio 公式**: (2^23 * logical_sector_size) / block_size（MS-VHDX §2.5.1）。
- **约定**: 写路径在 Task 1 已正确实现 payload 索引换算；读路径此前遗漏了同一逻辑。
- **测试发现**: 当 logical_sector_size=512 时扇区大小为 512 字节，测试缓冲区必须匹配实际扇区大小（而非硬编码 4096）。
- **边界行为**: at.entry() 越界时返回 None，Dynamic 读路径安全地回退到零填充 — 无需额外保护代码。


## Task 3: Fixed BAT sector-bitmap 条目编码修复 (completed)

- **问题**: create_bat_data 在 Fixed 模式下对所有 at_entries（含 sector bitmap 条目）都写入 FullyPresent+payload offset。
- **根因**: 原 create_bat_data 参数缺少 logical_sector_size 和 irtual_size，无法计算 chunk_ratio 和 payload_blocks，因此无法区分 payload 条目和 sector bitmap 条目。
- **修复**: 添加 logical_sector_size 和 irtual_size 参数，使用 Bat::is_sector_bitmap_entry_index() 判断每个条目类型。payload 条目编码为 FullyPresent+payload offset，sector bitmap 条目保持全零（NotPresent+偏移 0）。
- **关键**: payload 偏移使用独立的 payload_idx 计数器（仅对 payload 条目递增），而非 BAT 索引 i，确保数据区域偏移连续正确。
- **Fixed 不需要 sector bitmap**: Fixed 类型所有块完全存在，不存在 partial write 场景，sector bitmap 条目只需 NotPresent。
- **Dynamic 分支未变**: 全零 BAT 策略保持不变，Dynamic 类型的所有条目都是 NotPresent。
- **itmap_bat_index_for_chunk helper 限制**: 该 helper 假设完整 chunk，对于 payload_blocks < chunk_ratio 的场景计算错误。测试改为直接遍历 entries 并按 BatState 类型断言。

## [2026-04-24T18:28:33.5838617+08:00] Task 4

- Replay 前置校验必须在 descriptor 处理之前执行；本次将 signature / entry_length / descriptor area / CRC-32C 统一收口到 `Log::precheck_replay_entry`，失败即返回 `Error::LogEntryCorrupted`，不继续回放。
- Entry 级 CRC 计算口径：仅覆盖 `entry_length` 指定范围，且计算前将 checksum 字段 `[4..8]` 置零，与 MS-VHDX 条目校验语义一致。
- `entries()` 过滤掉明显无效条目后，`replay()` 仍需再次做强校验，避免依赖调用方先执行 validator。
- Task 4 测试需要先把夹具注入的 checksum 修正到正确值，再进行“篡改为坏值”断言；否则无法稳定区分“原始无效”与“篡改无效”。

## [2026-04-24T19:42:35.4163572+08:00] Task 5

- Task 5 在回放入口显式绑定 active header 的 `log_guid`，并把该 GUID 贯穿到两条路径：磁盘回放（`replay_with_log_guid`）与只读 overlay（`entries_for_log_guid` 过滤后构建）。
- GUID 语义统一为“严格匹配 + 失败即报错”：发现任一条目 `entry.header().log_guid != current_header.log_guid` 立即返回 `Error::LogEntryCorrupted("Log GUID mismatch: ...")`，禁止静默跳过。
- `current_header.log_guid == Guid::nil()` 保持“无可回放日志”语义：直接返回，无 replay/overlay 副作用，避免误处理日志区噪声。
- Task 4 precheck 防线未退化：signature/entry_length/descriptor 边界/CRC 校验仍在 replay 主流程执行，Task 5 仅新增 GUID 门控，不改变原有校验顺序与错误模型。

## [2026-04-24T19:48:41.4328475+08:00] Task 5 follow-up

- GUID 一致性检查应只针对“有效活动日志条目”生效；日志区尾部零槽位/噪声不应触发 mismatch。实现上先过滤非 `"loge"` 签名，再对候选执行 Task 4 precheck，最后做 GUID 比对。
- 测试夹具 `inject_pending_log_entry` 在 Task 5 后必须同时维护两处一致性：`entry.log_guid == header.log_guid` 且 entry checksum 合法，否则会被 precheck/GUID 门控先行拦截，误伤既有 overlay/replay 行为测试。
- 保持“严格拒绝”与“不误拒绝空槽位”可并存：对可解析有效条目 mismatch 立即 `Error::LogEntryCorrupted`，对无效候选按既有扫描语义忽略。

## [2026-04-24T20:23:43.5531603+08:00] Task 5 CLI follow-up

- CLI pending-log 夹具 `inject_pending_log_for_cli` 也必须满足 Task 4 + Task 5 的同一约束：写盘前设置 `entry.log_guid`，并生成合法 checksum；否则 `check --log-replay` 会在预检阶段直接失败，无法进入 CLI 预期的 pending-log 展示路径。
- 在 CLI crate 测试中避免新增依赖：可复用 `vhdx_rs::crc32c_with_zero_field` 计算日志条目校验和，保持 Cargo 清洁且与库侧口径一致。

## [2026-04-24] Task 7

- Task 7 的 active sequence 需要先过滤“可回放候选”再做连续性裁剪：候选必须同时满足 Task 4 precheck + 描述符/数据扇区一致性（数量、签名、torn 检测、descriptor sequence 一致）。
- active sequence 采用“首条起步 + 连续递增（+1）前缀”策略即可满足当前回归要求：一旦出现 sequence gap/回退，后续条目全部不回放。
- `is_replay_required` 必须与 active sequence 一致；不能再以 `entries().is_empty()` 判定，否则会被可解析但无效/断链条目误触发。
- 保留 Task 5 的 GUID 严格拒绝策略同时避免噪声误判：仅对有效候选做 GUID 比对，然后在 matched 集合上再套 active sequence 前缀裁剪。

## Task 6: Data Descriptor leading/trailing 扇区合并语义修正

- **旧语义（错误）**：leading_bytes 和 trailing_bytes 被解释为零填充长度。replay 路径写 leading 零字节 + 扇区数据 + trailing 零字节；overlay 路径构建 `[trailing零][sector.data][leading零]`。
- **新语义（正确）**：leading_bytes = 目标范围开头需保留（不覆盖）的字节数，trailing_bytes = 目标范围末尾需保留的字节数。有效数据 = sector.data()[0..4084-leading-trailing]，写入 file_offset + leading。
- **双路径一致性**：磁盘回放（`replay_entries` in `log.rs`）和只读 overlay（`build_replay_overlay` in `file.rs`）必须同步更新，否则 InMemoryOnReadOnly 测试会使用旧语义。
- **边界校验**：leading_bytes + trailing_bytes > 4084 应返回 `Error::LogEntryCorrupted`，不能静默截断或零填充。
- **overflow 防御**：file_offset + leading 需要 checked_add，防止 u64 溢出。
- **misc/test-fs.vhdx 回归**：该样本文件包含 leading_bytes=4194310 的日志描述符（超出 4084），属于格式损坏。旧代码将其当作 4MB 零填充静默处理；新代码正确拒绝。test_open_test_fs_vhdx 改用 `ReadOnlyNoReplay` 策略绕过日志回放。
- **测试夹具**：`build_controllable_log_entry_bytes` 构建的条目默认 checksum=0，注入后必须调用 `fix_log_entry_checksum` 修正，否则 Task 4 precheck 会拒绝。

## [2026-04-24] Task 7 regression-fix

- `is_replay_required` 不能只看“有效 active sequence 是否非空”；若日志区存在 `"loge"` 候选但候选损坏，也必须返回 true，让打开流程进入 replay 并抛出 Task 4 腐坏错误，避免 silent-ignore。
- `entries_for_log_guid` 与 `replay` 严格路径中，对 `"loge"` 候选应直接执行 `validate_replay_candidate` 并传播错误；仅非 `"loge"` 噪声槽位可忽略。
- `inject_multi_entry_log_sequence` 这类多条目夹具写入时需在注入阶段写合法 checksum；否则 strict replay 会在第一条候选处因 CRC 失败而中断，掩盖 active-sequence 行为断言。

## [2026-04-24] Task 8: replay 后文件尺寸约束与 sections 刷新

- **`flushed_file_offset` 语义**（MS-VHDX §2.3.1.1）：entry header 偏移 48（u64 LE）。若文件实际长度 < `flushed_file_offset` 且 `flushed_file_offset > 0`，则该 entry 对应的回放前提不满足，应返回 `Error::LogEntryCorrupted`。`flushed_file_offset == 0` 表示无约束，跳过检查。
- **`last_file_offset` 语义**：entry header 偏移 56（u64 LE）。回放后文件必须至少延伸到所有 entry 中最大的 `last_file_offset`。实现为：预扫描全部 entry 取 max，回放完成后 `file.set_len(max_last_file_offset)`。
- **Sections 缓存失效**：`Sections` 使用 `RefCell<Option<T>>` 做延迟加载。回放会修改磁盘上的 header/BAT/metadata，若不清缓存，后续 `sections.header()` 等调用返回过期数据。新增 `Sections::invalidate_caches(&self)` 将四个 `RefCell` 重置为 `None`。
- **RefCell 借用陷阱**：`invalidate_caches()` 不能在 `sections.log()` 返回的 `Ref<'_, Log>` 仍存活时调用，否则 `RefCell already borrowed` panic。修复：将 `invalidate_caches()` 调用移到 log borrow scope 结束之后。
- **双路径一致性**：磁盘回放（`replay_entries`）和只读 overlay（`build_replay_overlay`）都执行 `flushed_file_offset` 检查；文件延伸仅在磁盘回放路径执行（overlay 不修改文件）。
- **测试夹具**：`build_controllable_log_entry_bytes` 不设置 `flushed_file_offset`/`last_file_offset`（全零），测试需手动写 `entry_bytes[48..56]` 和 `[56..64]` 并重算 checksum。

## Task 9: PartiallyPresent sector bitmap 判定读取 (completed)

- **PartiallyPresent 读路径**（MS-VHDX §2.5.1）：当 BAT 条目为 `PartiallyPresent` 时，读路径必须查阅 sector bitmap 逐扇区判定：bitmap=1 的扇区从子盘 payload 读取，bitmap=0 的扇区返回零（父盘回退是 Task 10）。
- **Bitmap BAT 索引公式**：完整 chunk 使用 `chunk_number * (chunk_ratio + 1) + chunk_ratio`；不完整（最后）chunk 使用 `chunk_number * (chunk_ratio + 1) + min(remaining_payload, chunk_ratio)`。必须用 `min` 而非固定 `chunk_ratio`，否则最后一个 chunk 的 bitmap BAT 索引会越界。
- **Bitmap 数据位置**：`bitmap_file_offset + (block_within_chunk * sectors_per_block) / 8`。每个 payload block 占 `sectors_per_block / 8` 字节的 bitmap 空间。
- **Bit 检查**：`bitmap_data[sector_index / 8] & (1 << (sector_index % 8))`，其中 `sector_index` 是 block 内的扇区偏移。
- **非对齐读取**：读取范围可能不与扇区边界对齐。使用 overlap 计算确定实际从 payload 读取的字节数：`overlap = min(read_end, sector_end) - max(read_start, sector_start)`。
- **Replay overlay 交互**：PartiallyPresent 块中 bitmap=1 的扇区仍需检查 replay overlay，与 FullyPresent 块行为一致。
- **`inject_sector_bitmap_bits` 前置条件**：该 helper 会先读取 bitmap 位置的数据再修改写入。调用前必须确保文件足够大且目标位置已写入初始零，否则 `read_exact` 会失败。
- **测试夹具参数**（1 MiB block, 4096 sector）：chunk_ratio=32768, payload_blocks=4, total BAT entries=5, BAT[4] 是 sector bitmap 条目。

## Task 11: Dynamic 未分配块自动分配与 BAT 更新 (completed)

- **自动分配触发条件**：`write_dynamic` 中 BAT 状态为 `NotPresent`、`Zero` 或 `Unmapped` 时触发自动分配。`FullyPresent` 和 `PartiallyPresent` 走已有路径。
- **分配策略**：新 payload block 偏移 = `align_1mib(file.metadata().len())`，确定性且与文件尾对齐。
- **文件扩展**：通过 seek-to-end + 写入单个零字节实现文件增长，避免大块零写入。
- **BAT 双路径更新**：内存更新通过 `Sections::bat_mut()` → `Bat::update_entry()`；磁盘持久化通过直接写 8 字节（`bat_disk_offset + index * 8`）跳过全量 BAT 序列化。
- **`bat_mut()` 生命周期**：返回 `RefMut<'_, Bat<'static>>` 以匹配存储的 `RefCell<Option<Bat<'static>>>` 类型，避免生命周期强制转换问题。
- **跨块写入**：`write_dynamic` 使用循环分段写入，与 `read_dynamic` 的 read 路径模式一致，正确处理跨块边界的写入请求。
- **chunk_ratio 换算**：`bat_payload_index = block_idx + (block_idx / chunk_ratio)`，与 read 路径一致。自动分配使用 payload index 而非 block index 定位 BAT 条目。
- **测试更新**：4 个原预期"写入未分配块返回错误"的测试更新为预期自动分配成功，包括持久化验证（关闭重开后读取）和多种 BAT 状态覆盖（NotPresent、Zero、Unmapped）。

## [2026-04-24] Task 10: 差分盘父链回退读取

- 差分读路径回退父盘应复用 `ParentLocator::resolve_parent_path` 既有优先级（`relative_path` → `volume_path` → `absolute_win32_path`），避免引入新顺序导致与校验器行为不一致。
- 为避免每个未命中扇区重复打开父盘，在 `File::read` 动态分支内引入按次读取缓存（`Option<File>`）即可满足性能和行为确定性。
- Task 10 回退触发点应覆盖四类路径：`NotPresent/Zero/Unmapped`、BAT 缺失（`None`）、`PartiallyPresent` 的 bitmap=0、以及 bitmap 条目缺失/非 Present；bitmap=1 的子盘数据路径保持不变。
- 父盘缺失/不可解析时应直接传播显式错误（`ParentNotFound` 或元数据错误），不能保留旧的静默零填充分支。

## [2026-04-24] Task 12: 全量验证收口

- **DataSector 序列号编码约定**：`sequence_high` 和 `sequence_low` 必须相同以通过撕裂写入检测。`sequence_number() = (high << 32) | low`，当 `high = low = seq32` 时结果为 `(seq32 << 32) | seq32`，对 u32 范围内的序列号不等于原始 `seq32`。因此**不能**用 `sequence_number()` 直接与描述符序列号比较——撕裂检测（`high != low`）才是正确的完整性守卫。
- **CRC-32C 校验实现**：`validate_log()` 中的 CRC 检查必须使用 `crc32c_with_zero_field(raw, 4, 4)` 将校验和字段本身清零后计算，与 `precheck_replay_entry` 语义一致。
- **日志 GUID 一致性**：校验器应从活动 header 读取 `log_guid`，在每个日志条目上验证匹配。非 nil 的 header GUID 才触发检查。
- **测试夹具维护**：注入可控日志条目后修改内容（如注入描述符数量错误）时，必须重新调用 `fix_log_entry_checksum` 以维持有效 CRC，否则 CRC 不匹配会先于预期错误被检测到。
- **验证器不应过度校验**：数据扇区的 `sequence_number()` 与描述符序列号精确相等检查因编码惯例不适用；保留撕裂写入检测（`high != low`）和描述符间序列一致性即可。

## [2026-04-25] F2: Code Quality Review

### Verdict: APPROVE (with follow-up recommendations)

### MAJOR Findings (non-blocking, follow-up recommended)

1. **Bat::new pre-parses entries with hardcoded chunk_ratio** (bat.rs:51-56)
   - Bat::new computes chunk_ratio from LOGICAL_SECTOR_SIZE_512/DEFAULT_BLOCK_SIZE instead of actual metadata values.
   - This causes bat.entries() to return incorrectly classified BatState for non-default configs (e.g., logical_sector_size=4096 or block_size=1MiB).
   - The runtime read/write paths in file.rs compute correct indices independently, so core IO is unaffected.
   - However, validation.rs::validate_bat() (line 236) iterates these pre-parsed entries and cross-checks with correctly computed expected_sector_bitmap, creating an inconsistency for non-default configs.
   - Fix: pass actual logical_sector_size and block_size to Bat::new (or defer pre-parse to first access with context).

### MINOR Findings

2. **ValidationIssue struct is dead code** (validation.rs:34-44)
   - Struct defined with section/code/message/spec_ref fields but never instantiated. All validators return Result via Error variants.
   - Consider removing or integrating into validator return types.

3. **unsafe in Log::entry() lacks formal SAFETY comment** (log.rs:118-119)
   - std::slice::from_raw_parts is sound (offset bounded by while loop, data_len = raw.len() - offset) but has no SAFETY block comment per Rust convention.

4. **Helper function duplication** (log.rs, metadata.rs)
   - read_array/read_u32/read_u64/read_guid defined independently in both modules. Should be consolidated into a shared utility.

5. **validate_required_metadata_items repetitive pattern** (validation.rs:329-373)
   - Five sequential blocks each iterating entries to check for a specific GUID. Could be a loop over a Guid array.

6. **CRC helper partial duplication** (log.rs:51 vs sections.rs:374)
   - calculate_log_entry_crc32c in log.rs is a specialized version of crc32c_with_zero_field in sections.rs.

### Pre-existing Warnings (confirmed NOT newly introduced)

- flush_raw dead code (file.rs:695) -- pre-existing
- read_sectors/write_sectors dead code (io_module.rs:73,103) -- pre-existing
- from_raw dead code (bat.rs:211) -- pre-existing
- 3 dead fixture helpers in integration tests -- test infrastructure, not production code

### Quality Metrics

- All 241 tests pass (36 lib + 32 api_smoke + 120 integration + 50 cli + 3 doctests)
- 0 new compiler warnings or errors
- 0 todo!() / unimplemented!() macros
- 0 TODO/FIXME/HACK comments in production code
- Error handling is thorough: checked_add/try_into throughout, no unwrap on user-controlled paths
- All new public types have /// Chinese doc comments per project convention
- CLI help attributes use English per project convention
- Test coverage: behavioral tests for all 12 tasks, not trivial pass-throughs
