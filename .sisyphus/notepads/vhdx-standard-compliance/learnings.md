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

3. **Error variant**: Added `UnknownRequiredMetadata { guid: String }` to `VhdxError`

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

1. **Validation location**: `VhdxFile::open()` method, immediately after parent VHDX is loaded
   - Line ~165-174 in `src/file/vhdx_file.rs`
   - Validation occurs before parent is wrapped in `Option<Box<VhdxFile>>`

2. **Validation logic**:
   ```rust
   let parent_sector_size = parent_vhdx.logical_sector_size();
   if parent_sector_size != logical_sector_size {
       return Err(VhdxError::SectorSizeMismatch {
           parent: parent_sector_size,
           child: logical_sector_size,
       });
   }
   ```

3. **Error variant added**: `SectorSizeMismatch { parent: u32, child: u32 }` to `VhdxError`
   - Error message format: `"Parent/child sector size mismatch: parent={parent}, child={child}"`
   - Both values displayed for debugging

4. **Test coverage**: 2 unit tests
   - `test_sector_size_validation_matching`: Creates parent+child with 512-byte sectors → passes
   - `test_sector_size_validation_mismatch`: Tests error type and message format directly

### MS-VHDX Spec Reference

Section 2.6.2.4: "The logical sector size of the parent virtual disk MUST match the logical sector size of the child virtual disk."

### Key Learnings

1. **Builder may enforce matching**: The `VhdxBuilder` appears to inherit sector sizes from parent when creating differencing disks, making it difficult to test mismatch scenarios end-to-end. The error type was tested directly instead.

2. **Validation placement critical**: Sector size validation must happen AFTER parent is loaded but BEFORE it's stored, so we can access the parent's metadata via `logical_sector_size()` accessor method.

3. **Accommodates both sector sizes**: Implementation supports both 512-byte and 4096-byte logical sector sizes as per MS-VHDX spec (validated in `SectorSize::from_bytes()`).

## Parent DataWriteGuid Validation (Task 4)

**Date**: 2026-03-15
**Task**: Add validation in `VhdxFile::open()` to verify parent disk's DataWriteGuid matches the expected value stored in the differencing disk's parent locator

### Implementation Details

1. **Error enhancements** (src/error.rs):
   - Enhanced `ParentGuidMismatch` error with `expected` and `found` fields for detailed mismatch information
   - Added `InvalidParentLocator(String)` error variant for parent_linkage2 validation

2. **ParentLocator enhancement** (src/metadata/parent_locator.rs):
   - Added `parent_linkage2()` method to check if parent_linkage2 key exists
   - Per MS-VHDX Section 2.2.4, parent_linkage2 MUST NOT exist

3. **VhdxFile::open() validation** (src/file/vhdx_file.rs):
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
       return Err(VhdxError::InvalidParentLocator(...));
   }
   
   // 2. Verify parent_linkage exists and matches parent's DataWriteGuid
   if let Some(expected_guid_str) = locator.parent_linkage() {
       let parent_data_write_guid = parent_vhdx.header.data_write_guid;
       let expected_guid = parse_guid_string(expected_guid_str)?;
       
       if parent_data_write_guid != expected_guid {
           return Err(VhdxError::ParentGuidMismatch {
               expected: expected_guid_str.clone(),
               found: parent_data_write_guid.to_string(),
           });
       }
   } else {
       return Err(VhdxError::InvalidParentLocator(...));
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


