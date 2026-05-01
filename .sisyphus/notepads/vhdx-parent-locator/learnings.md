# VHDX Parent Locator Implementation Research

## Date: 2026-05-01

## Overview
Researched open-source implementations of VHDX parent locator stale path update/writeback.
No existing library implements in-place parent locator writeback. All implementations either:
- Create parent locator metadata at initial creation time only
- Provide read-only parent path resolution

## Implementations Found

### 1. QEMU (C) - block/vhdx.c
- **Status**: Read-only parent locator detection
- **GUID definition**: parent_locator_guid = {0xa8d35f2d, 0xb30b, 0x454d, ...}
- **Parent type GUID**: {0xb04aefb7, 0xd19e, 0x4a81, ...}
- **Has structs**: VHDXParentLocatorHeader (20 bytes), VHDXParentLocatorEntry (12 bytes)
- **Limitation**: VHDX_TYPE_DIFFERENCING marked as "Currently unsupported", parent locator parsing exists but returns -ENOTSUP when differencing disk detected
- **Reference**: https://github.com/qemu/qemu/blob/master/block/vhdx.c and block/vhdx.h

### 2. DiscUtils (C#) - DiscUtils.Vhd
- **Status**: INITIAL-CREATION-ONLY parent locator writeback for VHD (not VHDX)
- **Key code** (DiskImageFile.cs:InitializeDifferencingInternal):
  - Sets parent locator at creation: ParentLocators[7] = absolute win32 path, ParentLocators[6] = relative path
  - Platform codes: PlatformCodeWindowsAbsoluteUnicode, PlatformCodeWindowsRelativeUnicode
  - Data space: 512 bytes per locator entry
  - Writes platform locator data as UTF-16 LE
  - Reference: https://github.com/DiscUtils/DiscUtils/blob/develop/Library/DiscUtils.Vhd/DiskImageFile.cs (lines 545-582)

### 3. DiscUtils.VHDX (C#) - ParentLocator.cs
- **Status**: Read-only + INITIAL-ONLY. WriteTo() throws NotImplementedException
- Has complete ParentLocator parsing with key-value dictionary:
  - Reads entries as UTF-16 LE strings
  - Dictionary<string, string> Entries
  - Reference: https://github.com/DiscUtils/DiscUtils/blob/develop/Library/DiscUtils.Vhdx/ParentLocator.cs

### 4. uroni/urbackup_backend (C++) - vhdxfile.cpp
- **Status**: INITIAL-CREATION-ONLY parent locator construction
- **Key code**: getMetaRegion() function constructs parent locator with three entries:
  - "parent_linkage" -> parent_data_uuid (GUID string)
  - "relative_path" -> parent_rel_loc
  - "absolute_win32_path" -> parent_abs_loc
- Properly serializes VhdxParentLocatorHeader, VhdxParentLocatorEntry entries, and UTF-16 LE strings
- No in-place update/writeback path exists
- Reference: https://github.com/uroni/urbackup_backend/blob/dev/fsimageplugin/vhdxfile.cpp (lines 460-540)

### 5. FATtools/vhdxutils.py (Python) - ParentLocator
- **Status**: FULL SERIALIZATION AVAILABLE (parse + pack)
- The only implementation with both parse() and pack() methods for ParentLocator
- pack() serializes entries dict back to binary format:
  - Entries are UTF-16 LE encoded
  - Key/Value offsets are relative to locator start
  - Reference: https://github.com/maxpat78/FATtools/blob/master/FATtools/vhdxutils.py (line ~1180-1260)
- Also handles "parent_linkage2" key (Windows 10 adds this)

### 6. 7-Zip (C++) - VhdxHandler.cpp
- **Status**: Read-only parent path resolution
- Defines g_ParentKeys[] = {"relative_path", "volume_path", "absolute_win32_path"}
- Resolution order per spec: relative_path -> volume_path -> absolute_win32_path
- Multiple forks with same code pattern
- Reference: https://github.com/mcmilk/7-Zip-zstd/blob/master/CPP/7zip/Archive/VhdxHandler.cpp

## MS-VHDX Parent Locator Binary Format

Parent Locator metadata (GUID: {a8d35f2d-b30b-454d-abf7-d3d84834ab0c}):

**Header (20 bytes)**:
- LocatorType (16 bytes): GUID {b04aefb7-d19e-4a81-b789-25b8e9445913}
- Reserved (2 bytes): 0
- KeyValueCount (2 bytes): number of key-value entries

**Entry array (12 bytes each, KeyValueCount entries)**:
- KeyOffset (4 bytes LE): offset from locator start to key string in data area
- ValueOffset (4 bytes LE): offset from locator start to value string
- KeyLength (2 bytes): byte length of key data (UTF-16 LE)
- ValueLength (2 bytes): byte length of value data (UTF-16 LE)

**String data area**:
- Keys and values stored as UTF-16 LE, no NUL terminators
- Key/value pairs placed sequentially after the entry array

### Standard Keys:
- "relative_path" - relative path from child to parent
- "volume_path" - volume-qualified path
- "absolute_win32_path" - absolute Windows path
- "parent_linkage" - GUID of parent's DataWriteGuid (for verification)
- "parent_linkage2" - alternative linkage GUID (Windows 10+)

## Current vhdx-rs Implementation State

- ParentLocator struct in src/sections/metadata.rs: READ-ONLY
  - Can parse existing locator
  - resolve_parent_path() resolves parent with priority: relative_path -> volume_path -> absolute_win32_path
  - No writeback/mutation capability
- File in src/file.rs:
  - open_parent_for_read() uses parent locator to open parent
  - read_from_parent_chain_cached() supports chain traversal
  - has_parent/is_fixed tracked
  - No parent locator update/writeback

## Portability to Rust

### Key Observations:
1. **No open-source project does in-place parent locator writeback** - this is novel
2. The binary format is straightforward to serialize:
   - Fixed 20-byte header
   - Fixed 12-byte entries
   - Variable-length UTF-16 LE string data area
3. All offsets within the parent locator are RELATIVE to the locator data start
4. CRC-32C must be recalculated for the metadata table after modification

### Recommended Approach:
1. Add mutation methods to ParentLocator (or create a builder)
2. Add a pack() method that serializes entries back to raw bytes
3. Since metadata is lazily loaded via Sections, the update path needs to:
   - Take the Sections container in mutable form
   - Modify the metadata raw data in-place
   - Recalculate metadata table CRC-32C
   - Write the modified metadata region back to disk
   - Update the BAT and headers (via log for crash consistency)
4. The size constraint: if new parent path data is LONGER than existing, the metadata table's offset/length needs updating too
5. For a simple implementation, the minimum viable approach is to:
   - Replace the UTF-16 LE strings in the existing data area (if same length or shorter)
   - Or rebuild the entire metadata region (if data grows)
