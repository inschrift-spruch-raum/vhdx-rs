//! VHDX 文件格式常量定义
//!
//! 本模块定义了 VHDX 虚拟硬盘文件格式中使用的所有常量，包括：
//! - 文件大小单位（KiB、MiB）
//! - 文件布局偏移量和区域大小（对应 MS-VHDX §2.1）
//! - 结构签名（对应 MS-VHDX §2.2-§2.6）
//! - 已知区域和元数据项的 GUID（对应 MS-VHDX §2.2.3.2、§2.6.2）
//! - 块大小限制和对齐辅助函数

/// 千字节（1024 字节），VHDX 文件大小的基本单位
#[allow(non_upper_case_globals)]
pub const KiB: u64 = 1024;
/// 兆字节（1024 × 1024 字节），VHDX 1MB 对齐的基本单位（MS-VHDX §2.1）
#[allow(non_upper_case_globals)]
pub const MiB: u64 = 1024 * KiB;

/// 头部区域总大小，固定为 1MB（MS-VHDX §2.2）
pub const HEADER_SECTION_SIZE: usize = 1024 * 1024;

/// 文件类型标识符大小，64KB（MS-VHDX §2.2.1）
pub const FILE_TYPE_SIZE: usize = 64 * 1024;

/// 第一个头部结构的偏移量，64KB（MS-VHDX §2.2）
pub const HEADER_1_OFFSET: usize = 64 * 1024;

/// 第二个头部结构的偏移量（冗余备份），128KB（MS-VHDX §2.2）
pub const HEADER_2_OFFSET: usize = 128 * 1024;

/// 单个头部结构的大小，4KB（MS-VHDX §2.2.2）
pub const HEADER_SIZE: usize = 4 * 1024;

/// 第一个区域表的偏移量，192KB（MS-VHDX §2.2.3）
pub const REGION_TABLE_1_OFFSET: usize = 192 * 1024;

/// 第二个区域表的偏移量（冗余备份），256KB（MS-VHDX §2.2.3）
pub const REGION_TABLE_2_OFFSET: usize = 256 * 1024;

/// 区域表的大小，64KB（MS-VHDX §2.2.3）
pub const REGION_TABLE_SIZE: usize = 64 * 1024;

/// 元数据表的大小，64KB（MS-VHDX §2.6.1）
pub const METADATA_TABLE_SIZE: usize = 64 * 1024;

/// BAT 条目大小，8 字节（64 位）（MS-VHDX §2.5.1）
pub const BAT_ENTRY_SIZE: usize = 8;

/// 512 字节逻辑扇区大小（MS-VHDX §2.6.2.4）
pub const LOGICAL_SECTOR_SIZE_512: u32 = 512;

/// 默认块大小，32MB
pub const DEFAULT_BLOCK_SIZE: u32 = 32 * 1024 * 1024;

/// 最小块大小，1MB（MS-VHDX §2.6.2.1）
pub const MIN_BLOCK_SIZE: u32 = 1024 * 1024;

/// 最大块大小，256MB（MS-VHDX §2.6.2.1）
pub const MAX_BLOCK_SIZE: u32 = 256 * 1024 * 1024;

/// 块比率常量，2^23（MS-VHDX §2.5），用于计算 BAT 中扇区位图的交错间隔
pub const CHUNK_RATIO_CONSTANT: u64 = 1 << 23;

/// 日志条目头部大小，64 字节（MS-VHDX §2.3.1.1）
pub const LOG_ENTRY_HEADER_SIZE: usize = 64;

/// 数据扇区大小，4KB（MS-VHDX §2.3.1.4）
pub const DATA_SECTOR_SIZE: usize = 4 * 1024;

/// 描述符大小，32 字节（MS-VHDX §2.3.1.2/§2.3.1.3）
pub const DESCRIPTOR_SIZE: usize = 32;

/// 文件类型标识符签名 "vhdxfile"（MS-VHDX §2.2.1）
pub const FILE_TYPE_SIGNATURE: &[u8; 8] = b"vhdxfile";
/// 头部结构签名 "head"（MS-VHDX §2.2.2）
pub const HEADER_SIGNATURE: &[u8; 4] = b"head";
/// 区域表签名 "regi"（MS-VHDX §2.2.3）
pub const REGION_TABLE_SIGNATURE: &[u8; 4] = b"regi";
/// 元数据表签名 "metadata"（MS-VHDX §2.6.1.1）
pub const METADATA_SIGNATURE: &[u8; 8] = b"metadata";
/// 日志条目签名 "loge"（MS-VHDX §2.3.1.1）
pub const LOG_ENTRY_SIGNATURE: &[u8; 4] = b"loge";
/// 数据描述符签名 "desc"（MS-VHDX §2.3.1.3）
pub const DATA_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"desc";
/// 零描述符签名 "zero"（MS-VHDX §2.3.1.2）
pub const ZERO_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"zero";

/// VHDX 文件格式版本号，当前为 1（MS-VHDX §2.2.2）
pub const VHDX_VERSION: u16 = 1;
/// 日志版本号，当前为 0（MS-VHDX §2.3.1.1）
pub const LOG_VERSION: u16 = 0;

/// 已知区域 GUID 定义（MS-VHDX §2.2.3.2）
///
/// 每个 GUID 标识一个区域表中的已知区域类型。
pub mod region_guids {
    use crate::types::Guid;

    /// 块分配表（BAT）区域 GUID（MS-VHDX §2.2.3.2）
    pub const BAT_REGION: Guid = Guid::from_bytes([
        0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A,
        0x08,
    ]);

    /// 元数据区域 GUID（MS-VHDX §2.2.3.2）
    pub const METADATA_REGION: Guid = Guid::from_bytes([
        0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88,
        0x6E,
    ]);
}

/// 已知元数据项 GUID 定义（MS-VHDX §2.6.2）
///
/// 每个 GUID 标识一个元数据表中的已知元数据项。
pub mod metadata_guids {
    use crate::types::Guid;

    /// 文件参数元数据项 GUID（MS-VHDX §2.6.2.1）
    pub const FILE_PARAMETERS: Guid = Guid::from_bytes([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7,
        0x6B,
    ]);

    /// 虚拟磁盘大小元数据项 GUID（MS-VHDX §2.6.2.2）
    pub const VIRTUAL_DISK_SIZE: Guid = Guid::from_bytes([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4,
        0xB8,
    ]);

    /// 虚拟磁盘标识符元数据项 GUID（MS-VHDX §2.6.2.3）
    pub const VIRTUAL_DISK_ID: Guid = Guid::from_bytes([
        0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7,
        0x46,
    ]);

    /// 逻辑扇区大小元数据项 GUID（MS-VHDX §2.6.2.4）
    pub const LOGICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB,
        0x5F,
    ]);

    /// 物理扇区大小元数据项 GUID（MS-VHDX §2.6.2.5）
    pub const PHYSICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5,
        0x56,
    ]);

    /// 父磁盘定位器元数据项 GUID（MS-VHDX §2.6.2.6）
    pub const PARENT_LOCATOR: Guid = Guid::from_bytes([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB,
        0x0C,
    ]);
}

/// 将 value 向上对齐到 alignment 的整数倍
///
/// 使用位运算实现高效对齐：`(value + alignment - 1) & !(alignment - 1)`
/// 要求 alignment 必须是 2 的幂次。
#[must_use]
pub const fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

/// 将 value 向上对齐到 1MB 边界
///
/// VHDX 文件格式要求所有区域按 1MB 对齐（MS-VHDX §2.1）。
#[must_use]
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
