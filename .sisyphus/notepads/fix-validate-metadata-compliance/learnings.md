# Learnings — fix-validate-metadata-compliance

## 2026-04-30 Task 1: Metadata Table-Header Validation

### Pattern: Validator early-return style
- All validators in `src/validation.rs` follow the same pattern: obtain section → check constraints → early-return `Err(Error::InvalidMetadata(msg))` on violation.
- `validate_region_table()` is the closest analog for table-header checks (signature + entry count + reserved).
- No checksum field exists in metadata table header (MS-VHDX §2.6.1.1), unlike region table headers.

### Key constants
- `METADATA_SIGNATURE` = `b"metadata"` (8 bytes) — defined in `src/common/constants.rs`
- `METADATA_TABLE_SIZE` = 64KB — metadata table is fixed-size
- Max entry count: 2047 (from MS-VHDX §2.6.1.1)
- Table header: 32 bytes (signature 8 + reserved 2 + entry_count 2 + reserved2 20)

### TableHeader struct fields are public
- `TableHeader.reserved` and `TableHeader.reserved2` are public `[u8]` arrays, can be directly compared.
- `TableHeader.signature()` returns `&[u8; 8]`, `entry_count()` returns `u16`.

### Test strategy
- Existing integration tests (test_t8_*, test_validate_file_*) already exercise `validate_metadata()` through `validate_file()` for valid files.
- Dedicated invalid-signature test deferred to Task 4 (test-adding task) since it requires raw byte manipulation of VHDX file at metadata region offset.

## 2026-04-30 Task 2: Metadata Entry Structural Validation

### Entry structure (32 bytes, MS-VHDX §2.6.1.2)
- `item_id`: 16 bytes (GUID) at offset 0
- `offset`: 4 bytes (u32 LE) at offset 16 — data offset relative to metadata region start
- `length`: 4 bytes (u32 LE) at offset 20 — data length in bytes
- `flags`: 4 bytes (u32 LE) at offset 24
- `reserved`: 4 bytes (u32 LE) at offset 28

### Bounds baseline: `metadata.raw().len()`
- `Metadata::raw()` returns the full metadata region raw data (Vec<u8> length).
- Used as the upper bound for `entry.offset() + entry.length()` checks.

### Duplicate detection pattern
- Follows `validate_region_table()` loop style: nested loops with `skip(index + 1)` for O(n²) pairwise comparison.
- Compares `entry.item_id()` for duplicates.

### Overlap detection
- Uses half-open interval `[start, start+length)` for each entry.
- Two entries overlap iff `a_start < b_end && b_start < a_end`.
- Uses `saturating_add` for overflow safety with u64 conversion from u32.

### Test gotcha: modifying first entry breaks File::open()
- `File::open()` reads file_parameters (entry[0]) during metadata parsing.
- Setting entry[0] length=0 or offset out-of-range causes `File::open()` to fail before validator runs.
- **Solution**: Modify the LAST entry (physical_sector_size, index `entry_count-1`) for zero-length and out-of-range tests.
- Modifying last entry's item_id to match entry[0]'s item_id also works for duplicate test (since `get_item_data` finds first match).
- The overlap test modifies entry[1]'s offset/length — safe because `File::open()` accesses items by GUID, and entry[1]'s data (virtual_disk_size) still resolves correctly since the test doesn't change entry[0]'s offset.

## 2026-04-30 Task 3: Known Metadata Item Semantic Constraints

### validate_metadata ordering kept deterministic
- Structural metadata checks (table header / bounds / duplicate / overlap) stay first.
- Known-item semantic checks are appended after structural checks in SpecValidator::validate_metadata().
- Cross-item consistency check (physical >= logical) is executed after local item allowlist checks.

### Implemented known-item semantics
- FileParameters.block_size: must be in [MIN_BLOCK_SIZE, MAX_BLOCK_SIZE] and power-of-two.
- logical_sector_size / physical_sector_size: allowlist is {512, 4096}.
- virtual_disk_size: must be > 0, <= 64 TiB, and aligned to logical sector size when present.
- Violations return Error::InvalidMetadata(...) with concise contextual English messages.

### Test fixture pattern for semantic-value mutations
- Mutate metadata data payload bytes (not entry header structure) to ensure File::open(strict=false) still succeeds and validator path is exercised.
- Added reusable helpers:
  - mutate_known_metadata_u32(path, item_id, value)
  - mutate_known_metadata_u64(path, item_id, value)
- Helpers locate the target item by GUID via metadata table scan, then overwrite bytes at entry data offset.

## 2026-04-30 Task 4: Metadata validation test-scope completeness

### Added scope-completeness tests in integration_test.rs
- Added 	est_validate_metadata_rejects_invalid_table_signature to explicitly cover metadata table/header invalid-path rejection in alidate_metadata().
- Added 	est_validate_metadata_scope_boundary_required_item_completeness_is_separate to prove boundary ownership: alidate_metadata() passes while alidate_required_metadata_items() rejects missing physical_sector_size.

### Coverage mapping after Task 4
- valid metadata path: 	est_validate_metadata_entry_constraints_happy, 	est_validate_metadata_entry_constraints_dynamic_happy, 	est_validate_metadata_known_items_happy.
- table/header invalid cases: 	est_validate_metadata_rejects_invalid_table_signature.
- entry invalid cases: duplicate/zero-length/out-of-range/overlapping tests under 	est_validate_metadata_rejects_*.
- known-item invalid cases: 	est_validate_metadata_rejects_invalid_known_item_values.
- boundary proof: required-item completeness remains in alidate_required_metadata_items() path and is not duplicated in alidate_metadata().


## 2026-04-30 Task 5: Workspace-level verification evidence
- Timestamp: 2026-04-30 17:54:15
- Executed required sequence successfully: cargo test -p vhdx-rs test_validate_metadata, cargo test --workspace, cargo clippy --workspace.
- Capturing per-command logs plus aggregated happy-path output in .sisyphus/evidence/ provides traceable evidence for final verification wave.

