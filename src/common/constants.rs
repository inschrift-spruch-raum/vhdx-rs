//! Constants for VHDX format

/// Size constants
pub const KB: u64 = 1024;
pub const MB: u64 = 1024 * KB;
pub const GB: u64 = 1024 * MB;
pub const TB: u64 = 1024 * GB;

/// VHDX file alignment (1 MB)
pub const VHDX_ALIGNMENT: u64 = MB;

/// Header section size (1 MB)
pub const HEADER_SECTION_SIZE: usize = MB as usize;

/// File Type Identifier offset and size
pub const FILE_TYPE_OFFSET: u64 = 0;
pub const FILE_TYPE_SIZE: usize = 64 * 1024; // 64 KB

/// Header 1 offset (64 KB)
pub const HEADER_1_OFFSET: u64 = 64 * KB;

/// Header 2 offset (128 KB)
pub const HEADER_2_OFFSET: u64 = 128 * KB;

/// Header size (4 KB)
pub const HEADER_SIZE: usize = 4 * 1024;

/// Region Table 1 offset (192 KB)
pub const REGION_TABLE_1_OFFSET: u64 = 192 * KB;

/// Region Table 2 offset (256 KB)
pub const REGION_TABLE_2_OFFSET: u64 = 256 * KB;

/// Region Table size (64 KB)
pub const REGION_TABLE_SIZE: usize = 64 * 1024;

/// Metadata Region fixed size (64 KB for table)
pub const METADATA_TABLE_SIZE: usize = 64 * 1024;

/// BAT entry size (8 bytes)
pub const BAT_ENTRY_SIZE: usize = 8;

/// Sector sizes
pub const LOGICAL_SECTOR_SIZE_512: u32 = 512;
pub const LOGICAL_SECTOR_SIZE_4096: u32 = 4096;

/// Default block size (32 MB)
pub const DEFAULT_BLOCK_SIZE: u32 = 32 * 1024 * 1024;

/// Minimum block size (1 MB)
pub const MIN_BLOCK_SIZE: u32 = MB as u32;

/// Maximum block size (256 MB)
pub const MAX_BLOCK_SIZE: u32 = 256 * 1024 * 1024;

/// Chunk ratio calculation constant: 2^23
pub const CHUNK_RATIO_CONSTANT: u64 = 1 << 23;

/// Log entry header size
pub const LOG_ENTRY_HEADER_SIZE: usize = 64;

/// Data sector size (4 KB)
pub const DATA_SECTOR_SIZE: usize = 4 * 1024;

/// Descriptor size (32 bytes)
pub const DESCRIPTOR_SIZE: usize = 32;

/// Signatures
pub const FILE_TYPE_SIGNATURE: &[u8; 8] = b"vhdxfile";
pub const HEADER_SIGNATURE: &[u8; 4] = b"head";
pub const REGION_TABLE_SIGNATURE: &[u8; 4] = b"regi";
pub const METADATA_SIGNATURE: &[u8; 8] = b"metadata";
pub const LOG_ENTRY_SIGNATURE: &[u8; 4] = b"loge";
pub const DATA_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"desc";
pub const ZERO_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"zero";
pub const DATA_SECTOR_SIGNATURE: &[u8; 4] = b"data";

/// Version constants
pub const VHDX_VERSION: u16 = 1;
pub const LOG_VERSION: u16 = 0;

/// Region GUIDs
pub mod region_guids {
    use crate::types::Guid;

    /// BAT Region GUID: 2DC27766-F623-4200-9D64-115E9BFD4A08
    pub const BAT_REGION: Guid = Guid::from_bytes([
        0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A,
        0x08,
    ]);

    /// Metadata Region GUID: 8B7CA206-4790-4B9A-B8FE-575F050F886E
    pub const METADATA_REGION: Guid = Guid::from_bytes([
        0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88,
        0x6E,
    ]);
}

/// Metadata Item GUIDs
pub mod metadata_guids {
    use crate::types::Guid;

    /// File Parameters GUID: CAA16737-FA36-4D43-B3B6-33F0AA44E76B
    pub const FILE_PARAMETERS: Guid = Guid::from_bytes([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7,
        0x6B,
    ]);

    /// Virtual Disk Size GUID: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
    pub const VIRTUAL_DISK_SIZE: Guid = Guid::from_bytes([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4,
        0xB8,
    ]);

    /// Virtual Disk ID GUID: BECA12AB-B2E6-4523-93EF-C309E000C746
    pub const VIRTUAL_DISK_ID: Guid = Guid::from_bytes([
        0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7,
        0x46,
    ]);

    /// Logical Sector Size GUID: 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
    pub const LOGICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB,
        0x5F,
    ]);

    /// Physical Sector Size GUID: CDA348C7-445D-4471-9CC9-E9885251C556
    pub const PHYSICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5,
        0x56,
    ]);

    /// Parent Locator GUID: A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
    pub const PARENT_LOCATOR: Guid = Guid::from_bytes([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB,
        0x0C,
    ]);

    /// VHDX Parent Locator Type GUID: B04AEFB7-D19E-4A81-B789-25B8E9445913
    pub const LOCATOR_TYPE_VHDX: Guid = Guid::from_bytes([
        0xB7, 0xEF, 0x4A, 0xB0, 0x9E, 0xD1, 0x81, 0x4A, 0xB7, 0x89, 0x25, 0xB8, 0xE9, 0x44, 0x59,
        0x13,
    ]);
}

/// Align a value up to the specified alignment
pub fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

/// Align a value down to the specified alignment
pub fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

/// Check if a value is aligned
pub fn is_aligned(value: u64, alignment: u64) -> bool {
    value & (alignment - 1) == 0
}

/// Align to 1 MB boundary
pub fn align_1mb(value: u64) -> u64 {
    align_up(value, MB)
}

/// Align to 4 KB boundary
pub fn align_4kb(value: u64) -> u64 {
    align_up(value, 4 * KB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, MB), 0);
        assert_eq!(align_up(1, MB), MB);
        assert_eq!(align_up(MB, MB), MB);
        assert_eq!(align_up(MB + 1, MB), 2 * MB);
    }

    #[test]
    fn test_align_down() {
        assert_eq!(align_down(0, MB), 0);
        assert_eq!(align_down(1, MB), 0);
        assert_eq!(align_down(MB, MB), MB);
        assert_eq!(align_down(MB + 1, MB), MB);
    }

    #[test]
    fn test_is_aligned() {
        assert!(is_aligned(0, MB));
        assert!(is_aligned(MB, MB));
        assert!(!is_aligned(1, MB));
        assert!(!is_aligned(MB + 1, MB));
    }

    #[test]
    fn test_guid_constants() {
        // Verify that GUIDs are correctly defined
        assert!(!region_guids::BAT_REGION.is_nil());
        assert!(!region_guids::METADATA_REGION.is_nil());
        assert!(!metadata_guids::FILE_PARAMETERS.is_nil());
    }
}
