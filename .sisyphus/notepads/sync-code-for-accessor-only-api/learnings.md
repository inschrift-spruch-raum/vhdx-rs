## 2026-05-01T20:21:00+08:00 Task: init
计划初始化：等待首个执行任务产出经验。

## Task-1 Inventory (2026-05-01)

### Key Findings
- 17 structs with pub fields, 72 total pub fields across 7 source files
- 45 fields already have accessor methods (62.5%)
- 18 fields need new accessors before migration
- 4 structs need public constructors (PayloadBlock, ParentChainInfo, ValidationIssue, KeyValueEntry)
- 3 type-change risks: creator (&[u8]→String), required (u32→bool), flags (u32→EntryFlags)
- DataSector/DataDescriptor/ZeroDescriptor .signature fields lack accessors but are used for byte comparison
- api_surface_smoke.rs is the heaviest direct-field consumer (smoke tests deliberately access all pub fields)

### Pattern: Dual field+method on same struct
Most structs in header.rs/metadata.rs/log.rs already have both pub field: T AND pub fn field() -> T.
The plan is to keep the methods, migrate callers, then make fields pub(crate) or private.

### Safe migration order confirmed
1. Add missing accessors (18 fields)
2. Add constructors (4 structs)
3. Migrate all call sites to use accessors
4. Make fields private
5. Full test run

## Task-2 Accessor Implementation (2026-05-01)

### Accessors Added (9 methods across 3 files)
1. **bat.rs** `BatEntry::state() -> BatState` — returns Copy type by value, `const fn`
2. **bat.rs** `BatEntry::file_offset_mb() -> u64` — returns Copy type by value, `const fn`
3. **metadata.rs** `TableHeader::reserved() -> &[u8; 2]` — returns reference, `const fn`, matches `signature()` pattern
4. **metadata.rs** `TableHeader::reserved2() -> &[u8; 20]` — returns reference, `const fn`, matches `signature()` pattern
5. **metadata.rs** `KeyValueEntry::key_offset() -> u32` — returns Copy type by value, `const fn`
6. **metadata.rs** `KeyValueEntry::value_offset() -> u32` — returns Copy type by value, `const fn`
7. **metadata.rs** `KeyValueEntry::key_length() -> u16` — returns Copy type by value, `const fn`
8. **metadata.rs** `KeyValueEntry::value_length() -> u16` — returns Copy type by value, `const fn`
9. **log.rs** `DataSector::signature() -> &[u8; 4]` — returns reference, `const fn`, matches `LogEntryHeader::signature()` pattern

### Style Consistency
- All accessors use `#[must_use]` attribute
- All are `const fn` (no runtime logic)
- Chinese `///` doc comments with MS-VHDX section references
- Return types match existing module conventions: primitives by value, arrays by reference



## Task-3 Public Constructor Migration (2026-05-01)

### Constructors Added (5 types, 6 methods)

1. **ParentChainInfo::new(child, parent, linkage_matched)** — src/file.rs
   - Simple field-forwarding constructor with #[must_use]
2. **ValidationIssue::new(section, code, message, spec_ref)** — src/validation.rs
   - Field-forwarding, #[must_use]
3. **PayloadBlock::new(bytes)** — src/io_module.rs
   - const fn + #[must_use], minimal wrapper
4. **KeyValueEntry::from_parts(key_offset, value_offset, key_length, value_length)** — src/sections/metadata.rs
   - Named rom_parts to distinguish from existing 
ew(data) which parses from 12 bytes
   - Sets aw: &[] since callers using rom_parts don't need key()/alue() methods
5. **BatEntry::create(state, file_offset_mb)** — src/sections/bat.rs
   - Named create to avoid collision with existing pub(crate) new
   - Identical implementation to 
ew, just public

### Constructor Naming Strategy
- **
ew** — used when no existing 
ew exists or when it's the primary construction path
- **rom_parts** — used when an existing 
ew already parses from raw bytes (KeyValueEntry)
- **create** — used when an existing pub(crate) new exists and making it public would change visibility (BatEntry)

### Test Migration Pattern
All struct literal constructions replaced with constructor calls:
- pi_surface_smoke.rs: 6 sites migrated (ParentChainInfo x1, ValidationIssue x2, BatEntry x1, PayloadBlock x3)
- src/sections/metadata.rs unit test: 1 site migrated (KeyValueEntry x1)
- No changes needed in integration_test.rs -- it doesn't use struct literals for these types

### Verification
- cargo test --test api_surface_smoke: 28/28 passed
- cargo test --test integration_test: 160/160 passed
- All LSP diagnostics clean on changed files

## Task-4 Guardrail Evidence (2026-05-01)

### Key Findings
- 将范围约束写成独立证据文件后，后续任务可直接引用，不需要重复口头同步。
- 白名单文件同时包含允许集合与当前 diff 快照，便于审计时快速定位越界来源。
- 将“结论 + 原因”固定为末尾段落，有利于后续自动化解析。

### Reusable Pattern
- 先锁硬约束，再列白名单，再贴实际 diff，最后给 PASS/FAIL。
- 对于阶段性计划，白名单要按阶段滚动更新，避免误报历史改动。

## Task-5 validation.rs Field→Accessor Migration (2026-05-01)

### Sites Migrated (4 structs, 16 call sites)
1. **BatEntry** in validate_bat: `entry.state` → `entry.state()`, `entry.file_offset_mb` → `entry.file_offset_mb()` (8 sites in sector-bitmap + payload branches)
2. **TableHeader** in validate_metadata: `table_header.reserved` → `table_header.reserved()`, `table_header.reserved2` → `table_header.reserved2()` (2 sites, comparison changed from `[0u8; N]` to `&[0u8; N]`)
3. **DataSector** in validate_log: `sector.signature != *b"data"` → `sector.signature() != b"data"` (1 site, dereference removed since accessor returns `&[u8; 4]`)
4. **KeyValueEntry** in validate_parent_locator: `entry.key_length/value_length/key_offset/value_offset` → corresponding accessors (5 sites in error messages + guards)

### Pattern: Reference vs Value Adjustment
- Accessors returning `&[u8; N]` require comparison targets adjusted: `[0u8; 2]` → `&[0u8; 2]`, `*b"data"` → `b"data"` (both are `&[u8; 4]`).
- Accessors returning Copy types (`BatState`, `u64`, `u32`, `u16`) are drop-in replacements with `()` suffix only.

## Task 6: Migrate src/file.rs BatEntry direct field access to accessor methods

### Date: 2026-05-01

### Changes Made
- Replaced 3 direct .state field accesses with .state() method calls in src/file.rs:
  1. Line ~361 (read method, dynamic BAT entry match): &entry.state → ntry.state()
     - Note: removed & reference since state() returns a Copy value
  2. Line ~399 (bitmap_offset closure, matches! macro): e.state → e.state()
  3. Line ~635 (get_or_allocate_block method): ntry.state → ntry.state()

### Key Observations
- ile_offset() and ile_offset_mb were already using accessor methods in file.rs; only .state needed migration
- BatEntry::state() returns BatState (Copy type), so removing & reference is safe
- The matches! macro works identically with method call vs field access
- No test changes required — all 160 integration tests passed
- lsp_diagnostics showed zero errors after edits
## Task-7 Integration Test Field→Accessor Migration (2026-05-01)

### Sites Migrated (4 structs, 20 call sites)
1. **BatEntry** in test_bat_entries_vec_traversable: `entry.state → ntry.state() (2 sites)
2. **BatEntry** in fixed_bat_sector_bitmap_notpresent: ntry.state → ntry.state(), ntry.file_offset_mb → ntry.file_offset_mb() (4 sites)
3. **BatEntry** in test_read_dynamic_4096_sector_consistent_with_bat_state: itmap_entry.state → itmap_entry.state() (1 site)
4. **LocatorHeader** in test_locator_header_public_fields_accessible, test_parent_locator_api_surface, test_parent_locator_empty_entries_and_data, test_t9_section_module_import_paths: header.key_value_count → header.key_value_count() (4 sites)
5. **KeyValueEntry** in test_locator_header_public_fields_accessible: kv.key_offset → kv.key_offset() (2 sites)
6. **KeyValueEntry** in test_key_value_entry_public_fields_accessible: ntry.key_offset/value_offset/key_length/value_length → accessor calls (4 sites)
7. **KeyValueEntry** in test_spec_decision_manifest: ntry.key_offset/value_offset/key_length/value_length as usize → accessor calls (4 sites)

### Not Migrated (no accessor exists)
- Sector.block_sector_index — no accessor method
- PayloadBlock.bytes — no accessor method
- Sector.payload — has payload() accessor but test 	est_sector_public_fields_accessible explicitly tests field+method equivalence (line 1631); replacing field with method would weaken the assertion

### Verification
- lsp_diagnostics: 0 errors on integration_test.rs
- cargo test --test integration_test: 160/160 passed

## Task-8 api_surface_smoke.rs Field→Accessor Migration (2026-05-01)

### Accessors Added in src/ (9 new accessors, compilation-required)
1. **file.rs** `ParentChainInfo::linkage_matched() -> bool` — const fn, #[must_use]
2. **validation.rs** `ValidationIssue::section() -> &'static str` — const fn, #[must_use]
3. **validation.rs** `ValidationIssue::code() -> &'static str` — const fn, #[must_use]
4. **io_module.rs** `Sector::block_sector_index() -> u32` — const fn, #[must_use]
5. **io_module.rs** `PayloadBlock::bytes() -> &'a [u8]` — const fn, #[must_use]
6. **log.rs** `DataDescriptor::signature() -> &[u8; 4]` — const fn, #[must_use]
7. **log.rs** `ZeroDescriptor::signature() -> &[u8; 4]` — const fn, #[must_use]

### Sites Migrated in api_surface_smoke.rs (13 test functions, ~50 call sites)
1. **ParentChainInfo::linkage_matched** → `linkage_matched()` (line 96)
2. **ValidationIssue::section** → `section()` (line 110)
3. **BatEntry::file_offset_mb** → `file_offset_mb()` (line 163)
4. **ValidationIssue::code** → `code()` (line 247)
5. **FileTypeIdentifier::signature/creator** → `signature()`/`creator()` (lines 430-431), type changed from `[u8; 8]` to `&[u8; 8]`, from `&[u8]` to `String`
6. **HeaderStructure** 10 fields → methods (lines 435-444), all return types by value/reference as per accessor signatures
7. **RegionTable::header/entries** → `header()`/`entries()` (lines 450-451), entries type changed from `&[...]` to `Vec<...>`
8. **RegionTableHeader** signature/checksum/entry_count → methods (lines 455-457)
9. **RegionTableEntry** guid/file_offset/length/required → methods (lines 460-464), required() returns bool
10. **TableHeader** signature/entry_count → methods (lines 481-482)
11. **TableEntry** item_id/offset/length/flags → methods (lines 486-494)
12. **BatEntry::state/file_offset_mb** → methods (lines 526-539)
13. **DataDescriptor** signature/trailing_bytes/leading_bytes/file_offset/sequence_number → methods (lines 566-570)
14. **ZeroDescriptor** signature/zero_length/file_offset/sequence_number → methods (lines 573-576)
15. **DataSector** signature/sequence_high/data/sequence_low → methods (lines 583-586)
16. **Sector::block_sector_index** → method (lines 370, 605)
17. **Sector::payload + PayloadBlock::bytes** → `payload()` + `bytes()` (lines 607-609), removed dual field/method pattern

### Not Migrated (no struct field access remaining)
- EntryFlags(0x8000_0000) — tuple struct construction, not named-field struct literal
- Error::InvalidChecksum { expected, actual } etc. — enum variants, not affected by field privatization

### Pattern: Return type changes when migrating to accessors
- `[u8; N]` field → `&[u8; N]` accessor (signature, checksum, etc.)
- `&[u8]` field → `String` accessor (FileTypeIdentifier::creator)
- `u32` field → `bool` accessor (RegionTableEntry::required)
- `Vec<T>` accessor instead of `&[T]` field (RegionTable::entries)

### Verification
- lsp_diagnostics: 0 errors on all changed files
- cargo test --test api_surface_smoke: 28/28 passed
- cargo test --workspace: 294/294 passed (48 unit + 28 smoke + 160 integration + 55 CLI + 3 doctests)

## Task-9 Privatize Fields (2026-05-01)

### Fields Privatized (89 fields across 17 structs)
All `pub field: Type` changed to `field: Type` in:
- header.rs: FileTypeIdentifier(3), HeaderStructure(11), RegionTable(2), RegionTableHeader(5), RegionTableEntry(5)
- bat.rs: BatEntry(2)
- metadata.rs: TableHeader(5), TableEntry(6), FileParameters(3), LocatorHeader(4), KeyValueEntry(5)
- log.rs: LogEntryHeader(11), DataDescriptor(6), ZeroDescriptor(6), DataSector(5)
- io_module.rs: Sector(2), PayloadBlock(1)
- file.rs: ParentChainInfo(3)

## Task-11 Remediation Evidence Backfill (2026-05-01)

### Key Learnings
- 证据补档要优先保留“命令行 + 可复现筛选参数 + 结果摘要 + 退出码”，审核可直接复跑。
- 针对 error-path 证据，使用测试名过滤前缀（例如 `t12_validator_log_rejects`）比单测全量更稳定，覆盖面也清晰。
- final scope mapping 最好按三类固定落位：planned migration file、formatting-only side effect、orchestration tracking file，评审定位最快。

- validation.rs: ValidationIssue(4)

### New Accessors Added (2 methods)
1. **file.rs** `ParentChainInfo::child() -> &Path` — returns reference, #[must_use]
2. **file.rs** `ParentChainInfo::parent() -> &Path` — returns reference, #[must_use]

### Compilation Fixes
1. **validation.rs**: `ParentChainInfo { child, parent, linkage_matched }` struct literal → `ParentChainInfo::new(child, parent, linkage_matched)` constructor call
2. **integration_test.rs**: 25 direct field accesses migrated to accessor calls:
   - Sector.block_sector_index → block_sector_index() (7 sites)
   - Sector.payload → payload() (3 sites)
   - PayloadBlock.bytes → bytes() (3 sites)
   - ValidationIssue.section/code → section()/code() (2 sites)
   - ParentChainInfo.child/parent/linkage_matched → child()/parent()/linkage_matched() (5 sites)
   - KeyValueEntry.raw → raw() (2 sites)
   - BatEntry.state → state() (3 sites)
3. **integration_test.rs**: 3 struct literal constructions migrated to constructors:
   - PayloadBlock { bytes } → PayloadBlock::new(bytes) (3 sites)
   - ValidationIssue { section, code, message, spec_ref } → ValidationIssue::new(...) (1 site)
   - ParentChainInfo { child, parent, linkage_matched } → ParentChainInfo::new(...) (1 site)
   - KeyValueEntry { key_offset, value_offset, ... } → KeyValueEntry::from_parts(...) (2 sites)

### dead_code Warnings Suppressed
Added `#[allow(dead_code)]` on reserved/unused private fields:
- RegionTable::header, entries (re-parsed from data rather than stored fields)
- RegionTableHeader::reserved
- LogEntryHeader::reserved
- ZeroDescriptor::reserved
- TableEntry::reserved
- LocatorHeader::reserved

### Pattern: Field privatization reveals test debt
T7 (integration test migration) left ~25 direct field accesses and ~7 struct literal constructions un-migrated.
These only surfaced after the visibility gate was applied. The privatization step acts as a compiler-enforced audit.

### Verification
- lsp_diagnostics: 0 errors on all 7 changed source files
- cargo build --workspace: PASS
- cargo test --workspace: 294/294 passed
- [2026-05-01T21:28:46.4803275+08:00] T10质量门执行要点：在同一证据文件中按固定顺序记录 build→test→clippy→fmt，并为每道门保留命令、起止时间、完整输出、退出码与汇总结论，保证可审计追溯。
