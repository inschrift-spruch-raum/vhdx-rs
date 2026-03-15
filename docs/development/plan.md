# VHDX MS-VHDX Standard Compliance Implementation

## TL;DR

> **Goal**: Implement all MS-VHDX v20240423 MUST and SHOULD requirements to achieve full standard compliance
>
> **Critical Corrections from Metis**:
> - Block size: Power of 2 (1-256 MB), NOT just 1MB/2MB
> - Disk size: 64 TB maximum MUST be enforced
> - Parent/child sector size MUST match
>
> **Deliverables**:
> - IsRequired flag parsing and validation
> - Parent DataWriteGuid validation
> - Block size power-of-2 validation
> - Disk size bounds and alignment validation
> - Security hardening (path traversal, circular parent detection)
> - Comprehensive unit tests for all validations
>
> **Estimated Effort**: Medium (6-8 hours across 8 commits)
> **Parallel Execution**: NO - Sequential commits with dependencies
> **Critical Path**: IsRequired → Parent Validation → Block Size → Disk Size → Security

---

## Context

### Original Request
用户要求实现MS-VHDX标准中所有强制要求(MUST)和建议要求(SHOULD)，不包含任何扩展功能。

### Gap Analysis Summary

**CRITICAL MUST (Missing - Blocks Compliance)**:
1. **IsRequired flag handling** - Metadata table entry bit 2 not parsed, no rejection of unknown required metadata
2. **Parent DataWriteGuid validation** - Differencing disks don't validate parent hasn't changed
3. **Block size power-of-2 validation** - Currently allows any 1MB multiple, must be power of 2
4. **Disk size 64TB maximum** - No upper bound check

**SHOULD (Missing)**:
5. **Minimum disk size 3MB** (or LogicalSectorSize if smaller)
6. **VirtualDiskSize sector alignment**
7. **Parent/child sector size match**

**Security Vulnerabilities**:
8. **Path traversal** in parent locator resolution
9. **Circular parent chain** DoS
10. **Integer overflow** in BAT calculations
11. **Memory exhaustion** via malicious BAT

**IMPLEMENTED (Compliant)**:
- Reserved BAT states 4,5 rejection ✓
- Sector size 512/4096 validation ✓
- BAT/Metadata alignment (1MB, over-validates) ✓

### Metis Review Findings

**Critical Corrections**:
- ❌ Block size restricted to 1MB/2MB only
- ✅ Block size must be power of 2: 1, 2, 4, 8, 16, 32, 64, 128, 256 MB
- ❌ Only minimum disk size check
- ✅ Must also enforce 64TB maximum and sector alignment
- ❌ Only DataWriteGuid validation
- ✅ Must also validate parent/child sector size match

**Additional Requirements Identified**:
- Metadata offset >= 64 KB
- Max 1024 IsUser entries
- Reserved bits must be 0
- Chunk ratio calculation validation
- FileWriteGuid update on modification
- Metadata item uniqueness (ItemId + IsUser)

---

## Work Objectives

### Core Objective
Achieve 100% compliance with MS-VHDX v20240423 specification by implementing all missing MUST and SHOULD requirements, plus security hardening.

### Concrete Deliverables
1. `MetadataTableEntry` with `is_required` field and bit 2 parsing
2. Known required metadata whitelist with rejection logic
3. Parent DataWriteGuid comparison validation
4. Parent/child sector size match validation
5. Block size power-of-2 validation (1-256 MB)
6. Disk size bounds validation (min: sector size, max: 64TB)
7. Disk size sector alignment validation
8. Circular parent chain detection (max depth 16)
9. Path traversal protection for parent locator
10. Unit tests for all validations (happy path + failure cases)

### Definition of Done
- [ ] All `cargo test` pass
- [ ] New validations reject non-compliant VHDX files
- [ ] Existing valid VHDX files still open successfully
- [ ] Security tests verify protection against attacks
- [ ] No compiler warnings

### Must Have
- IsRequired flag parsing from bit 2
- Rejection of unknown metadata with IsRequired=true
- Parent DataWriteGuid validation on differencing disk open
- Block size power-of-2 validation
- Disk size 64TB maximum enforcement
- Circular parent detection (max depth 16)

### Must NOT Have (Guardrails)
- Do NOT restrict block size to 1MB/2MB only (breaks spec)
- Do NOT use blacklist for metadata rejection (must be whitelist)
- Do NOT skip parent validation if parent_linkage2 exists
- Do NOT modify file write operations (read-only validation only)
- Do NOT break existing tests without fixing them first

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES - `cargo test` available
- **Automated tests**: Tests-after (implement then add tests)
- **Framework**: Built-in Rust test framework
- **Coverage target**: 100% of new validation paths

### QA Policy
Every task MUST include Agent-Executed QA Scenarios:
- **Unit Tests**: Rust `#[test]` functions with assertions
- **Integration Tests**: Full VHDX file operations
- **Security Tests**: Malicious input handling
- **Regression Tests**: Existing functionality preserved

---

## Execution Strategy

### Sequential Execution (NO Parallelism)

Dependencies require sequential implementation:

```
Phase 1: Foundation (Start Immediately)
├── Task 1: Add IsRequired flag parsing to MetadataTableEntry
│   └── Unblocks: Task 2
├── Task 2: Add known required metadata whitelist
│   └── Unblocks: Task 8
│
Phase 2: Parent Validation (After Phase 1)
├── Task 3: Add parent sector size validation
│   └── Unblocks: Task 4
├── Task 4: Add parent DataWriteGuid validation
│   └── Unblocks: Task 5
├── Task 5: Add circular parent detection
│   └── Unblocks: Task 8
│
Phase 3: Size Validations (After Phase 2)
├── Task 6: Fix block size validation (power of 2)
│   └── Note: Must fix existing tests first
├── Task 7: Add disk size validation (64TB max, alignment)
│   └── Unblocks: Task 8
│
Phase 4: Security & Integration (After Phase 3)
├── Task 8: Add path traversal protection
├── Task 9: Comprehensive unit tests for all validations
└── Task 10: Full regression test suite + final verification
```

### Critical Path
Task 1 → Task 2 → Task 4 → Task 5 → Task 6 → Task 7 → Task 10

### Agent Dispatch Summary

- **T1-T2**: `quick` - Metadata flag parsing (straightforward bit manipulation)
- **T3-T5**: `deep` - Parent validation with chain traversal and security
- **T6-T7**: `quick` - Size validation arithmetic
- **T8**: `deep` - Path security and sanitization
- **T9-T10**: `unspecified-high` - Comprehensive test coverage

---

## TODOs

### Task 1: Add IsRequired Flag Parsing to MetadataTableEntry [x]

**What to do**:
- Add `is_required: bool` field to `MetadataTableEntry` struct in `src/metadata/table.rs`
- Parse bit 2 from flags field: `let is_required = flags & 0x4 != 0;`
- Update `MetadataTableEntry::from_bytes()` to extract bit 2
- Update any Display/Debug implementations
- Add unit test for flag parsing

**Must NOT do**:
- Do NOT skip reserved bit validation (bits 3-31 must be 0)
- Do NOT change existing `is_user` or `is_virtual_disk` parsing

**Recommended Agent Profile**:
- **Category**: `quick`
- **Skills**: None needed (simple bit manipulation)

**Parallelization**:
- **Can Run In Parallel**: NO (foundation task, blocks T2)
- **Blocks**: Task 2
- **Blocked By**: None

**References**:
- **Pattern**: `src/metadata/table.rs:63-86` - Existing MetadataTableEntry parsing
- **Spec**: `misc/MS-VHDX.md` Section 2.2 - Metadata Table Entry
- **API**: `src/metadata/table.rs:MetadataTableEntry` - Struct to modify

**Acceptance Criteria**:
- [ ] `is_required` field added to `MetadataTableEntry`
- [ ] Bit 2 correctly parsed from flags
- [ ] Unit test: `test_is_required_flag_parsed_from_bit_2()` passes
- [ ] Unit test: `test_is_required_false_when_bit_2_clear()` passes
- [ ] Reserved bits 3-31 validated as 0

**QA Scenarios**:

```
Scenario: Parse IsRequired=true (bit 2 set)
  Tool: Bash (cargo test)
  Preconditions: Test file with flags=0x00000004
  Steps:
    1. Run: cargo test test_is_required_flag_parsed_from_bit_2
  Expected Result: Test passes, is_required=true
  Evidence: .sisyphus/evidence/task-1-isrequired-true.txt

Scenario: Parse IsRequired=false (bit 2 clear)
  Tool: Bash (cargo test)
  Preconditions: Test file with flags=0x00000000
  Steps:
    1. Run: cargo test test_is_required_false_when_bit_2_clear
  Expected Result: Test passes, is_required=false
  Evidence: .sisyphus/evidence/task-1-isrequired-false.txt

Scenario: Reject non-zero reserved bits
  Tool: Bash (cargo test)
  Preconditions: Test file with flags=0xFFFFFFF8 (bits 3-31 set)
  Steps:
    1. Run: cargo test test_reject_nonzero_reserved_bits
  Expected Result: Returns InvalidMetadata error
  Evidence: .sisyphus/evidence/task-1-reserved-bits-error.txt
```

**Commit**: YES
- Message: `feat(metadata): add IsRequired flag parsing to MetadataTableEntry`
- Files: `src/metadata/table.rs`, `src/metadata/mod.rs` (if needed)
- Pre-commit: `cargo test metadata::table`

---

### Task 2: Add Known Required Metadata Whitelist [x]

**What to do**:
- Define whitelist of recognized required metadata GUIDs
- Add validation in `MetadataRegion::from_bytes()` to reject unknown required metadata
- Add new error variant `UnknownRequiredMetadata` to `Error`
- Update error messages to include the unknown GUID

**Must NOT do**:
- Do NOT use blacklist approach (must be whitelist)
- Do NOT reject unknown non-required metadata (MAY ignore per spec)

**Known Required Metadata GUIDs**:
- File Parameters: `CAA16737-FA36-4D43-B3B6-33F0AA44E76B`
- Virtual Disk Size: `2FA54224-CD1B-4876-B211-5DBED83BF4B8`
- Virtual Disk ID: `BECA4B1E-C294-4701-8F99-C63D33312C71`
- Logical Sector Size: `8141BF1D-A96F-4709-BA47-F233A8FAAB5F`
- Physical Sector Size: `CDA348C7-889D-4916-90F7-89D5DA63A0C5`
- Parent Locator: `A558951E-B615-4723-A4B7-6A1A4B2B5A6A`

**Recommended Agent Profile**:
- **Category**: `quick`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T1, blocks T8)
- **Blocks**: Task 8
- **Blocked By**: Task 1

**References**:
- **Pattern**: `src/metadata/region.rs` - MetadataRegion parsing
- **Pattern**: `src/error.rs` - Error type definitions
- **Spec**: `misc/MS-VHDX.md` Section 2.2 - "If IsRequired is set... MUST fail"

**Acceptance Criteria**:
- [ ] Whitelist defined with all known required metadata GUIDs
- [ ] Validation rejects unknown required metadata
- [ ] Unknown non-required metadata allowed (ignored)
- [ ] Error includes the unknown GUID
- [ ] Unit test: known required metadata passes
- [ ] Unit test: unknown required metadata rejected
- [ ] Unit test: unknown non-required metadata allowed

**QA Scenarios**:

```
Scenario: Accept known required metadata
  Tool: Bash (cargo test)
  Preconditions: VHDX with FileParameters (known required)
  Steps:
    1. Create VHDX with FileParameters metadata (IsRequired=true)
    2. Open with VhdxFile::open()
  Expected Result: Opens successfully
  Evidence: .sisyphus/evidence/task-2-known-required-ok.txt

Scenario: Reject unknown required metadata
  Tool: Bash (cargo test)
  Preconditions: VHDX with unknown GUID (IsRequired=true)
  Steps:
    1. Create VHDX with fake GUID metadata (IsRequired=true)
    2. Attempt to open
  Expected Result: Returns UnknownRequiredMetadata error
  Evidence: .sisyphus/evidence/task-2-unknown-required-error.txt

Scenario: Allow unknown non-required metadata
  Tool: Bash (cargo test)
  Preconditions: VHDX with unknown GUID (IsRequired=false)
  Steps:
    1. Create VHDX with fake GUID metadata (IsRequired=false)
    2. Open with VhdxFile::open()
  Expected Result: Opens successfully (metadata ignored)
  Evidence: .sisyphus/evidence/task-2-unknown-nonrequired-ok.txt
```

**Commit**: YES
- Message: `feat(metadata): reject unknown required metadata per MS-VHDX spec`
- Files: `src/metadata/region.rs`, `src/error.rs`
- Pre-commit: `cargo test metadata::region`

---

### Task 3: Add Parent Sector Size Validation [x]

**What to do**:
- In `File::open()` when loading differencing disk parent, validate sector sizes match
- Compare `parent.metadata.logical_sector_size()` with `self.metadata.logical_sector_size()`
- Add error variant `SectorSizeMismatch` if not matching

**Must NOT do**:
- Do NOT skip validation if parent is missing (different error)
- Do NOT compare physical sector size (only logical required)

**Recommended Agent Profile**:
- **Category**: `quick`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T2, blocks T4)
- **Blocks**: Task 4
- **Blocked By**: Task 2

**References**:
- **Pattern**: `src/file/vhdx_file.rs:152-174` - Parent loading code
- **Pattern**: `src/metadata/sector_size.rs` - Sector size handling
- **Spec**: `misc/MS-VHDX.md` Section 2.6.2.4 - Parent/child sector size

**Acceptance Criteria**:
- [ ] Sector sizes compared when opening differencing disk
- [ ] Error returned if parent sector size != child sector size
- [ ] Error message includes both values
- [ ] Unit test: matching sector sizes pass
- [ ] Unit test: mismatching sector sizes fail

**QA Scenarios**:

```
Scenario: Matching sector sizes
  Tool: Bash (cargo test)
  Preconditions: Differencing disk with parent (both 512 bytes/sector)
  Steps:
    1. Create parent with 512 byte sectors
    2. Create child differencing disk with 512 byte sectors
    3. Open child
  Expected Result: Opens successfully
  Evidence: .sisyphus/evidence/task-3-sector-match-ok.txt

Scenario: Mismatched sector sizes
  Tool: Bash (cargo test)
  Preconditions: Differencing disk with parent (parent 512, child 4096)
  Steps:
    1. Create parent with 512 byte sectors
    2. Create child with 4096 byte sectors
    3. Attempt to open child
  Expected Result: Returns SectorSizeMismatch error
  Evidence: .sisyphus/evidence/task-3-sector-mismatch-error.txt
```

**Commit**: YES
- Message: `feat(file): validate parent/child sector size match`
- Files: `src/file/vhdx_file.rs`, `src/error.rs`
- Pre-commit: `cargo test file`

---

### Task 4: Add Parent DataWriteGuid Validation [x]

**What to do**:
- In `File::open()` when loading differencing disk parent, validate DataWriteGuid
- Extract `parent_linkage` from parent locator
- Compare with `parent.header.data_write_guid`
- Use existing `ParentGuidMismatch` error (currently unused)

**Must NOT do**:
- Do NOT skip if parent_linkage2 exists (must verify it's absent per spec)
- Do NOT compare FileWriteGuid (only DataWriteGuid matters for parent)

**Recommended Agent Profile**:
- **Category**: `deep`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T3, blocks T5)
- **Blocks**: Task 5
- **Blocked By**: Task 3

**References**:
- **Pattern**: `src/file/vhdx_file.rs:152-174` - Parent loading
- **Pattern**: `src/metadata/parent_locator.rs` - Parent locator parsing
- **Spec**: `misc/MS-VHDX.md` Section 2.2.4 - DataWriteGuid validation
- **Error**: `src/error.rs:56` - Existing ParentGuidMismatch error

**Acceptance Criteria**:
- [ ] DataWriteGuid extracted from parent header
- [ ] Parent linkage extracted from parent locator
- [ ] GUIDs compared and validated
- [ ] Error returned if mismatch
- [ ] Verify parent_linkage2 is absent (MUST NOT exist per spec)
- [ ] Unit test: matching GUIDs pass
- [ ] Unit test: mismatching GUIDs fail
- [ ] Unit test: parent_linkage2 present fails

**QA Scenarios**:

```
Scenario: Matching DataWriteGuid
  Tool: Bash (cargo test)
  Preconditions: Differencing disk with unchanged parent
  Steps:
    1. Create parent disk
    2. Create child pointing to parent
    3. Open child (parent unchanged)
  Expected Result: Opens successfully
  Evidence: .sisyphus/evidence/task-4-guid-match-ok.txt

Scenario: Mismatched DataWriteGuid (parent modified)
  Tool: Bash (cargo test)
  Preconditions: Differencing disk with modified parent
  Steps:
    1. Create parent disk
    2. Create child pointing to parent
    3. Modify parent (changes DataWriteGuid)
    4. Attempt to open child
  Expected Result: Returns ParentGuidMismatch error
  Evidence: .sisyphus/evidence/task-4-guid-mismatch-error.txt

Scenario: parent_linkage2 present
  Tool: Bash (cargo test)
  Preconditions: Differencing disk with parent_linkage2
  Steps:
    1. Create VHDX with parent_linkage2 entry
    2. Attempt to open
  Expected Result: Returns error (parent_linkage2 MUST NOT exist)
  Evidence: .sisyphus/evidence/task-4-linkage2-error.txt
```

**Commit**: YES
- Message: `feat(file): validate parent DataWriteGuid for differencing disks`
- Files: `src/file/vhdx_file.rs`, `src/error.rs` (if modifying)
- Pre-commit: `cargo test file`

---

### Task 5: Add Circular Parent Chain Detection [x]

**What to do**:
- Track parent chain depth during parent loading
- Maintain HashSet of visited disk GUIDs
- Return error if depth exceeds 16 or if cycle detected
- Add error variant `CircularParentChain` or `ParentChainTooDeep`

**Must NOT do**:
- Do NOT use recursion (stack overflow risk)
- Do NOT allow unlimited chain depth

**Recommended Agent Profile**:
- **Category**: `deep`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T4, blocks T8)
- **Blocks**: Task 8
- **Blocked By**: Task 4

**References**:
- **Pattern**: `src/file/vhdx_file.rs:152-174` - Parent loading logic
- **Security**: Circular reference DoS attack vector

**Acceptance Criteria**:
- [ ] Parent chain depth tracked during loading
- [ ] Cycle detection using visited GUIDs
- [ ] Maximum depth of 16 enforced
- [ ] Error returned on cycle detection
- [ ] Error returned on exceeding max depth
- [ ] Unit test: valid chain (depth 3) passes
- [ ] Unit test: circular chain detected
- [ ] Unit test: chain too deep (>16) rejected

**QA Scenarios**:

```
Scenario: Valid parent chain (depth 3)
  Tool: Bash (cargo test)
  Preconditions: Grandchild → Child → Parent
  Steps:
    1. Create parent
    2. Create child pointing to parent
    3. Create grandchild pointing to child
    4. Open grandchild
  Expected Result: Opens successfully
  Evidence: .sisyphus/evidence/task-5-valid-chain-ok.txt

Scenario: Circular parent chain
  Tool: Bash (cargo test)
  Preconditions: A → B → C → A
  Steps:
    1. Create disk A
    2. Create disk B pointing to A
    3. Modify A to point to B (circular)
    4. Attempt to open A
  Expected Result: Returns CircularParentChain error
  Evidence: .sisyphus/evidence/task-5-circular-error.txt

Scenario: Chain too deep (>16)
  Tool: Bash (cargo test)
  Preconditions: Chain of 17 disks
  Steps:
    1. Create chain of 17 disks
    2. Attempt to open deepest
  Expected Result: Returns ParentChainTooDeep error
  Evidence: .sisyphus/evidence/task-5-too-deep-error.txt
```

**Commit**: YES
- Message: `security(file): add circular parent chain detection`
- Files: `src/file/vhdx_file.rs`, `src/error.rs`
- Pre-commit: `cargo test file`

---

### Task 6: Fix Block Size Validation (Power of 2) [x]

**What to do**:
- Update `FileParameters::validate()` to check power of 2
- Valid values: 1, 2, 4, 8, 16, 32, 64, 128, 256 MB
- Check: `size & (size - 1) == 0` (only one bit set)
- Range: 1MB to 256MB inclusive
- Fix any existing tests that use non-power-of-2 sizes

**Must NOT do**:
- Do NOT restrict to 1MB/2MB only (breaks spec compliance)
- Do NOT allow non-power-of-2 values

**Recommended Agent Profile**:
- **Category**: `quick`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T5, blocks T7)
- **Blocks**: Task 7
- **Blocked By**: Task 5

**References**:
- **Pattern**: `src/metadata/file_parameters.rs:38-51` - Current validation
- **Spec**: `misc/MS-VHDX.md` Section 2.2.2 - Block size power of 2

**Acceptance Criteria**:
- [ ] Power of 2 validation added
- [ ] 1MB minimum enforced
- [ ] 256MB maximum enforced
- [ ] All existing tests updated to use valid block sizes
- [ ] Unit test: valid powers of 2 pass (1, 2, 4, 8, 16, 32, 64, 128, 256 MB)
- [ ] Unit test: non-power-of-2 rejected (3, 5, 6, 7 MB, etc.)
- [ ] Unit test: below minimum rejected (<1MB)
- [ ] Unit test: above maximum rejected (>256MB)

**QA Scenarios**:

```
Scenario: Valid block sizes (power of 2)
  Tool: Bash (cargo test)
  Preconditions: Test each valid size
  Steps:
    1. Run: cargo test test_valid_block_sizes
  Expected Result: All powers of 2 from 1MB to 256MB pass
  Evidence: .sisyphus/evidence/task-6-valid-sizes.txt

Scenario: Invalid block size (3MB)
  Tool: Bash (cargo test)
  Preconditions: Block size = 3MB (not power of 2)
  Steps:
    1. Create FileParameters with 3MB block size
    2. Validate
  Expected Result: Returns InvalidBlockSize error
  Evidence: .sisyphus/evidence/task-6-invalid-size-3mb.txt

Scenario: Block size below minimum
  Tool: Bash (cargo test)
  Preconditions: Block size = 512KB (<1MB)
  Steps:
    1. Create FileParameters with 512KB block size
    2. Validate
  Expected Result: Returns InvalidBlockSize error
  Evidence: .sisyphus/evidence/task-6-below-min.txt

Scenario: Block size above maximum
  Tool: Bash (cargo test)
  Preconditions: Block size = 512MB (>256MB)
  Steps:
    1. Create FileParameters with 512MB block size
    2. Validate
  Expected Result: Returns InvalidBlockSize error
  Evidence: .sisyphus/evidence/task-6-above-max.txt
```

**Commit**: YES
- Message: `fix(metadata): enforce block size power of 2 per MS-VHDX spec`
- Files: `src/metadata/file_parameters.rs`, `tests/` (fix existing tests)
- Pre-commit: `cargo test`

---

### Task 7: Add Disk Size Validation (64TB Max, Alignment) [x]

**What to do**:
- Update `VirtualDiskSize::validate()` in `src/metadata/disk_size.rs`
- Minimum: LogicalSectorSize (or 3MB per recommendation)
- Maximum: 64TB (64 * 1024 * 1024 * 1024 * 1024 bytes)
- Alignment: Must be multiple of LogicalSectorSize
- Add error variant `InvalidDiskSize` with size value

**Must NOT do**:
- Do NOT only check minimum (must check max and alignment)
- Do NOT use hardcoded 3MB minimum (use sector size)

**Recommended Agent Profile**:
- **Category**: `quick`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T6, blocks T8)
- **Blocks**: Task 8
- **Blocked By**: Task 6

**References**:
- **Pattern**: `src/metadata/disk_size.rs:33-37` - Current validation
- **Spec**: `misc/MS-VHDX.md` Section 2.6.2.3 - Virtual disk size

**Acceptance Criteria**:
- [ ] 64TB maximum enforced
- [ ] Sector size minimum enforced
- [ ] Sector alignment enforced
- [ ] Unit test: 64TB passes
- [ ] Unit test: 64TB+1 rejected
- [ ] Unit test: sector size minimum passes
- [ ] Unit test: unaligned size rejected
- [ ] Unit test: 0 size rejected

**QA Scenarios**:

```
Scenario: Maximum disk size (64TB)
  Tool: Bash (cargo test)
  Preconditions: VirtualDiskSize = 64TB
  Steps:
    1. Create disk size = 64TB
    2. Validate
  Expected Result: Passes validation
  Evidence: .sisyphus/evidence/task-7-max-size-ok.txt

Scenario: Disk size exceeds maximum
  Tool: Bash (cargo test)
  Preconditions: VirtualDiskSize = 64TB + 1 byte
  Steps:
    1. Create disk size = 64TB + 1
    2. Validate
  Expected Result: Returns InvalidDiskSize error
  Evidence: .sisyphus/evidence/task-7-above-max-error.txt

Scenario: Disk size unaligned
  Tool: Bash (cargo test)
  Preconditions: VirtualDiskSize = 1MB + 100 bytes (not sector aligned)
  Steps:
    1. Create disk size with remainder
    2. Validate
  Expected Result: Returns InvalidDiskSize error
  Evidence: .sisyphus/evidence/task-7-unaligned-error.txt

Scenario: Zero disk size
  Tool: Bash (cargo test)
  Preconditions: VirtualDiskSize = 0
  Steps:
    1. Create disk size = 0
    2. Validate
  Expected Result: Returns InvalidDiskSize error
  Evidence: .sisyphus/evidence/task-7-zero-error.txt
```

**Commit**: YES
- Message: `feat(metadata): add disk size bounds and alignment validation`
- Files: `src/metadata/disk_size.rs`, `src/error.rs`
- Pre-commit: `cargo test metadata::disk_size`

---

### Task 8: Add Path Traversal Protection [x]

**What to do**:
- In parent locator resolution, canonicalize and validate paths
- Reject paths containing `..` or absolute paths
- Ensure resolved path is within allowed base directory
- Add error variant `InvalidParentPath`

**Must NOT do**:
- Do NOT allow relative path escape (`../../../etc/passwd`)
- Do NOT allow absolute paths (`/etc/passwd`, `C:\Windows\...`)

**Recommended Agent Profile**:
- **Category**: `deep`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T2, T5, T7)
- **Blocks**: Task 9, Task 10
- **Blocked By**: Task 2, Task 5, Task 7

**References**:
- **Security**: Path traversal attack vector
- **Pattern**: `src/metadata/parent_locator.rs` - Path resolution

**Acceptance Criteria**:
- [ ] Paths canonicalized before use
- [ ] `..` sequences rejected
- [ ] Absolute paths rejected
- [ ] Path must be within base directory
- [ ] Unit test: valid relative path passes
- [ ] Unit test: path with `..` rejected
- [ ] Unit test: absolute path rejected

**QA Scenarios**:

```
Scenario: Valid relative path
  Tool: Bash (cargo test)
  Preconditions: Parent path = "parent.vhdx"
  Steps:
    1. Create differencing disk with relative parent path
    2. Open in same directory
  Expected Result: Opens successfully
  Evidence: .sisyphus/evidence/task-8-valid-path-ok.txt

Scenario: Path traversal attack
  Tool: Bash (cargo test)
  Preconditions: Parent path = "../../../etc/passwd"
  Steps:
    1. Create differencing disk with malicious parent path
    2. Attempt to open
  Expected Result: Returns InvalidParentPath error
  Evidence: .sisyphus/evidence/task-8-traversal-error.txt

Scenario: Absolute path
  Tool: Bash (cargo test)
  Preconditions: Parent path = "/absolute/path/parent.vhdx"
  Steps:
    1. Create differencing disk with absolute path
    2. Attempt to open
  Expected Result: Returns InvalidParentPath error
  Evidence: .sisyphus/evidence/task-8-absolute-error.txt
```

**Commit**: YES
- Message: `security(file): add path traversal protection for parent locator`
- Files: `src/file/vhdx_file.rs` or `src/metadata/parent_locator.rs`, `src/error.rs`
- Pre-commit: `cargo test file`

---

### Task 9: Comprehensive Unit Tests [x]

**What to do**:
- Write unit tests for ALL new validations
- Cover happy path and failure cases
- Test boundary conditions
- Ensure 100% branch coverage for new code

**Must NOT do**:
- Do NOT skip negative test cases
- Do NOT use placeholder test data (use concrete values)

**Recommended Agent Profile**:
- **Category**: `unspecified-high`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (depends on T8, blocks T10)
- **Blocks**: Task 10
- **Blocked By**: Task 8

**Test Coverage Requirements**:
- [ ] Task 1: IsRequired flag parsing (3 tests)
- [ ] Task 2: Known required metadata whitelist (3 tests)
- [ ] Task 3: Parent sector size validation (2 tests)
- [ ] Task 4: Parent DataWriteGuid validation (3 tests)
- [ ] Task 5: Circular parent chain detection (3 tests)
- [ ] Task 6: Block size power of 2 (4+ tests)
- [ ] Task 7: Disk size bounds (4+ tests)
- [ ] Task 8: Path traversal protection (3 tests)

**Total**: 25+ new unit tests

**QA Scenarios**:

```
Scenario: All unit tests pass
  Tool: Bash (cargo test)
  Preconditions: All new validation code implemented
  Steps:
    1. Run: cargo test
  Expected Result: All tests pass, no failures
  Evidence: .sisyphus/evidence/task-9-all-tests-pass.txt

Scenario: Test coverage report
  Tool: Bash (cargo tarpaulin or similar)
  Preconditions: Tests implemented
  Steps:
    1. Run coverage analysis on new code
  Expected Result: 100% branch coverage for validation code
  Evidence: .sisyphus/evidence/task-9-coverage-report.txt
```

**Commit**: YES (can be multiple commits, one per task's tests)
- Message: `test: add comprehensive unit tests for MS-VHDX compliance validations`
- Files: `src/*/tests.rs` or `tests/*.rs`
- Pre-commit: `cargo test`

---

### Task 10: Full Regression Test Suite [x]

**What to do**:
- Run complete test suite: `cargo test`
- Run integration tests: `cargo test --test integration`
- Verify no existing functionality broken
- Create test VHDX files and verify they still work
- Document any breaking changes

**Must NOT do**:
- Do NOT skip integration tests
- Do NOT ignore test failures

**Recommended Agent Profile**:
- **Category**: `unspecified-high`
- **Skills**: None needed

**Parallelization**:
- **Can Run In Parallel**: NO (final verification)
- **Blocks**: None
- **Blocked By**: Task 9

**Acceptance Criteria**:
- [ ] `cargo test` passes (0 failures)
- [ ] `cargo clippy` shows no warnings
- [ ] `cargo fmt` check passes
- [ ] Integration tests pass
- [ ] Manual test: Create and open VHDX files
- [ ] Manual test: Read/write operations work
- [ ] Manual test: Differencing disk operations work

**QA Scenarios**:

```
Scenario: Full test suite passes
  Tool: Bash (cargo test)
  Preconditions: All changes committed
  Steps:
    1. Run: cargo test --all
  Expected Result: All tests pass
  Evidence: .sisyphus/evidence/task-10-full-test-suite.txt

Scenario: No compiler warnings
  Tool: Bash (cargo clippy)
  Preconditions: All changes committed
  Steps:
    1. Run: cargo clippy -- -D warnings
  Expected Result: No warnings
  Evidence: .sisyphus/evidence/task-10-clippy-clean.txt

Scenario: Code formatting check
  Tool: Bash (cargo fmt)
  Preconditions: All changes committed
  Steps:
    1. Run: cargo fmt -- --check
  Expected Result: No formatting issues
  Evidence: .sisyphus/evidence/task-10-fmt-clean.txt

Scenario: Integration test
  Tool: Bash (cargo test --test integration)
  Preconditions: Integration tests exist
  Steps:
    1. Run integration tests
  Expected Result: All integration tests pass
  Evidence: .sisyphus/evidence/task-10-integration-tests.txt

Scenario: Manual end-to-end test
  Tool: Bash (cargo run)
  Preconditions: CLI built
  Steps:
    1. Create new VHDX: cargo run -- create test.vhdx --size 100MB
    2. Get info: cargo run -- info test.vhdx
    3. Write data: cargo run -- write test.vhdx --offset 0 --input data.bin
    4. Read data: cargo run -- read test.vhdx --offset 0 --length 1024
    5. Check: cargo run -- check test.vhdx
  Expected Result: All operations succeed
  Evidence: .sisyphus/evidence/task-10-manual-e2e.txt
```

**Commit**: NO (verification only)

---

## Final Verification Wave

### F1. Plan Compliance Audit (oracle)

Read the completed implementation and verify:
- [ ] All MUST requirements from MS-VHDX spec are implemented
- [ ] All SHOULD requirements are implemented
- [ ] No spec violations (e.g., block size not restricted to 1MB/2MB)
- [ ] Security vulnerabilities addressed
- [ ] All tests pass

**Command**: Manual review comparing against `misc/MS-VHDX.md`

### F2. Code Quality Review (unspecified-high)

- [ ] `cargo clippy` clean
- [ ] `cargo fmt` clean
- [ ] No `unsafe` code added
- [ ] Error handling follows Rust best practices
- [ ] Documentation comments added for public APIs

### F3. Security Review (unspecified-high)

- [ ] Path traversal protection verified
- [ ] Circular parent chain protection verified
- [ ] Integer overflow protection verified (checked arithmetic)
- [ ] No unwrap() or expect() in new code
- [ ] All error cases handled explicitly

### F4. Test Coverage Review (unspecified-high)

- [ ] All new code has unit tests
- [ ] Boundary conditions tested
- [ ] Error paths tested
- [ ] 100% branch coverage for validation logic
- [ ] Integration tests verify end-to-end scenarios

---

## Commit Strategy

### Commit Sequence (8 commits)

```
Commit 1: feat(metadata): add IsRequired flag parsing to MetadataTableEntry
Commit 2: feat(metadata): reject unknown required metadata per MS-VHDX spec
Commit 3: feat(file): validate parent/child sector size match
Commit 4: feat(file): validate parent DataWriteGuid for differencing disks
Commit 5: security(file): add circular parent chain detection
Commit 6: fix(metadata): enforce block size power of 2 per MS-VHDX spec
Commit 7: feat(metadata): add disk size bounds and alignment validation
Commit 8: security(file): add path traversal protection for parent locator
Commit 9: test: add comprehensive unit tests for MS-VHDX compliance validations
```

### Pre-commit Checks (Each Commit)

```bash
cargo test        # All tests pass
cargo clippy      # No warnings
cargo fmt --check # No formatting issues
```

---

## Success Criteria

### Verification Commands

```bash
# All tests pass
cargo test --all

# No compiler warnings
cargo clippy -- -D warnings

# Code formatting clean
cargo fmt -- --check

# Integration tests
cargo test --test integration

# Manual verification
cargo run -- create test.vhdx --size 100MB --type dynamic
cargo run -- info test.vhdx
cargo run -- check test.vhdx
```

### Final Checklist

- [ ] **MUST Have** (All implemented):
  - [ ] IsRequired flag parsing from bit 2
  - [ ] Rejection of unknown metadata with IsRequired=true
  - [ ] Parent DataWriteGuid validation
  - [ ] Block size power-of-2 validation (1-256 MB)
  - [ ] Disk size 64TB maximum enforcement
  - [ ] Circular parent detection (max depth 16)
  - [ ] Path traversal protection

- [ ] **SHOULD Have** (All implemented):
  - [ ] Minimum disk size validation
  - [ ] Disk size sector alignment
  - [ ] Parent/child sector size match

- [ ] **Security** (All addressed):
  - [ ] Path traversal
  - [ ] Circular parent chains
  - [ ] Integer overflow (checked arithmetic)

- [ ] **Tests** (100% coverage):
  - [ ] 25+ new unit tests
  - [ ] All existing tests still pass
  - [ ] Integration tests pass

- [ ] **Quality**:
  - [ ] No compiler warnings
  - [ ] Code formatted
  - [ ] Clippy clean
  - [ ] Documentation complete

---

## Notes

### Critical Corrections from Metis

1. **Block Size**: Power of 2 (1-256 MB), NOT just 1MB/2MB
   - Windows creates VHDX files with 4MB, 8MB, etc. block sizes
   - Restricting to 1MB/2MB would reject valid Windows-created files

2. **Disk Size**: 64TB maximum is MUST requirement
   - Currently no upper bound check
   - Must also enforce sector alignment

3. **Parent Validation**: Sector size match is MUST
   - Parent and child must have same logical sector size
   - Currently not validated

### Security Considerations

- **Path Traversal**: Malicious parent path like `../../../etc/passwd` could escape directory
- **Circular Chains**: A→B→C→A causes infinite recursion/stack overflow
- **Integer Overflow**: VirtualDiskSize × BlockSize can overflow u64

### Backward Compatibility

- Task 6 (block size validation) may break existing tests
- Must fix tests to use valid power-of-2 block sizes before committing
- No breaking changes to public API

---

*Plan generated by Prometheus with Metis gap analysis*
*Date: 2026-03-14*
*Target: MS-VHDX v20240423 full compliance*
