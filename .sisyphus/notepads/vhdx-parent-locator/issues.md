# Issues and Challenges

## Key Finding: No Existing In-Place Writeback
No open-source project implements true in-place parent locator writeback.
This means the Rust implementation would be first-of-its-kind for VHDX.
All existing code either creates locators at init time or only reads them.

## Technical Challenges

### 1. Metadata Region Mutability
The metadata region in Sections is lazily loaded and cached. Mutation requires:
- Exclusive access to the Sections container
- Modifying raw bytes in the cached Metadata
- Writing back to disk
- Updating metadata table CRC-32C

### 2. Variable-Length String Replacement
Parent paths have variable length (UTF-16 LE). If new path is:
- **Same or shorter**: can overwrite in-place (with padding)
- **Longer**: requires shifting all subsequent data, potentially growing metadata region

### 3. Log-Based Crash Consistency
Per MS-VHDX spec (§2.4), metadata updates should go through the log for crash consistency.
Simple direct writes risk corruption on power failure.

### 4. CRC-32C Recalculation Required
After any metadata modification, the metadata table header's CRC-32C checksum
(at offset 4 in the 64KB metadata table) must be recalculated.

### 5. Windows 10 parent_linkage2
Windows 10 may add optional "parent_linkage2" key. Need to preserve unknown
keys in the locator if performing in-place updates.

### 6. VHD vs VHDX Discrepancy
DiscUtils VHD implementation has different parent locator format (8 entries
with platform codes). VHDX uses key-value dictionary format. This is a
completely different mechanism and should not be confused.
