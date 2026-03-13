# VHDX Refactor Learnings

## Wave 1 Task 1: common/ Module Creation

**Date:** 2026-03-14

### What was done:
- Created `src/common/` directory
- Copied `src/guid.rs` to `src/common/guid.rs` (original retained)
- Copied `src/crc32c.rs` to `src/common/crc32c.rs` (original retained)
- Created `src/common/disk_type.rs` with `DiskType` enum extracted from `vhdx.rs`
- Created `src/common/mod.rs` exporting all submodules
- Added `pub mod common;` to `src/lib.rs`

### Design decisions:
- `disk_type.rs` contains `DiskType` enum (Fixed, Dynamic, Differencing) with `Display` trait implementation
- Original files (`guid.rs`, `crc32c.rs`) retained in `src/` to avoid breaking existing imports
- Module exports: `Guid`, `crc32c`, `crc32c_with_zero_field`, `DiskType`
- `DiskType::Display` implementation provides user-friendly string formatting

### Files created:
- `src/common/mod.rs` - Module entry point with public exports
- `src/common/guid.rs` - GUID handling (copied from src/guid.rs)
- `src/common/crc32c.rs` - CRC-32C checksum utilities (copied from src/crc32c.rs)
- `src/common/disk_type.rs` - DiskType enum (new, extracted from vhdx.rs)

### Verification:
- `cargo check --lib` completed successfully
- No new warnings introduced
- All common utilities now accessible via `crate::common`

## Wave 1 Task 2: utils/ Module Creation

**Date:** 2026-03-14

### What was done:
- Created `src/utils/` directory
- Created `src/utils/mod.rs` as the module entry point
- Added `pub mod utils;` to `src/lib.rs`
- Re-exported common utilities: `crc32c`, `crc32c_with_zero_field`, and `Guid`

### Design decisions:
- Instead of duplicating code, `utils/mod.rs` re-exports existing utilities from `crc32c.rs` and `guid.rs`
- This allows other modules to import from `crate::utils` instead of directly from `crate::crc32c` or `crate::guid`
- The original modules (`crc32.rs`, `guid.rs`) remain in place to avoid breaking existing code

### Findings:
- No generic utility functions needed to be moved - existing structure is already well-organized
- `crc32c.rs` contains CRC-32C checksum utilities used across the codebase
- `guid.rs` contains GUID handling used by multiple modules
- Both are now accessible via `crate::utils` for convenience

### Verification:
- `cargo build` completed successfully
- Only pre-existing warnings remain (unused imports/variables in other files)

## Wave 1 Task 3: header/ Module Creation

**Date:** 2026-03-14

### What was done:
- Created `src/header/` directory
- Created `src/header/file_type.rs` with `FileTypeIdentifier` struct and `FILE_TYPE_SIGNATURE` constant
- Created `src/header/header.rs` with `VhdxHeader` struct, `HEADER_SIGNATURE`, and dual-header functions (`read_headers`, `update_headers`)
- Created `src/header/region_table.rs` with `RegionTable`, `RegionTableHeader`, `RegionTableEntry`, `BAT_GUID`, `METADATA_GUID`, `REGION_SIGNATURE`, and `read_region_tables` function
- Created `src/header/mod.rs` exporting all submodules and re-exporting key types
- Updated `src/lib.rs` to remove `pub mod region;` (RegionTable moved to header/)
- Updated `src/vhdx.rs` to import from `crate::header` instead of `crate::region`

### Design decisions:
- Split original `src/header.rs` into three focused modules:
  - `file_type.rs`: File signature validation ("vhdxfile") and creator string handling
  - `header.rs`: VHDX header with dual-header mechanism, SequenceNumber, CRC-32C checksum
  - `region_table.rs`: Region table structures (moved from `src/region.rs`)
- Region Table code was moved into header module since it's logically part of the header section per MS-VHDX spec
- All original logic preserved - no validation or parsing changes
- `region.rs` file removed from lib.rs exports (no longer needed as standalone module)
- `vhdx.rs` imports consolidated to use `crate::header::*` for header-related types

### Files created:
- `src/header/mod.rs` - Module entry point with public exports
- `src/header/file_type.rs` - FileTypeIdentifier with signature validation
- `src/header/header.rs` - VhdxHeader with dual-header support and checksum verification
- `src/header/region_table.rs` - RegionTable, RegionTableHeader, RegionTableEntry

### Files modified:
- `src/lib.rs` - Removed `pub mod region;`
- `src/vhdx.rs` - Updated imports from `crate::region::` to `crate::header::`

### Verification:
- `cargo check --lib` completed successfully
- 8 warnings (all pre-existing, unrelated to this refactor)
- All header/region types now accessible via `crate::header`

## Wave 1 Task 6: BAT Module Refactoring

**Date:** 2026-03-14

### What was done:
- Created `src/bat/` directory structure
- Created `src/bat/mod.rs` as module entry point, exporting entry, states, table submodules
- Created `src/bat/entry.rs` with `BatEntry` struct and bit operations
- Created `src/bat/states.rs` with `PayloadBlockState` and `SectorBitmapState` enums
- Created `src/bat/table.rs` with `Bat` struct and chunk ratio calculations
- Original `src/bat.rs` retained (not deleted per requirements)

### Design decisions:
- **Entry layout (64-bit)**: State (bits 0-2, 3 bits) + Reserved (bits 3-19, 17 bits) + FileOffsetMB (bits 20-63, 44 bits)
- **PayloadBlockState enum**: NotPresent(0), Undefined(1), Zero(2), Unmapped(3), FullyPresent(6), PartiallyPresent(7)
- **SectorBitmapState enum**: NotPresent(0), Present(6)
- **Chunk ratio formula**: (2^23 * LogicalSectorSize) / BlockSize
- **BAT index formulas**:
  - Payload: `bat_index = chunk_index * (chunk_ratio + 1) + block_in_chunk`
  - Sector bitmap: `bat_index = chunk_idx * (chunk_ratio + 1) + chunk_ratio`
- **Bit operations**: Using masks `0x7` for state, `0xFFFFFFFFFFF` for 44-bit offset

### Files created:
- `src/bat/mod.rs` - Module entry point with public exports
- `src/bat/entry.rs` - BatEntry struct with from_raw(), new(), file_offset(), to_bytes()
- `src/bat/states.rs` - PayloadBlockState and SectorBitmapState enums with from_bits()/to_bits()
- `src/bat/table.rs` - Bat struct with chunk calculations, index lookups, and translation methods

### Module exports:
- `BatEntry` - 64-bit BAT entry with state and file offset
- `PayloadBlockState` - Block allocation states for payload blocks
- `SectorBitmapState` - Block allocation states for sector bitmap blocks
- `Bat` - Block Allocation Table with entries and chunk calculations

### Tests passed:
- `bat::entry::tests::test_bat_entry` - Entry creation and parsing
- `bat::table::tests::test_bat_index_calculation` - BAT index calculations for payload and sector bitmap blocks
- `bat::table::tests::test_chunk_calculation` - Chunk ratio and size calculations

### Verification:
- Module compiles successfully with `cargo test bat --lib`
- All 3 BAT tests pass
- Original `src/bat.rs` preserved for backward compatibility (renamed temporarily during testing)
