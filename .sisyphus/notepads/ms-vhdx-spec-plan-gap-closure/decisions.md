## 2026-04-26T08:38:00Z Task: init
Notepad initialized.

## 2026-04-26T16:45:00Z Task: 1

### Decision 1: header-session
- **Interpretation**: `open(write)` triggers exactly one header session-init update: increment `sequence_number` by 1, set `file_write_guid` to new random GUID.
- **Current state**: Not implemented. `sequence_number` starts at 0 on creation and is never updated on open.
- **Locked by**: `test_spec_decision_manifest` — asserts header fields are readable and non-nil GUIDs, logs initial sequence_number. Follow-up Task must assert increment behavior.

### Task 2 outcome: header-session (IMPLEMENTED)
- `init_session_header()` correctly implements Decision 1 semantics.
- `sequence_number` incremented by exactly 1 per writable-open.
- `file_write_guid` regenerated via `uuid::Uuid::new_v4()` on each writable-open.
- `data_write_guid` preserved (not modified during session init).
- Only non-current header slot is written (alternating write pattern).
- Create path triggers one session-init; subsequent writable-open triggers another.

### Decision 2: parent_linkage2 forbidden in strict mode
- **Interpretation**: In strict mode, `parent_linkage2` key in Parent Locator is forbidden — its presence should trigger `InvalidMetadata`.
- **Current state**: `parent_linkage2` is accepted as optional key; only invalid GUID values are rejected.
- **Locked by**: `test_parent_locator_strict_rejects_parent_linkage2` — asserts `InvalidMetadata` with `parent_linkage2` in error text (currently via invalid GUID injection). Full rejection requires src/ change.

### Decision 3: entry offset/length semantics
- **Interpretation**: `key_offset`/`value_offset` in KeyValueEntry are relative to the key_value_data region (not the whole metadata item). `key_length` and `value_length` must each be > 0.
- **Current state**: Correctly implemented in both `build_parent_locator_payload()` and `KeyValueEntry` parsing.
- **Locked by**: `test_spec_decision_manifest` — asserts bounds and >0 for all entries.
### Decision 4: replay header lifecycle (IMPLEMENTED)
- After log replay, the non-active header is updated with: sequence_number = active_seq + 1, new file_write_guid, log_guid = nil.
- Only the non-active header slot is written (alternating write policy), matching init_session_header() pattern.
- The subsequent init_session_header() call (line 790) then writes the OTHER header slot with seq+1 again and another new file_write_guid.
- Total sequence increment per writable+Auto open with pending logs: +2 from pre-replay baseline.

### Decision 5: ReadOnlyNoReplay is non-writing
- ReadOnlyNoReplay policy returns has_pending_logs=true without any disk modification.
- Caller can observe pending state via has_pending_logs() and header sections API, but on-disk data is untouched.
- This is the correct semantics for a diagnostic/inspection mode.

## 2026-04-29T16:00:00Z Task: 4

### Decision 6: header lifecycle matrix invariants (LOCKED BY TEST)
- Writable open: sequence += 1, file_write_guid = new random, data_write_guid = unchanged.
- Readonly open: no header mutation at all (sequence, file_write_guid, data_write_guid all preserved).
- These invariants are now regression-locked by 3 dedicated tests in Task 4.

## 2026-04-29T20:00:00Z Task: 5

### Decision 7: LOCATOR_TYPE_VHDX as locator header GUID (IMPLEMENTED)
- Parent locator payload header bytes 0-15 must be the VHDX Locator Type GUID: B04AEFB7-D19E-4A81-B789-25B8E9445913.
- Reserved field (bytes 16-17) must be 0.
- These are now locked by 	est_create_diff_parent_locator_has_vhdx_locator_type.

### Decision 3 reaffirmed: entry offsets relative to key_value_data
- Confirmed existing implementation is correct: key_offset/
alue_offset relative to kv_data region start.
- key_length/
alue_length are always > 0 (since keys and values are non-empty strings).
- Locked by 	est_parent_locator_rejects_zero_offsets_or_lengths.

### Decision: Task 6 parent locator strict validation rules
- locator_type 必须等于 LOCATOR_TYPE_VHDX（B04AEFB7-D19E-4A81-B789-25B8E9445913）
- key_offset/value_offset 无需显式检查 > 0（因为 key_length/value_length > 0 隐含键/值区域非空，且 key() 解码失败时已返回错误）
- key_length 和 value_length 必须严格 > 0（MS-VHDX §2.6.2.6.2）
- 键唯一性通过 HashSet 检查，检测重复键时立即拒绝
- parent_linkage 必须存在且为有效 GUID
- 至少一个路径键存在（relative_path | volume_path | absolute_win32_path）
- 所有错误统一使用 InvalidMetadata 变体，错误文本包含具体违规字段/键名

## 2026-04-29T21:10:00Z Task: 8
### Decision 8: ReadOnlyNoReplay compatibility-exception wording (LOCKED BY DOC+TEST)
- ReadOnlyNoReplay 被定义为兼容模式例外，仅用于只读诊断/兼容场景。
- 该策略不是严格规范路径，存在 pending log 时应保留 pending 状态且不触发回放写入。
- 严格路径由 Require/Auto/InMemoryOnReadOnly 承担，Require 在 pending log 下继续返回 LogReplayRequired。


## 2026-04-29T22:00:00Z Task: 7

### Decision 8: validate_parent_chain 为 SINGLE-HOP ONLY（LOCKED BY TEST）
- 仅校验 child -> direct parent 的 DataWriteGuid 一致性
- 不递归、不检测循环、不遍历多级链
- 三条错误路径回归锁定：
  - happy path: linkage_matched=true, child/parent 路径正确
  - mismatch: Error::ParentMismatch { expected, actual }
  - not-found: Error::ParentNotFound 或 Error::Io
- 由 test_validate_parent_chain_single_hop_happy/mismatch/parent_not_found 回归保护

## 2026-04-29 Task: 9
Decision: Confirmed targeted gap suite (header session fields + parent locator strict valid) is stable and repeatable across two consecutive identical runs. No code changes needed.

## 2026-04-29 Task: 10 — Final regression gate

### Decision: Region table injection must target active RT
- After `File::create()`, h1.seq=1 > h2.seq=0, so `region_table(0)` returns RT1 (192KB).
- Test helpers that corrupt/inject into region tables must determine which RT is active at runtime, or target RT1 for post-create scenarios.
- Minimal fix: changed offset from 256KB to 192KB in both injection helpers.

### Decision: Test assertions must match validation check ordering
- When multiple validation checks exist, the FIRST failing check determines the error message.
- Tests should assert on the actual error output, not on the theoretical error that a deeper check would produce.
## 2026-04-29 Task: F4 Scope Fidelity Check (deep)

### Scope mapping (changed file -> plan task)
- src/file.rs -> Task 2 (writable-open header session), Task 3 (replay header lifecycle), Task 5 (parent locator payload)
- src/validation.rs -> Task 6 (parent locator strict validation), Task 7 (single-hop parent chain boundary)
- 	ests/integration_test.rs -> Tasks 1/2/3/4/5/6/7/8/9/10 regression coverage
- README.md -> Task 8 (ReadOnlyNoReplay compatibility exception wording)
- docs/API.md -> Task 8 (ReadOnlyNoReplay compatibility exception wording)
- hdx-cli/tests/cli_integration.rs -> regression alignment for existing CLI behavior (no command-surface expansion)

### Guardrail checks
- misc/ modifications: none
- Public API signature expansion/change: none detected in src/file.rs public signatures and docs alignment
- BAT strategy change: none detected
- CLI command surface expansion (hdx-cli/src/cli.rs, hdx-cli/src/commands/*): none

### Out-of-scope modifications detected
- .sisyphus/plans/ms-vhdx-spec-plan-gap-closure.md was edited (checkbox flips). Plan file is read-only by process rule.
- .sisyphus/notepads/ms-vhdx-spec-plan-gap-closure/{issues.md,learnings.md,problems.md} and .sisyphus/boulder.json changed; not part of plan deliverables.

### Decision
F4 VERDICT: REJECT
Reason: functional/code deliverables stay within scope, but process boundary was violated by direct edits to sacred plan file; plus unrelated metadata/notepad drift present in working tree.
## 2026-04-29T23:20:00Z Task: F4 Scope Fidelity Check (deep) — corrected boundary rules

### Re-evaluation basis
- Reused the same evidence set (changed-file list + previously reviewed product diffs/mappings).
- Applied corrected boundary constraints: plan checkbox updates and operational artifacts are excluded from product-scope violations.

### Guardrail status (product scope only)
- No edits under misc/: PASS
- No CLI command surface expansion (hdx-cli/src/cli.rs, hdx-cli/src/commands/* unchanged): PASS
- No public API signature changes (File/OpenOptions/CreateOptions/exported API shape unchanged): PASS
- No BAT strategy expansion beyond plan boundary: PASS

### Product-scope conclusion
- Product deliverable changes (src/file.rs, src/validation.rs, 	ests/integration_test.rs, README.md, docs/API.md, hdx-cli/tests/cli_integration.rs) map to plan Tasks 2/3/5/6/7/8 + regression gates, with no extra feature surface.

### Decision
F4 VERDICT: APPROVE
