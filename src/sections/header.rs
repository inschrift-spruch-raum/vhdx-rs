//! VHDX 头部区域解析模块
//!
//! 本模块实现了 VHDX 文件头部区域（Header Section）的解析，对应 MS-VHDX §2.2。
//!
//! 头部区域总大小为 1MB，包含以下结构：
//! - **文件类型标识符**（[`FileTypeIdentifier`]）— 固定 64KB，包含签名和创建者信息（§2.2.1）
//! - **头部结构**（[`HeaderStructure`]）— 两个冗余副本，各 4KB，包含序列号、GUID 和日志位置（§2.2.2）
//! - **区域表**（[`RegionTable`]）— 两个冗余副本，各 64KB，描述 BAT 和元数据区域的位置（§2.2.3）

use crate::common::constants::{
    FILE_TYPE_SIGNATURE, FILE_TYPE_SIZE, HEADER_1_OFFSET, HEADER_2_OFFSET, HEADER_SECTION_SIZE,
    HEADER_SIGNATURE, HEADER_SIZE, LOG_VERSION, REGION_TABLE_1_OFFSET, REGION_TABLE_2_OFFSET,
    REGION_TABLE_SIZE, VHDX_VERSION,
};
use crate::error::{Error, Result};
use crate::sections::crc32c_with_zero_field;
use crate::types::Guid;
use std::marker::PhantomData;

/// 从切片安全读取固定长度数组；长度不足时以 0 填充。
fn read_array<const N: usize>(data: &[u8], start: usize) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(slice) = data.get(start..start + N) {
        out.copy_from_slice(slice);
    }
    out
}

/// 从切片安全读取 `u16`（LE）；长度不足返回 0。
fn read_u16(data: &[u8], start: usize) -> u16 {
    u16::from_le_bytes(read_array::<2>(data, start))
}

/// 从切片安全读取 `u32`（LE）；长度不足返回 0。
fn read_u32(data: &[u8], start: usize) -> u32 {
    u32::from_le_bytes(read_array::<4>(data, start))
}

/// 从切片安全读取 `u64`（LE）；长度不足返回 0。
fn read_u64(data: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(read_array::<8>(data, start))
}

/// 从切片安全读取 GUID；长度不足的字节按 0 填充。
fn read_guid(data: &[u8], start: usize) -> Guid {
    Guid::from_bytes(read_array::<16>(data, start))
}

/// VHDX 头部区域的统一访问接口
///
/// 包装 1MB 头部区域的原始数据，提供对文件类型标识符、
/// 头部结构和区域表的类型化访问。
///
/// 头部区域布局（MS-VHDX §2.2）：
/// - `[0, 64KB)` — 文件类型标识符
/// - `[64KB, 68KB)` — 头部结构 1
/// - `[128KB, 132KB)` — 头部结构 2
/// - `[192KB, 256KB)` — 区域表 1
/// - `[256KB, 320KB)` — 区域表 2
pub struct Header<'a> {
    raw_data: Vec<u8>,
    marker: PhantomData<&'a [u8]>,
}

impl Header<'_> {
    /// 从原始数据创建头部区域实例，验证数据长度必须为 1MB
    pub fn new(data: Vec<u8>) -> Result<Self> {
        if data.len() != HEADER_SECTION_SIZE {
            return Err(Error::InvalidFile(format!(
                "Header section must be {} bytes, got {}",
                HEADER_SECTION_SIZE,
                data.len()
            )));
        }
        Ok(Self {
            raw_data: data,
            marker: PhantomData,
        })
    }

    /// 返回头部区域的原始字节数据
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// 获取文件类型标识符（MS-VHDX §2.2.1）
    #[must_use]
    pub fn file_type(&self) -> FileTypeIdentifier<'_> {
        FileTypeIdentifier::new(&self.raw_data[0..FILE_TYPE_SIZE])
    }

    /// 获取指定索引的头部结构（MS-VHDX §2.2.2）
    ///
    /// - `index = 0`：返回两个头部中序列号较大的一个（活动头部），
    ///   根据 MS-VHDX §2.2.2.1，头部更新时会交替写入两个副本，
    ///   序列号较大的代表最新的有效头部
    /// - `index = 1`：强制返回头部 1（偏移 64KB）
    /// - `index = 2`：强制返回头部 2（偏移 128KB）
    #[must_use]
    pub fn header(&self, index: usize) -> Option<HeaderStructure<'_>> {
        match index {
            0 => {
                let h1 = HeaderStructure::new(
                    &self.raw_data[HEADER_1_OFFSET..HEADER_1_OFFSET + HEADER_SIZE],
                )
                .ok()?;
                let h2 = HeaderStructure::new(
                    &self.raw_data[HEADER_2_OFFSET..HEADER_2_OFFSET + HEADER_SIZE],
                )
                .ok()?;

                if h1.sequence_number() > h2.sequence_number() {
                    Some(h1)
                } else {
                    Some(h2)
                }
            }
            1 => {
                HeaderStructure::new(&self.raw_data[HEADER_1_OFFSET..HEADER_1_OFFSET + HEADER_SIZE])
                    .ok()
            }
            2 => {
                HeaderStructure::new(&self.raw_data[HEADER_2_OFFSET..HEADER_2_OFFSET + HEADER_SIZE])
                    .ok()
            }
            _ => None,
        }
    }

    /// 获取指定索引的区域表（MS-VHDX §2.2.3），index=0 或 1 返回表 1，index=2 返回表 2
    #[must_use]
    pub fn region_table(&self, index: usize) -> Option<RegionTable<'_>> {
        let offset = match index {
            0 => {
                let h1 = HeaderStructure::new(
                    &self.raw_data[HEADER_1_OFFSET..HEADER_1_OFFSET + HEADER_SIZE],
                )
                .ok()?;
                let h2 = HeaderStructure::new(
                    &self.raw_data[HEADER_2_OFFSET..HEADER_2_OFFSET + HEADER_SIZE],
                )
                .ok()?;
                if h1.sequence_number() > h2.sequence_number() {
                    REGION_TABLE_1_OFFSET
                } else {
                    REGION_TABLE_2_OFFSET
                }
            }
            1 => REGION_TABLE_1_OFFSET,
            2 => REGION_TABLE_2_OFFSET,
            _ => return None,
        };
        RegionTable::new(&self.raw_data[offset..offset + REGION_TABLE_SIZE]).ok()
    }
}

/// 文件类型标识符（MS-VHDX §2.2.1）
///
/// 固定大小 64KB，位于文件开头。
/// 前 8 字节为签名 "vhdxfile"，随后 512 字节为 UTF-16 LE 编码的创建者字符串。
pub struct FileTypeIdentifier<'a> {
    /// 文件类型签名（应为 "vhdxfile"）
    pub signature: [u8; 8],
    /// 创建者原始字节（UTF-16 LE）
    pub creator: &'a [u8],
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> FileTypeIdentifier<'a> {
    /// 从原始字节数据创建文件类型标识符实例
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        let signature = read_array::<8>(data, 0);
        let creator_end = 8 + 512.min(data.len().saturating_sub(8));
        let creator = &data[8..creator_end];
        Self {
            signature,
            creator,
            raw: data,
        }
    }

    /// 返回文件类型标识符的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 返回 8 字节签名（MS-VHDX §2.2.1），应为 "vhdxfile"
    #[must_use]
    pub const fn signature(&self) -> &[u8; 8] {
        &self.signature
    }

    /// 返回 UTF-16 LE 编码的创建者字符串（MS-VHDX §2.2.1）
    ///
    /// 从偏移 8 开始读取最多 512 字节，以空字符结尾。
    #[must_use]
    pub fn creator(&self) -> String {
        let creator_bytes = self.creator;
        let utf16: Vec<u16> = creator_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&utf16)
    }

    /// 构造新的文件类型标识符字节数据（MS-VHDX §2.2.1）
    ///
    /// 写入 "vhdxfile" 签名和可选的创建者字符串（UTF-16 LE 编码），
    /// 返回固定 64KB 大小的数据。
    #[must_use]
    pub fn create(creator: Option<&str>) -> Vec<u8> {
        let mut data = vec![0u8; FILE_TYPE_SIZE];
        data[0..8].copy_from_slice(FILE_TYPE_SIGNATURE);

        if let Some(creator) = creator {
            let utf16: Vec<u16> = creator.encode_utf16().collect();
            for (i, &c) in utf16.iter().enumerate() {
                if 8 + i * 2 + 2 > data.len() {
                    break;
                }
                data[8 + i * 2..8 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
            }
        }

        data
    }
}

/// VHDX 头部结构（MS-VHDX §2.2.2）
///
/// 每个头部结构固定 4KB，VHDX 文件包含两个冗余副本。
/// 包含序列号、多个 GUID、日志版本和位置等信息。
///
/// 字段布局（偏移量相对于结构起始）：
/// | 偏移 | 大小 | 字段 |
/// |------|------|------|
/// | 0 | 4 | 签名 "head" |
/// | 4 | 4 | 校验和（CRC32C） |
/// | 8 | 8 | 序列号 |
/// | 16 | 16 | 文件写入 GUID |
/// | 32 | 16 | 数据写入 GUID |
/// | 48 | 16 | 日志 GUID |
/// | 64 | 2 | 日志版本 |
/// | 66 | 2 | VHDX 版本 |
/// | 68 | 4 | 日志长度 |
/// | 72 | 8 | 日志偏移量 |
pub struct HeaderStructure<'a> {
    /// 头部签名（应为 "head"）
    pub signature: [u8; 4],
    /// CRC32C 校验和
    pub checksum: u32,
    /// 序列号
    pub sequence_number: u64,
    /// 文件写入 GUID
    pub file_write_guid: Guid,
    /// 数据写入 GUID
    pub data_write_guid: Guid,
    /// 日志 GUID
    pub log_guid: Guid,
    /// 日志版本
    pub log_version: u16,
    /// VHDX 版本
    pub version: u16,
    /// 日志长度
    pub log_length: u32,
    /// 日志偏移
    pub log_offset: u64,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> HeaderStructure<'a> {
    /// 从原始字节数据创建头部结构实例，验证数据长度必须为 4KB
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != HEADER_SIZE {
            return Err(Error::CorruptedHeader(format!(
                "Header must be {} bytes, got {}",
                HEADER_SIZE,
                data.len()
            )));
        }
        Ok(Self {
            signature: read_array::<4>(data, 0),
            checksum: read_u32(data, 4),
            sequence_number: read_u64(data, 8),
            file_write_guid: read_guid(data, 16),
            data_write_guid: read_guid(data, 32),
            log_guid: read_guid(data, 48),
            log_version: read_u16(data, 64),
            version: read_u16(data, 66),
            log_length: read_u32(data, 68),
            log_offset: read_u64(data, 72),
            raw: data,
        })
    }

    /// 返回头部结构的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 头部签名 "head"（MS-VHDX §2.2.2）
    #[must_use]
    pub const fn signature(&self) -> &[u8; 4] {
        &self.signature
    }

    /// CRC32C 校验和（MS-VHDX §2.2.2），计算时校验和字段本身置零
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }

    /// 验证头部结构的 CRC32C 校验和的正确性
    pub fn verify_checksum(&self) -> Result<()> {
        let expected = self.checksum();
        let actual = crc32c_with_zero_field(self.raw, 4, 4);
        if expected != actual {
            return Err(Error::InvalidChecksum { expected, actual });
        }
        Ok(())
    }

    /// 序列号（MS-VHDX §2.2.2），用于确定两个头部副本中哪个是活动的
    #[must_use]
    pub const fn sequence_number(&self) -> u64 {
        self.sequence_number
    }

    /// 文件写入 GUID（MS-VHDX §2.2.2），每次文件打开并写入时更新
    #[must_use]
    pub const fn file_write_guid(&self) -> Guid {
        self.file_write_guid
    }

    /// 数据写入 GUID（MS-VHDX §2.2.2），每次虚拟磁盘数据写入时更新
    #[must_use]
    pub const fn data_write_guid(&self) -> Guid {
        self.data_write_guid
    }

    /// 日志 GUID（MS-VHDX §2.2.2），标识当前活跃的日志
    #[must_use]
    pub const fn log_guid(&self) -> Guid {
        self.log_guid
    }

    /// 日志版本号（MS-VHDX §2.3.1.1），当前为 0
    #[must_use]
    pub const fn log_version(&self) -> u16 {
        self.log_version
    }

    /// VHDX 格式版本号（MS-VHDX §2.2.2），当前为 1
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// 日志区域长度（MS-VHDX §2.2.2），单位字节
    #[must_use]
    pub const fn log_length(&self) -> u32 {
        self.log_length
    }

    /// 日志区域在文件中的偏移量（MS-VHDX §2.2.2）
    #[must_use]
    pub const fn log_offset(&self) -> u64 {
        self.log_offset
    }

    /// 构造新的头部结构字节数据，自动计算 CRC32C 校验和
    #[must_use]
    pub fn create(
        sequence_number: u64, file_write_guid: Guid, data_write_guid: Guid, log_guid: Guid,
        log_length: u32, log_offset: u64,
    ) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];

        data[0..4].copy_from_slice(HEADER_SIGNATURE);
        data[4..8].copy_from_slice(&[0; 4]);
        data[8..16].copy_from_slice(&sequence_number.to_le_bytes());
        data[16..32].copy_from_slice(file_write_guid.as_bytes());
        data[32..48].copy_from_slice(data_write_guid.as_bytes());
        data[48..64].copy_from_slice(log_guid.as_bytes());
        data[64..66].copy_from_slice(&LOG_VERSION.to_le_bytes());
        data[66..68].copy_from_slice(&VHDX_VERSION.to_le_bytes());
        data[68..72].copy_from_slice(&log_length.to_le_bytes());
        data[72..80].copy_from_slice(&log_offset.to_le_bytes());

        let checksum = crc32c::crc32c(&data);
        data[4..8].copy_from_slice(&checksum.to_le_bytes());

        data
    }
}

/// 区域表（MS-VHDX §2.2.3）
///
/// 描述 VHDX 文件中各区域（如 BAT、元数据）的位置和大小。
/// VHDX 文件包含两个冗余的区域表副本。
pub struct RegionTable<'a> {
    /// 原始字节数据（私有，供 `raw()` 方法向后兼容）
    data: &'a [u8],
    /// 区域表头部（MS-VHDX §2.2.3.1）
    pub header: RegionTableHeader<'a>,
    /// 区域表条目列表（MS-VHDX §2.2.3.2）
    pub entries: Vec<RegionTableEntry<'a>>,
}

impl<'a> RegionTable<'a> {
    /// 从原始字节数据创建区域表实例，验证数据长度必须为 64KB
    ///
    /// 解析时立即提取头部和所有条目，存为公开字段。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != REGION_TABLE_SIZE {
            return Err(Error::InvalidRegionTable(format!(
                "Region Table must be {} bytes, got {}",
                REGION_TABLE_SIZE,
                data.len()
            )));
        }
        let header = RegionTableHeader::new(&data[0..16]);
        let count = header.entry_count();
        let entries: Vec<RegionTableEntry<'_>> = (0..count)
            .filter_map(|i| {
                let offset = 16 + i as usize * 32;
                if offset + 32 > data.len() {
                    return None;
                }
                RegionTableEntry::new(&data[offset..offset + 32]).ok()
            })
            .collect();
        Ok(Self {
            data,
            header,
            entries,
        })
    }

    /// 返回区域表的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 获取区域表头部（MS-VHDX §2.2.3.1）
    #[must_use]
    pub fn header(&self) -> RegionTableHeader<'_> {
        RegionTableHeader::new(&self.data[0..16])
    }

    /// 按索引获取区域表条目（MS-VHDX §2.2.3.2），每个条目 32 字节
    #[must_use]
    pub fn entry(&self, index: u32) -> Option<RegionTableEntry<'_>> {
        let header = self.header();
        if index >= header.entry_count() {
            return None;
        }
        let offset = 16 + index as usize * 32;
        if offset + 32 > self.data.len() {
            return None;
        }
        RegionTableEntry::new(&self.data[offset..offset + 32]).ok()
    }

    /// 返回所有区域表条目的列表
    #[must_use]
    pub fn entries(&self) -> Vec<RegionTableEntry<'_>> {
        let count = self.header().entry_count();
        (0..count).filter_map(|i| self.entry(i)).collect()
    }

    /// 按 GUID 查找区域表条目
    #[must_use]
    pub fn find_entry(&self, guid: &Guid) -> Option<RegionTableEntry<'_>> {
        self.entries().into_iter().find(|e| e.guid() == *guid)
    }
}

/// 区域表头部（MS-VHDX §2.2.3.1）
///
/// 包含区域表的签名、校验和和条目数量。
#[derive(Clone, Copy)]
pub struct RegionTableHeader<'a> {
    /// 区域表签名（应为 "regi"）
    pub signature: [u8; 4],
    /// CRC32C 校验和
    pub checksum: u32,
    /// 表项数量
    pub entry_count: u32,
    /// 保留字段
    pub reserved: u32,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> RegionTableHeader<'a> {
    /// 从原始字节数据创建区域表头部实例
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            signature: read_array::<4>(data, 0),
            checksum: read_u32(data, 4),
            entry_count: read_u32(data, 8),
            reserved: read_u32(data, 12),
            raw: data,
        }
    }

    /// 返回区域表头部的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 返回 4 字节签名（MS-VHDX §2.2.3.1），应为 "regi"
    #[must_use]
    pub const fn signature(&self) -> &[u8; 4] {
        &self.signature
    }

    /// CRC32C 校验和（MS-VHDX §2.2.3.1），计算时校验和字段本身置零
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }

    /// 验证 CRC32C 校验和的正确性
    pub fn verify_checksum(&self) -> Result<()> {
        let expected = self.checksum();
        let actual = crc32c_with_zero_field(self.raw, 4, 4);
        if expected != actual {
            return Err(Error::InvalidChecksum { expected, actual });
        }
        Ok(())
    }

    /// 区域表中的条目数量（MS-VHDX §2.2.3.1）
    #[must_use]
    pub const fn entry_count(&self) -> u32 {
        self.entry_count
    }
}

/// 区域表条目（MS-VHDX §2.2.3.2）
///
/// 每个条目描述一个区域，包含：
/// - 区域 GUID（16 字节）— 标识区域类型
/// - 文件偏移量（8 字节）— 区域在文件中的起始位置
/// - 长度（4 字节）— 区域大小
/// - 必需标志（4 字节）— 是否为 VHDX 文件必需的区域
#[derive(Clone, Copy)]
pub struct RegionTableEntry<'a> {
    /// 区域 GUID
    pub guid: Guid,
    /// 区域文件偏移
    pub file_offset: u64,
    /// 区域长度
    pub length: u32,
    /// required 标志（原始值）
    pub required: u32,
    /// 原始字节视图
    pub raw: &'a [u8],
}

impl<'a> RegionTableEntry<'a> {
    /// 从原始字节数据创建区域表条目实例，验证数据长度必须为 32 字节
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != 32 {
            return Err(Error::InvalidRegionTable(
                "Entry must be 32 bytes".to_string(),
            ));
        }
        Ok(Self {
            guid: read_guid(data, 0),
            file_offset: read_u64(data, 16),
            length: read_u32(data, 24),
            required: read_u32(data, 28),
            raw: data,
        })
    }

    /// 返回区域表条目的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    /// 区域 GUID（MS-VHDX §2.2.3.2），标识区域类型（如 BAT、元数据区域）
    #[must_use]
    pub const fn guid(&self) -> Guid {
        self.guid
    }

    /// 区域在文件中的偏移量（MS-VHDX §2.2.3.2），单位字节
    #[must_use]
    pub const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    /// 区域长度（MS-VHDX §2.2.3.2），单位字节
    #[must_use]
    pub const fn length(&self) -> u32 {
        self.length
    }

    /// 区域是否为 VHDX 文件必需（MS-VHDX §2.2.3.2），非零值表示必需
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_identifier() {
        let data = FileTypeIdentifier::create(Some("TestCreator"));
        let ft = FileTypeIdentifier::new(&data);
        assert_eq!(ft.signature(), FILE_TYPE_SIGNATURE);
        assert_eq!(ft.creator(), "TestCreator");
    }

    #[test]
    fn test_header_structure() {
        let guid = Guid::nil();
        let data = HeaderStructure::create(1, guid, guid, guid, 0, 0);
        let header = HeaderStructure::new(&data).unwrap();
        assert_eq!(header.sequence_number(), 1);
        assert_eq!(header.version(), 1);
        assert_eq!(header.log_version(), 0);
    }

    #[test]
    fn test_region_table_entry() {
        let mut data = [0u8; 32];
        let guid_bytes = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD,
            0x4A, 0x08,
        ];
        data[0..16].copy_from_slice(&guid_bytes);
        data[16..24].copy_from_slice(&0x0010_0000_u64.to_le_bytes());
        data[24..28].copy_from_slice(&0x0010_0000_u32.to_le_bytes());
        data[28..32].copy_from_slice(&1u32.to_le_bytes());

        let entry = RegionTableEntry::new(&data).unwrap();
        assert_eq!(entry.file_offset(), 0x0010_0000);
        assert_eq!(entry.length(), 0x0010_0000);
        assert!(entry.required());
    }
}
