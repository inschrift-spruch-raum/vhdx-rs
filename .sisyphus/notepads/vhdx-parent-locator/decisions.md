# Architectural Decisions for Parent Locator Writeback

## Approach: In-Place Serialization vs Full Rebuild

### Option A: In-Place Update (Recommended for Initial Implementation)
- Replace UTF-16 LE strings directly in the existing metadata data area
- Pros: Minimal disk write, no metadata table rebuild needed
- Cons: Requires new path <= old path length (or padding)

### Option B: Full Metadata Rebuild
- Reconstruct entire metadata region from scratch
- Pros: Always works regardless of path length
- Cons: Complex, requires writing 1MB metadata region

### Option C: Append-and-Relocate
- Append new string data at end of existing data area
- Update entries to point to new offsets
- Pros: Handles growth
- Cons: More complex offset management

## Recommended Entry Point: File::update_parent_path()

Suggested API:
`ust
impl File {
    pub fn update_parent_path(&mut self, new_relative: &str, new_absolute: &str) -> Result<()>;
}
`

This would:
1. Take &mut self for exclusive access
2. Modify the cached Sections metadata
3. Write modified metadata region back to disk
4. Update metadata CRC-32C
5. Update headers (session init) for consistency

## Safety Considerations
- Validate new path exists before writing
- Keep a backup of old entries for rollback
- Use sync_all() after writes
- Consider log-based update for crash safety
