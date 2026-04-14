#[allow(non_upper_case_globals)]
pub const KiB: u64 = 1024;
#[allow(non_upper_case_globals)]
pub const MiB: u64 = 1024 * KiB;

pub const HEADER_SECTION_SIZE: usize = 1024 * 1024;

pub const FILE_TYPE_SIZE: usize = 64 * 1024;

pub const HEADER_1_OFFSET: usize = 64 * 1024;

pub const HEADER_2_OFFSET: usize = 128 * 1024;

pub const HEADER_SIZE: usize = 4 * 1024;

pub const REGION_TABLE_1_OFFSET: usize = 192 * 1024;

pub const REGION_TABLE_2_OFFSET: usize = 256 * 1024;

pub const REGION_TABLE_SIZE: usize = 64 * 1024;

pub const METADATA_TABLE_SIZE: usize = 64 * 1024;

pub const BAT_ENTRY_SIZE: usize = 8;

pub const LOGICAL_SECTOR_SIZE_512: u32 = 512;

pub const DEFAULT_BLOCK_SIZE: u32 = 32 * 1024 * 1024;

pub const MIN_BLOCK_SIZE: u32 = 1024 * 1024;

pub const MAX_BLOCK_SIZE: u32 = 256 * 1024 * 1024;

pub const CHUNK_RATIO_CONSTANT: u64 = 1 << 23;

pub const LOG_ENTRY_HEADER_SIZE: usize = 64;

pub const DATA_SECTOR_SIZE: usize = 4 * 1024;

pub const DESCRIPTOR_SIZE: usize = 32;

pub const FILE_TYPE_SIGNATURE: &[u8; 8] = b"vhdxfile";
pub const HEADER_SIGNATURE: &[u8; 4] = b"head";
pub const REGION_TABLE_SIGNATURE: &[u8; 4] = b"regi";
pub const METADATA_SIGNATURE: &[u8; 8] = b"metadata";
pub const LOG_ENTRY_SIGNATURE: &[u8; 4] = b"loge";
pub const DATA_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"desc";
pub const ZERO_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"zero";

pub const VHDX_VERSION: u16 = 1;
pub const LOG_VERSION: u16 = 0;

pub mod region_guids {
    use crate::types::Guid;

    pub const BAT_REGION: Guid = Guid::from_bytes([
        0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A,
        0x08,
    ]);

    pub const METADATA_REGION: Guid = Guid::from_bytes([
        0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88,
        0x6E,
    ]);
}

pub mod metadata_guids {
    use crate::types::Guid;

    pub const FILE_PARAMETERS: Guid = Guid::from_bytes([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7,
        0x6B,
    ]);

    pub const VIRTUAL_DISK_SIZE: Guid = Guid::from_bytes([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4,
        0xB8,
    ]);

    pub const VIRTUAL_DISK_ID: Guid = Guid::from_bytes([
        0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7,
        0x46,
    ]);

    pub const LOGICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB,
        0x5F,
    ]);

    pub const PHYSICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5,
        0x56,
    ]);

    pub const PARENT_LOCATOR: Guid = Guid::from_bytes([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB,
        0x0C,
    ]);
}

pub const fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

pub const fn align_1mib(value: u64) -> u64 {
    align_up(value, MiB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, MiB), 0);
        assert_eq!(align_up(1, MiB), MiB);
        assert_eq!(align_up(MiB, MiB), MiB);
        assert_eq!(align_up(MiB + 1, MiB), 2 * MiB);
    }

    #[test]
    fn test_guid_constants() {
        assert!(!region_guids::BAT_REGION.is_nil());
        assert!(!region_guids::METADATA_REGION.is_nil());
        assert!(!metadata_guids::FILE_PARAMETERS.is_nil());
    }
}
