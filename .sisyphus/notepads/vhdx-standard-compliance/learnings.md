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

### Files Modified

- `src/error.rs`: Added `SectorSizeMismatch` error variant
- `src/file/vhdx_file.rs`: Added validation in `open()` + unit tests
