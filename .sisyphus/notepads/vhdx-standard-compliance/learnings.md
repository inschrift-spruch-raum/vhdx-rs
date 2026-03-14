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

