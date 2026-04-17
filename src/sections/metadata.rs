//! VHDX 元数据区域解析模块
//!
//! 本模块实现了 VHDX 元数据区域（Metadata Region）的解析，对应 MS-VHDX §2.6。
//!
//! 元数据区域存储虚拟磁盘的配置参数和标识信息，包括：
//! - 文件参数（块大小、是否有父磁盘）（§2.6.2.1）
//! - 虚拟磁盘大小（§2.6.2.2）
//! - 虚拟磁盘标识符（§2.6.2.3）
//! - 逻辑/物理扇区大小（§2.6.2.4/§2.6.2.5）
//! - 父磁盘定位器（仅差分磁盘）（§2.6.2.6）
//!
//! 元数据区域的前 64KB 为元数据表（[`MetadataTable`]），
//! 包含一个固定 32 字节的表头和多个 32 字节的表项，
//! 每个表项通过 GUID 标识元数据类型，并指向表后的变长数据。

use crate::common::constants::{METADATA_TABLE_SIZE, metadata_guids};
use crate::error::{Error, Result};
use crate::types::Guid;

/// VHDX 元数据区域（MS-VHDX §2.6）
///
/// 包装元数据区域的原始数据，提供对元数据表和类型化元数据项的访问。
pub struct Metadata {
    /// 元数据区域的原始字节数据
    raw_data: Vec<u8>,
}

impl Metadata {
    /// 从原始字节数据创建元数据区域实例
    ///
    /// 数据长度必须至少为 `METADATA_TABLE_SIZE`（64KB），否则返回错误。
    pub fn new(data: Vec<u8>) -> Result<Self> {
        if data.len() < METADATA_TABLE_SIZE {
            return Err(Error::InvalidMetadata(format!(
                "Metadata section must be at least {} bytes, got {}",
                METADATA_TABLE_SIZE,
                data.len()
            )));
        }
        Ok(Self { raw_data: data })
    }

    /// 返回元数据区域的原始字节数据引用
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// 返回元数据表（前 64KB）的视图（MS-VHDX §2.6.1）
    pub fn table(&self) -> MetadataTable<'_> {
        MetadataTable::new(&self.raw_data[..METADATA_TABLE_SIZE])
    }

    /// 返回类型化元数据访问器，用于便捷访问已知元数据项（MS-VHDX §2.6.2）
    pub const fn items(&self) -> MetadataItems<'_> {
        MetadataItems::new(self)
    }
}

/// 元数据表（MS-VHDX §2.6.1）
///
/// 固定 64KB，包含表头和变长表项数组。
/// 表项不一定按特定顺序排列。
pub struct MetadataTable<'a> {
    data: &'a [u8],
}

impl<'a> MetadataTable<'a> {
    /// 从字节数据创建元数据表视图
    ///
    /// 数据应恰好为 `METADATA_TABLE_SIZE`（64KB）字节。
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// 返回元数据表的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 返回元数据表头部（前 32 字节）（MS-VHDX §2.6.1.1）
    #[must_use]
    pub fn header(&self) -> TableHeader<'_> {
        TableHeader::new(&self.data[0..32])
    }

    /// 根据 GUID 查找元数据表项（MS-VHDX §2.6.1.2）
    ///
    /// 遍历所有表项，返回匹配指定 `item_id` 的第一个表项。
    #[must_use]
    pub fn entry(&self, item_id: &Guid) -> Option<TableEntry<'_>> {
        self.entries().into_iter().find(|e| e.item_id() == *item_id)
    }

    /// 返回所有有效的元数据表项（MS-VHDX §2.6.1.2）
    #[must_use]
    pub fn entries(&self) -> Vec<TableEntry<'_>> {
        let count = self.header().entry_count();
        (0..count).filter_map(|i| self.entry_by_index(i)).collect()
    }

    /// 根据索引获取元数据表项
    ///
    /// 表项从偏移量 32 开始，每项固定 32 字节。
    fn entry_by_index(&self, index: u16) -> Option<TableEntry<'_>> {
        let header = self.header();
        if index >= header.entry_count() {
            return None;
        }
        let offset = 32 + index as usize * 32;
        if offset + 32 > self.data.len() {
            return None;
        }
        TableEntry::new(&self.data[offset..offset + 32]).ok()
    }
}

/// 元数据表头部（MS-VHDX §2.6.1.1）
///
/// 固定 32 字节，包含签名 "metadata" 和表项数量。
pub struct TableHeader<'a> {
    data: &'a [u8],
}

impl<'a> TableHeader<'a> {
    /// 从 32 字节数据创建表头部视图
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// 返回表头部的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 表签名 "metadata"（MS-VHDX §2.6.1.1）
    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.data[0..8]
    }

    /// 元数据表项数量（MS-VHDX §2.6.1.1）
    #[must_use]
    pub fn entry_count(&self) -> u16 {
        u16::from_le_bytes(self.data[10..12].try_into().unwrap())
    }
}

/// 元数据表项（MS-VHDX §2.6.1.2）
///
/// 每个表项固定 32 字节，描述一个元数据项的位置和属性：
/// - ItemID（16 字节）— 元数据项的 GUID 标识符
/// - Offset（4 字节）— 数据在元数据区域中的偏移量
/// - Length（4 字节）— 数据长度
/// - Flags（4 字节）— 属性标志位（IsUser / IsVirtualDisk / IsRequired）
pub struct TableEntry<'a> {
    data: &'a [u8],
}

impl<'a> TableEntry<'a> {
    /// 从 32 字节数据创建表项
    ///
    /// 数据长度必须恰好为 32 字节，否则返回错误。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != 32 {
            return Err(Error::InvalidMetadata("Entry must be 32 bytes".to_string()));
        }
        Ok(Self { data })
    }

    /// 返回表项的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 元数据项的 GUID 标识符（MS-VHDX §2.6.1.2），用于查找特定元数据
    #[must_use]
    pub fn item_id(&self) -> Guid {
        Guid::from_bytes(self.data[0..16].try_into().unwrap())
    }

    /// 元数据项数据在元数据区域中的偏移量（MS-VHDX §2.6.1.2）
    #[must_use]
    pub fn offset(&self) -> u32 {
        u32::from_le_bytes(self.data[16..20].try_into().unwrap())
    }

    /// 元数据项数据的字节长度（MS-VHDX §2.6.1.2）
    #[must_use]
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[20..24].try_into().unwrap())
    }

    /// 元数据项的属性标志位（MS-VHDX §2.6.1.2）
    #[must_use]
    pub fn flags(&self) -> EntryFlags {
        EntryFlags(u32::from_le_bytes(self.data[24..28].try_into().unwrap()))
    }
}

/// 元数据表项标志位（MS-VHDX §2.6.1.2）
///
/// 3 个标志位的位位置：
/// - bit 31（0x80000000）— `is_user`：是否为用户元数据（非系统定义）
/// - bit 30（0x40000000）— `is_virtual_disk`：是否与虚拟磁盘相关（vs. 文件元数据）
/// - bit 29（0x20000000）— `is_required`：是否为必需项（缺失则文件无效）
#[derive(Clone, Copy, Debug)]
pub struct EntryFlags(pub u32);

impl EntryFlags {
    /// 是否为用户自定义元数据（bit 31, 0x80000000）
    ///
    /// 系统定义的元数据此位为 0，用户自定义元数据此位为 1。
    #[must_use]
    pub const fn is_user(&self) -> bool {
        (self.0 & 0x8000_0000) != 0
    }

    /// 是否与虚拟磁盘相关（bit 30, 0x40000000）
    ///
    /// 为 1 表示该元数据描述虚拟磁盘属性，为 0 表示描述文件级属性。
    #[must_use]
    pub const fn is_virtual_disk(&self) -> bool {
        (self.0 & 0x4000_0000) != 0
    }

    /// 是否为必需元数据项（bit 29, 0x20000000）
    ///
    /// 为 1 表示该元数据项是必需的，缺失则 VHDX 文件无效。
    #[must_use]
    pub const fn is_required(&self) -> bool {
        (self.0 & 0x2000_0000) != 0
    }
}

/// 类型化元数据访问器（MS-VHDX §2.6.2）
///
/// 提供对已知元数据项的便捷类型化访问。
/// 每个方法通过 GUID 查找对应的元数据表项，然后解析数据。
pub struct MetadataItems<'a> {
    metadata: &'a Metadata,
}

impl<'a> MetadataItems<'a> {
    /// 从元数据区域引用创建类型化访问器
    #[must_use]
    pub const fn new(metadata: &'a Metadata) -> Self {
        Self { metadata }
    }

    /// 根据 GUID 获取元数据项的原始字节数据
    ///
    /// 在元数据表中查找匹配 GUID 的表项，然后根据表项的偏移和长度提取数据。
    fn get_item_data(&self, guid: &Guid) -> Option<&'a [u8]> {
        let table = self.metadata.table();
        let entry = table.entry(guid)?;
        let offset = entry.offset() as usize;
        let length = entry.length() as usize;
        self.metadata.raw_data.get(offset..offset + length)
    }

    /// 获取文件参数元数据（MS-VHDX §2.6.2.1）
    ///
    /// 返回块大小和标志位等基本文件配置参数。
    #[must_use]
    pub fn file_parameters(&self) -> Option<FileParameters> {
        let data = self.get_item_data(&metadata_guids::FILE_PARAMETERS)?;
        if data.len() < 8 {
            return None;
        }
        Some(FileParameters::from_bytes(data))
    }

    /// 获取虚拟磁盘大小（MS-VHDX §2.6.2.2）
    ///
    /// 返回虚拟磁盘的逻辑大小（字节），8 字节无符号整数。
    #[must_use]
    pub fn virtual_disk_size(&self) -> Option<u64> {
        let data = self.get_item_data(&metadata_guids::VIRTUAL_DISK_SIZE)?;
        if data.len() < 8 {
            return None;
        }
        Some(u64::from_le_bytes(data[0..8].try_into().unwrap()))
    }

    /// 获取虚拟磁盘标识符（MS-VHDX §2.6.2.3）
    ///
    /// 返回唯一标识虚拟磁盘的 GUID。
    #[must_use]
    pub fn virtual_disk_id(&self) -> Option<Guid> {
        let data = self.get_item_data(&metadata_guids::VIRTUAL_DISK_ID)?;
        if data.len() < 16 {
            return None;
        }
        Some(Guid::from_bytes(data[0..16].try_into().unwrap()))
    }

    /// 获取逻辑扇区大小（MS-VHDX §2.6.2.4）
    ///
    /// 返回虚拟磁盘的逻辑扇区大小（字节），通常为 512 或 4096。
    #[must_use]
    pub fn logical_sector_size(&self) -> Option<u32> {
        let data = self.get_item_data(&metadata_guids::LOGICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes(data[0..4].try_into().unwrap()))
    }

    /// 获取物理扇区大小（MS-VHDX §2.6.2.5）
    ///
    /// 返回底层物理磁盘的扇区大小（字节），用于对齐优化。
    #[must_use]
    pub fn physical_sector_size(&self) -> Option<u32> {
        let data = self.get_item_data(&metadata_guids::PHYSICAL_SECTOR_SIZE)?;
        if data.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes(data[0..4].try_into().unwrap()))
    }

    /// 获取父磁盘定位器（MS-VHDX §2.6.2.6）
    ///
    /// 仅用于差分 VHDX 文件，返回父磁盘文件的位置信息。
    #[must_use]
    pub fn parent_locator(&self) -> Option<ParentLocator<'_>> {
        let data = self.get_item_data(&metadata_guids::PARENT_LOCATOR)?;
        ParentLocator::new(data).ok()
    }
}

/// 文件参数元数据（MS-VHDX §2.6.2.1）
///
/// 描述 VHDX 文件的基本配置参数，固定 8 字节：
/// - block_size（4 字节）— 块大小，必须为 1MB 的幂次（1MB-256MB）
/// - flags（4 字节）— 标志位
///   - bit 0: LeaveBlockAllocated — 删除块时是否保留空间
///   - bit 1: HasParent — 是否为差分磁盘（有父磁盘）
#[derive(Clone, Copy, Debug)]
pub struct FileParameters {
    block_size: u32,
    flags: u32,
}

impl FileParameters {
    /// 从原始字节数据解析文件参数（MS-VHDX §2.6.2.1）
    ///
    /// 数据必须至少 8 字节：前 4 字节为块大小，后 4 字节为标志位。
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            block_size: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            flags: u32::from_le_bytes(data[4..8].try_into().unwrap()),
        }
    }

    /// 块大小（字节），必须为 1MB 到 256MB 之间的 2 的幂次（MS-VHDX §2.6.2.1）
    #[must_use]
    pub const fn block_size(&self) -> u32 {
        self.block_size
    }

    /// 是否在块被释放后保留空间分配（MS-VHDX §2.6.2.1），bit 0
    #[must_use]
    pub const fn leave_block_allocated(&self) -> bool {
        (self.flags & 0x01) != 0
    }

    /// 是否为差分 VHDX 文件（MS-VHDX §2.6.2.1），bit 1
    #[must_use]
    pub const fn has_parent(&self) -> bool {
        (self.flags & 0x02) != 0
    }

    /// 原始标志位值
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// 父磁盘定位器（MS-VHDX §2.6.2.6）
///
/// 仅用于差分 VHDX 文件，描述如何定位父磁盘文件。
/// 由定位器头部和键值对条目组成，键值以 UTF-16 LE 编码。
pub struct ParentLocator<'a> {
    data: &'a [u8],
}

impl<'a> ParentLocator<'a> {
    /// 从原始字节数据创建父磁盘定位器
    ///
    /// 数据长度必须至少 20 字节（定位器头部大小）。
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 20 {
            return Err(Error::InvalidMetadata(
                "Parent Locator too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    /// 返回父磁盘定位器的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 返回定位器头部（前 20 字节）（MS-VHDX §2.6.2.6.1）
    #[must_use]
    pub fn header(&self) -> LocatorHeader<'_> {
        LocatorHeader::new(&self.data[0..20])
    }

    /// 根据索引获取键值对条目（MS-VHDX §2.6.2.6.2）
    ///
    /// 条目从偏移量 20 开始，每项固定 12 字节。
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<KeyValueEntry> {
        let header = self.header();
        if index >= header.key_value_count() as usize {
            return None;
        }
        let offset = 20 + index * 12;
        if offset + 12 > self.data.len() {
            return None;
        }
        KeyValueEntry::new(&self.data[offset..offset + 12]).ok()
    }

    /// 返回所有键值对条目（MS-VHDX §2.6.2.6.2）
    #[must_use]
    pub fn entries(&self) -> Vec<KeyValueEntry> {
        let count = self.header().key_value_count();
        (0..count).filter_map(|i| self.entry(i as usize)).collect()
    }

    /// 返回键值对数据区域
    ///
    /// 该区域位于头部和所有键值对条目之后，存储 UTF-16 LE 编码的键和值数据。
    #[must_use]
    pub fn key_value_data(&self) -> &[u8] {
        let header = self.header();
        let entries_size = 20 + header.key_value_count() as usize * 12;
        if entries_size > self.data.len() {
            return &[];
        }
        &self.data[entries_size..]
    }
}

/// 父磁盘定位器头部（MS-VHDX §2.6.2.6.1）
///
/// 固定 20 字节，包含定位器类型 GUID 和键值对数量。
pub struct LocatorHeader<'a> {
    data: &'a [u8],
}

impl<'a> LocatorHeader<'a> {
    /// 从 20 字节数据创建定位器头部视图
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// 返回定位器头部的原始字节数据
    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    /// 定位器类型 GUID（MS-VHDX §2.6.2.6.1）
    ///
    /// 标识定位器的类型，例如 VHDX 父磁盘定位器。
    #[must_use]
    pub fn locator_type(&self) -> Guid {
        Guid::from_bytes(self.data[0..16].try_into().unwrap())
    }

    /// 键值对条目数量（MS-VHDX §2.6.2.6.1）
    #[must_use]
    pub fn key_value_count(&self) -> u16 {
        u16::from_le_bytes(self.data[18..20].try_into().unwrap())
    }
}

/// 父磁盘定位器键值对条目（MS-VHDX §2.6.2.6.2）
///
/// 每个条目固定 12 字节，描述一个键值对的偏移和长度。
/// 键和值均以 UTF-16 LE 编码存储在定位器的数据区域中。
#[derive(Clone, Copy, Debug)]
pub struct KeyValueEntry {
    /// 键数据在定位器数据区域中的偏移量（字节）
    key_offset: u32,
    /// 值数据在定位器数据区域中的偏移量（字节）
    value_offset: u32,
    /// 键数据的字节长度（UTF-16 LE 编码）
    key_length: u16,
    /// 值数据的字节长度（UTF-16 LE 编码）
    value_length: u16,
}

impl KeyValueEntry {
    /// 从 12 字节数据解析键值对条目
    ///
    /// 数据布局：key_offset(4) + value_offset(4) + key_length(2) + value_length(2)
    pub fn new(data: &[u8]) -> Result<Self> {
        if data.len() != 12 {
            return Err(Error::InvalidMetadata(
                "Key-Value Entry must be 12 bytes".to_string(),
            ));
        }
        Ok(Self {
            key_offset: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            value_offset: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            key_length: u16::from_le_bytes(data[8..10].try_into().unwrap()),
            value_length: u16::from_le_bytes(data[10..12].try_into().unwrap()),
        })
    }

    /// 将键值对条目序列化为 12 字节数组
    #[must_use]
    pub fn raw(&self) -> [u8; 12] {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&self.key_offset.to_le_bytes());
        data[4..8].copy_from_slice(&self.value_offset.to_le_bytes());
        data[8..10].copy_from_slice(&self.key_length.to_le_bytes());
        data[10..12].copy_from_slice(&self.value_length.to_le_bytes());
        data
    }

    /// 从定位器数据区域中读取键字符串
    ///
    /// 根据 key_offset 和 key_length 从 data 中提取 UTF-16 LE 编码的字节，
    /// 并解码为 String（遇到空字符截断）。
    #[must_use]
    pub fn key(&self, data: &[u8]) -> Option<String> {
        let start = self.key_offset as usize;
        let end = start + self.key_length as usize;
        let key_data = data.get(start..end)?;

        // UTF-16 LE 解码，遇到空字符停止
        let utf16: Vec<u16> = key_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&utf16))
    }

    /// 从定位器数据区域中读取值字符串
    ///
    /// 根据 value_offset 和 value_length 从 data 中提取 UTF-16 LE 编码的字节，
    /// 并解码为 String（遇到空字符截断）。
    #[must_use]
    pub fn value(&self, data: &[u8]) -> Option<String> {
        let start = self.value_offset as usize;
        let end = start + self.value_length as usize;
        let value_data = data.get(start..end)?;

        // UTF-16 LE 解码，遇到空字符停止
        let utf16: Vec<u16> = value_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&utf16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_flags() {
        let flags = EntryFlags(0xE000_0000);
        assert!(flags.is_user());
        assert!(flags.is_virtual_disk());
        assert!(flags.is_required());

        let flags = EntryFlags(0);
        assert!(!flags.is_user());
        assert!(!flags.is_virtual_disk());
        assert!(!flags.is_required());
    }

    #[test]
    fn test_file_parameters() {
        let data = [0x00, 0x00, 0x00, 0x02, 0x03, 0x00, 0x00, 0x00];
        let fp = FileParameters::from_bytes(&data);
        assert_eq!(fp.block_size(), 0x0200_0000);
        assert!(fp.leave_block_allocated());
        assert!(fp.has_parent());
    }

    #[test]
    fn test_key_value_entry() {
        let mut kv_data = vec![0u8; 100];
        let key = "parent_linkage";
        for (i, c) in key.encode_utf16().enumerate() {
            kv_data[i * 2..i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        let value = "parent.vhdx";
        for (i, c) in value.encode_utf16().enumerate() {
            kv_data[32 + i * 2..32 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }

        let entry = KeyValueEntry {
            key_offset: 0,
            value_offset: 32,
            key_length: u16::try_from(key.len() * 2).unwrap_or(0),
            value_length: u16::try_from(value.len() * 2).unwrap_or(0),
        };

        assert_eq!(entry.key(&kv_data).unwrap(), key);
        assert_eq!(entry.value(&kv_data).unwrap(), value);
    }
}
