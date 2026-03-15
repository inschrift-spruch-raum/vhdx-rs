## Metadata Whitelist Implementation (Task 2)

**Date**: 2026-03-15
**Task**: Add known required metadata whitelist validation

### Implementation Details

1. **Whitelist defined**: `KNOWN_REQUIRED_METADATA_GUIDS` const array with all 6 known required metadata GUIDs
   - File Parameters, Virtual Disk Size, Virtual Disk ID
   - Logical Sector Size, Physical Sector Size, Parent Locator

2. **Validation added**: In `MetadataRegion::from_bytes()`, checks each entry:
   - If `entry.is_required && !KNOWN_REQUIRED_METADATA_GUIDS.contains(&entry.item_id)` → return `UnknownRequiredMetadata` error
   - Unknown non-required metadata is allowed (ignored per spec)

3. **Error variant**: Added `UnknownRequiredMetadata { guid: String }` to `Error`

4. **Test coverage**: 5 unit tests:
   - Known required metadata passes
   - Unknown required metadata rejected
   - Unknown non-required metadata allowed
   - Mixed metadata entries
   - All 6 known GUIDs pass

### Key Learning

**GUID byte order matters**: When creating test data with GUID bytes, must match the actual `Uuid::from_bytes_le()` byte order in the constants. The test initially failed because the byte arrays didn't match the actual constant values.

### MS-VHDX Spec Reference

Section 2.2: "If IsRequired is set to True and the implementation does not recognize this metadata item, the implementation MUST fail to load the file."


## Parent Sector Size Validation (Task 3)

**Date**: 2026-03-15
**Task**: Add validation for parent/child logical sector size matching in differencing disks

### Implementation Details

1. **Validation location**: `File::open()` method, immediately after parent VHDX is loaded
   - Line ~165-174 in `src/file/file.rs`
   - Validation occurs before parent is wrapped in `Option<Box<File>>`

2. **Validation logic**:
   ```rust
    let parent_sector_size = parent_vhdx.logical_sector_size();
    if parent_sector_size != logical_sector_size {
        return Err(Error::SectorSizeMismatch {
            parent: parent_sector_size,
            child: logical_sector_size,
        });
    }
    ```

3. **Error variant added**: `SectorSizeMismatch { parent: u32, child: u32 }` to `Error`
   - Error message format: `"Parent/child sector size mismatch: parent={parent}, child={child}"`
   - Both values displayed for debugging

4. **Test coverage**: 2 unit tests
   - `test_sector_size_validation_matching`: Creates parent+child with 512-byte sectors → passes
   - `test_sector_size_validation_mismatch`: Tests error type and message format directly

### MS-VHDX Spec Reference

Section 2.6.2.4: "The logical sector size of the parent virtual disk MUST match the logical sector size of the child virtual disk."

### Key Learnings

1. **Builder may enforce matching**: The `Builder` appears to inherit sector sizes from parent when creating differencing disks, making it difficult to test mismatch scenarios end-to-end. The error type was tested directly instead.

2. **Validation placement critical**: Sector size validation must happen AFTER parent is loaded but BEFORE it's stored, so we can access the parent's metadata via `logical_sector_size()` accessor method.

3. **Accommodates both sector sizes**: Implementation supports both 512-byte and 4096-byte logical sector sizes as per MS-VHDX spec (validated in `SectorSize::from_bytes()`).

## Parent DataWriteGuid Validation (Task 4)

**Date**: 2026-03-15
**Task**: Add validation in `File::open()` to verify parent disk's DataWriteGuid matches the expected value stored in the differencing disk's parent locator

### Implementation Details

1. **Error enhancements** (src/error.rs):
   - Enhanced `ParentGuidMismatch` error with `expected` and `found` fields for detailed mismatch information
   - Added `InvalidParentLocator(String)` error variant for parent_linkage2 validation

2. **ParentLocator enhancement** (src/metadata/parent_locator.rs):
   - Added `parent_linkage2()` method to check if parent_linkage2 key exists
   - Per MS-VHDX Section 2.2.4, parent_linkage2 MUST NOT exist

3. **File::open() validation** (src/file/file.rs):
   - Added `parse_guid_string()` helper function using `uuid::Uuid::parse_str()`
   - Added three validation checks (in order):
     a. Check `parent_linkage2` doesn't exist → `InvalidParentLocator` error
     b. Check `parent_linkage` exists → `InvalidParentLocator` error if missing
     c. Compare parent's `data_write_guid` with child's `parent_linkage` → `ParentGuidMismatch` error if different

4. **Validation logic**:
   ```rust
   // After sector size validation...
   
    // 1. Verify parent_linkage2 MUST NOT exist
    if locator.parent_linkage2().is_some() {
        return Err(Error::InvalidParentLocator(...));
    }
    
    // 2. Verify parent_linkage exists and matches parent's DataWriteGuid
    if let Some(expected_guid_str) = locator.parent_linkage() {
        let parent_data_write_guid = parent_vhdx.header.data_write_guid;
        let expected_guid = parse_guid_string(expected_guid_str)?;
        
        if parent_data_write_guid != expected_guid {
            return Err(Error::ParentGuidMismatch {
                expected: expected_guid_str.clone(),
                found: parent_data_write_guid.to_string(),
            });
        }
    } else {
        return Err(Error::InvalidParentLocator(...));
    }
    ```

5. **Test coverage**: 4 unit tests added:
   - `test_parent_guid_mismatch_error_format`: Tests error message format
   - `test_invalid_parent_locator_error_format`: Tests InvalidParentLocator error
   - `test_parse_guid_string_valid`: Tests GUID parsing (with/without braces)
   - `test_parse_guid_string_invalid`: Tests invalid GUID handling

### Key Learnings

1. **GUID format flexibility**: `uuid::Uuid::parse_str()` handles both formats:
   - Standard: `550e8400-e29b-41d4-a716-446655440000`
   - With braces: `{550e8400-e29b-41d4-a716-446655440000}`

2. **Error context is essential**: Including both expected and found values in error messages helps debugging parent disk mismatches

3. **Validation sequence matters**: The validations are performed in order:
   - First check parent_linkage2 (stricter spec violation)
   - Then check parent_linkage exists
   - Finally compare GUIDs

4. **ParentLocator key-value structure**: The parent locator stores parent_linkage as a UTF-16 LE string value associated with the "parent_linkage" key

### MS-VHDX Spec Reference

Section 2.2.4:
- "The Parent DataWriteGuid MUST match the current DataWriteGuid value of the parent virtual disk. If the values do not match, the implementation MUST fail to load the file."
- "parent_linkage2 MUST NOT exist"

### Files Modified

- `src/error.rs`: Enhanced `ParentGuidMismatch`, added `InvalidParentLocator`
- `src/metadata/parent_locator.rs`: Added `parent_linkage2()` method
- `src/file/vhdx_file.rs`: Added validation logic, helper function, and tests

### Verification

- All 67 unit tests pass
- All 6 integration tests pass  
- No new compiler warnings or errors
- LSP diagnostics clean on all modified files



## Task 5: Circular Parent Chain Detection

**Date**: 2026-03-15
**Task**: Implement protection against circular parent chains and excessive chain depth

### Implementation Summary

Successfully implemented security hardening to prevent DoS attacks via malicious VHDX files with circular or excessively deep parent chains.

### Design Decisions

1. **ParentChainState struct**: Tracks chain traversal with:
   - `visited_guids: HashSet<Guid>` - all disk GUIDs encountered
   - `depth: usize` - current depth (root = 0)

2. **MAX_PARENT_CHAIN_DEPTH = 16**: Security limit (MS-VHDX doesn't specify maximum)

3. **Non-recursive state passing**: Since `Self::open()` is called recursively for parents, created `open_internal()` that accepts chain state parameter, while public `open()` creates initial empty state

### Error Variants Added

```rust
#[error("Circular parent chain detected")]
CircularParentChain,

#[error("Parent chain too deep: {depth} (max 16)")]
ParentChainTooDeep { depth: usize },
```

### Validation Flow

```rust
// Before loading parent, check current disk's GUID
let new_chain_state = chain_state.check_and_update(virtual_disk_id)?;
let parent_vhdx = Self::open_internal(parent_full_path, true, &new_chain_state)?;
```

### Test Coverage

6 new unit tests:
- `test_valid_parent_chain_depth_3`: Grandchild→Child→Parent chain passes
- `test_circular_parent_chain`: A→B→C→A cycle detected via ParentChainState
- `test_parent_chain_too_deep`: Chain of 17 rejected at depth 16
- `test_parent_chain_state_new`: Empty state initialization
- `test_circular_parent_chain_error_format`: Error message verification
- `test_parent_chain_too_deep_error_format`: Error message verification

### Files Modified

- `src/error.rs`: Added error variants
- `src/file/file.rs`: Added ParentChainState, MAX_PARENT_CHAIN_DEPTH, open_internal(), modified parent loading

### Verification

All 79 tests pass (73 unit + 6 integration), no compiler warnings.


## Task 6: Block Size Power-of-2 Validation

**Date**: 2026-03-15
**Task**: Fix block size validation to enforce power-of-2 requirement per MS-VHDX Section 2.2.2

### Implementation Summary

Updated block size validation to enforce MS-VHDX Section 2.2.2 requirements:
- Block size MUST be power of 2
- Block size MUST be between 1MB and 256MB inclusive

### Changes Made

1. **src/error.rs**: Added new error variant
   ```rust
   #[error("Invalid block size: {0} (must be power of 2, 1MB-256MB)")]
   InvalidBlockSize(u32),
   ```

2. **src/metadata/file_parameters.rs**: Updated `from_bytes()` validation
   - Replaced 1MB-aligned check with power-of-2 check
   - Uses bit manipulation: `block_size & (block_size - 1) != 0`
   - Enforces range: 1MB to 256MB inclusive

3. **src/file/builder.rs**: Updated `create()` validation
   - Same power-of-2 and range checks
   - Returns `InvalidBlockSize` error instead of `InvalidMetadata`

4. **Unit Tests Added** (4 new tests):
   - `test_valid_block_sizes`: All powers of 2 from 1MB to 256MB
   - `test_invalid_block_size_non_power_of_2`: Rejects 3MB, 5MB, 6MB, etc.
   - `test_invalid_block_size_below_min`: Rejects <1MB
   - `test_invalid_block_size_above_max`: Rejects >256MB

### Key Learning

Power-of-2 check using bitwise AND is more efficient than modulo:
```rust
// Power of 2: only one bit set
if block_size & (block_size - 1) != 0 {
    // Not power of 2
}
```

### MS-VHDX Spec Reference

Section 2.2.2: "BlockSize (4 bytes): The size of each payload block in bytes. The value MUST be at least 1 MB and not greater than 256 MB, and MUST be a power of 2."

Valid values: 1, 2, 4, 8, 16, 32, 64, 128, 256 MB

### Files Modified

- `src/error.rs`
- `src/metadata/file_parameters.rs`
- `src/file/builder.rs`

### Verification

- All 77 unit tests pass
- All 6 integration tests pass
- No compiler warnings

## Task 7: Disk Size Validation (64TB Max, Alignment)

**Date**: 2026-03-15
**Task**: Add disk size bounds and alignment validation per MS-VHDX Section 2.6.2.3

### Implementation Summary

Updated `VirtualDiskSize` validation to enforce MS-VHDX Section 2.6.2.3 requirements:
- VirtualDiskSize MUST be a multiple of LogicalSectorSize
- VirtualDiskSize MUST NOT exceed 64 TB
- VirtualDiskSize MUST be at least one sector (logical sector size)

### Changes Made

1. **src/error.rs**: Added new error variant
   ```rust
   #[error("Invalid disk size: {size} (must be {min}-{max} bytes and sector-aligned)")]
   InvalidDiskSize {
       size: u64,
       min: u64,
       max: u64,
   },
   ```

2. **src/metadata/disk_size.rs**: 
   - Added `MAX_DISK_SIZE` constant: `64 * 1024 * 1024 * 1024 * 1024` (64TB)
   - Added `validate(&self, logical_sector_size: u32) -> Result<()>` method with three checks:
     a. Minimum: `size >= sector_size`
     b. Maximum: `size <= MAX_DISK_SIZE`
     c. Alignment: `size.is_multiple_of(sector_size)`
   - Added 11 comprehensive unit tests

3. **src/metadata/region.rs**: Updated `virtual_disk_size()` to call validation
   - After parsing `VirtualDiskSize`, calls `validate()` with logical sector size
   - Validation happens during metadata region parsing

### Validation Rules

```rust
pub fn validate(&self, logical_sector_size: u32) -> Result<()> {
    let sector_size = logical_sector_size as u64;

    // 1. Must be at least one sector
    if self.size < sector_size { /* error */ }

    // 2. Must not exceed 64TB
    if self.size > MAX_DISK_SIZE { /* error */ }

    // 3. Must be sector-aligned
    if !self.size.is_multiple_of(sector_size) { /* error */ }

    Ok(())
}
```

### Unit Tests (11 tests)

- `test_valid_disk_size_64tb`: 64TB passes
- `test_invalid_disk_size_above_max`: 64TB+1 rejected
- `test_valid_disk_size_minimum_512`: 512 bytes passes with 512-byte sectors
- `test_valid_disk_size_minimum_4096`: 4096 bytes passes with 4096-byte sectors
- `test_invalid_disk_size_below_min`: < sector size rejected
- `test_invalid_disk_size_unaligned_512`: Not 512-aligned rejected
- `test_invalid_disk_size_unaligned_4096`: Not 4096-aligned rejected
- `test_valid_disk_size_aligned`: 1MB aligned passes
- `test_invalid_disk_size_zero_validation`: 0 rejected
- `test_valid_disk_size_1gb`: 1GB passes
- Plus existing `test_virtual_disk_size` and `test_virtual_disk_size_zero_fails`

### Key Learnings

1. **Sector size required for validation**: Disk size validation requires knowledge of logical sector size, which is stored in a separate metadata entry. Validation must happen after both are parsed.

2. **Validation at integration point**: Instead of modifying `from_bytes()` signature, added separate `validate()` method called from `MetadataRegion::virtual_disk_size()` after sector size is available.

3. **Use `is_multiple_of()` for clippy compliance**: Rust 1.94+ prefers `.is_multiple_of()` over `%` operator for readability. Clippy warning: `manual_is_multiple_of`.

4. **64TB constant**: `64 * 1024 * 1024 * 1024 * 1024` = 68,719,476,736,000 bytes = 64 TiB (tebibytes, binary)

### MS-VHDX Spec Reference

Section 2.6.2.3: "VirtualDiskSize (8 bytes): The size of the virtual disk in bytes. This value MUST be a multiple of LogicalSectorSize and MUST NOT exceed 64 TB."

### Files Modified

- `src/error.rs`: Added `InvalidDiskSize` error variant
- `src/metadata/disk_size.rs`: Added `MAX_DISK_SIZE` constant, `validate()` method, 11 unit tests
- `src/metadata/region.rs`: Updated `virtual_disk_size()` to call validation

### Verification

- All 87 unit tests pass
- All 6 integration tests pass
- No compiler warnings (clippy clean on modified code)
- LSP diagnostics clean on all modified files


## Task 8: Path Traversal Protection

**Date**: 2026-03-15
**Task**: Implement security hardening to prevent path traversal attacks in parent locator resolution

### Implementation Summary

Successfully implemented three-layer path traversal protection for parent locator resolution to prevent CVE-worthy security vulnerabilities.

### Changes Made

1. **src/error.rs**: Added new error variant
   ```rust
   #[error("Invalid parent path: {0}")]
   InvalidParentPath(String),
   ```

2. **src/file/vhdx_file.rs**: Added `validate_parent_path()` function with three-layer security:
   - **Layer 1**: Reject absolute paths (`is_absolute()` check)
   - **Layer 2**: Reject paths containing `..` (string scan)
   - **Layer 3**: Canonicalize and verify resolved path is within base directory

3. **src/file/vhdx_file.rs**: Integrated validation into parent loading code
   - Called after extracting parent_path from metadata
   - Returns clear error messages for security violations

### Security Validation Function

```rust
fn validate_parent_path(parent_path: &str, base_dir: &Path) -> Result<PathBuf> {
    // Layer 1: Reject absolute paths
    if Path::new(parent_path).is_absolute() {
        return Err(Error::InvalidParentPath(
            "Absolute paths not allowed".to_string()
        ));
    }
    
    // Layer 2: Check for .. components
    if parent_path.contains("..") {
        return Err(Error::InvalidParentPath(
            "Path traversal not allowed".to_string()
        ));
    }
    
    // Layer 3: Canonicalize and verify within base directory
    let resolved = base_dir.join(parent_path);
    let canonical_base = base_dir.canonicalize()?;
    let canonical_resolved = resolved.canonicalize()?;
    
    if !canonical_resolved.starts_with(&canonical_base) {
        return Err(Error::InvalidParentPath(
            "Path escapes base directory".to_string()
        ));
    }
    
    Ok(canonical_resolved)
}
```

### Attack Vectors Blocked

1. **Classic Path Traversal**: `../../../etc/passwd` → Blocked by `..` check
2. **Absolute Path Attack**: `/etc/passwd` or `C:\Windows\...` → Blocked by `is_absolute()` check
3. **Symlink Escape**: Blocked by canonicalization verification
4. **Normalized Traversal**: Blocked by canonicalization to actual path

### Platform-Specific Considerations

**Windows**:
- Drive letter paths (`C:\`) rejected as absolute
- UNC paths (`\server\share`) also rejected
- Forward slash paths (`/etc/passwd`) NOT absolute on Windows

**Unix/Linux**:
- Root-relative paths (`/etc/passwd`) rejected

### Unit Tests Added (5 tests)

- `test_valid_relative_path`: Legitimate relative paths pass
- `test_path_traversal_rejected`: `../../../etc/passwd` blocked
- `test_absolute_path_rejected`: Platform-aware absolute path rejection
- `test_path_escapes_directory`: `../parent.vhdx` blocked by `..` check
- `test_invalid_parent_path_error_format`: Error message format validation

### Key Learnings

1. **Platform testing critical**: Initial test failed on Windows because `/etc/passwd` isn't considered absolute there. Used `#[cfg(unix)]` and `#[cfg(windows)]` for platform-specific coverage.

2. **Defense in depth**: Three independent checks provide layered security:
   - String-level `..` detection (fast rejection)
   - Path-level absolute detection (OS-aware)
   - Filesystem-level canonicalization (final verification)

3. **Canonicalization requires file existence**: `canonicalize()` only works if the path exists. This is good for security - validates the file actually exists before resolving symlinks.

4. **Clear error messages**: Detailed error messages help users understand why their VHDX file was rejected, aiding in debugging legitimate use cases.

5. **Integration point**: Validation must happen early in parent loading, before any file operations that could be exploited.

### MS-VHDX Spec Reference

Section 2.2.4: Parent Locator structure stores parent path in key-value pairs. The relative_path key is used for locating the parent VHDX file relative to the child file's directory.

### Files Modified

- `src/error.rs`: Added `InvalidParentPath` error variant
- `src/file/file.rs`: Added `validate_parent_path()` function, integrated into parent loading, added 5 unit tests

### Verification

- All 92 tests pass (86 unit + 6 integration)
- `cargo test file` passes (24 file-related tests)
- No compiler warnings
- LSP diagnostics clean
