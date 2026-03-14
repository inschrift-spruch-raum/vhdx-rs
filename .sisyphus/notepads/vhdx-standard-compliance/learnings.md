## IsRequired Flag Implementation (Task 1)

**Date**: 2026-03-15
**Task**: Add IsRequired flag parsing to MetadataTableEntry

### Implementation Details

1. **Added field to struct**: `is_required: bool` to MetadataTableEntry in src/metadata/table.rs

2. **Flag parsing**: Extracted bit 2 from flags field using: `let is_required = flags \& 0x4 != 0;`

3. **Reserved bits validation**: Added validation for bits 3-31:
   - Check: `flags \& 0xFFFFFFF8 != 0`
   - Returns: `VhdxError::InvalidMetadata` if non-zero
   - Ensures forward compatibility per MS-VHDX spec

4. **Test coverage**: Added 5 unit tests:
   - test_is_required_true: flags=0x00000004
   - test_is_required_false: flags=0x00000000
   - test_reserved_bits_error: flags=0xFFFFFFF8
   - test_all_flags_combinations: tests all 8 valid combinations (0x0-0x7)
   - test_high_reserved_bits_error: flags=0xFFFF0000

### Pattern Learned

Follow existing flag parsing pattern in codebase:
- Use bitwise AND with mask for each flag bit
- Validate reserved bits immediately after parsing
- Return InvalidMetadata error for reserved bit violations
- Test boundary cases: all flags false, individual flags, all valid flags, reserved bits set

### MS-VHDX Spec Reference

Section 2.2 (Metadata Table Entry):
- Bit 0: IsUser
- Bit 1: IsVirtualDisk
- Bit 2: IsRequired (NEW)
- Bits 3-31: Reserved (must be 0)

IsRequired flag significance: "If this field is set to True and the implementation does not recognize this metadata item, the implementation MUST fail to load the file."

