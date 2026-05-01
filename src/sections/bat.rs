//! 块分配表（BAT）解析模块
//!
//! 本模块实现了 VHDX 块分配表（Block Allocation Table）的解析，对应 MS-VHDX §2.5。
//!
//! BAT 将虚拟磁盘的逻辑块地址映射到 VHDX 文件中的物理偏移量。
//! 每个 BAT 条目为 8 字节（64 位），采用位字段编码：
//! - 低 3 位：块状态（Payload 或 `SectorBitmap`）
//! - 中间 17 位：保留
//! - 高 44 位：文件偏移量（以 MB 为单位）
//!
//! BAT 中的条目按 Payload Block 和 Sector Bitmap Block 交错排列，
//! 交错间隔由块比率（Chunk ratio）决定（MS-VHDX §2.5）。

use crate::common::constants::{BAT_ENTRY_SIZE, CHUNK_RATIO_CONSTANT, MiB};
use crate::error::{Error, Result};
use std::marker::PhantomData;

/// 块分配表（BAT）（MS-VHDX §2.5）
///
/// 包装 BAT 的原始数据和条目计数。
/// BAT 条目按 Payload Block 和 Sector Bitmap Block 交错排列，
/// 总条目数 = Payload Block 数 + Sector Bitmap Block 数。
pub struct Bat<'a> {
    /// BAT 的原始字节数据
    raw_data: Vec<u8>,
    /// BAT 条目总数（Payload + Sector Bitmap）
    entry_count: usize,
    /// 预解析的 BAT 条目列表
    entries: Vec<BatEntry>,
    marker: PhantomData<&'a [u8]>,
}

impl Bat<'_> {
    /// 从原始数据创建 BAT 实例，验证数据长度是否足够容纳指定数量的条目
    ///
    /// 使用实际的 `logical_sector_size` 和 `block_size` 计算 chunk ratio，
    /// 而非硬编码默认值（MS-VHDX §2.5）。
    pub fn new(
        data: Vec<u8>, entry_count: u64, logical_sector_size: u32, block_size: u32,
    ) -> Result<Self> {
        let entry_count: usize = entry_count.try_into().map_err(|_| {
            Error::InvalidFile(format!("entry_count {entry_count} exceeds usize::MAX"))
        })?;
        let expected_size = entry_count
            .checked_mul(BAT_ENTRY_SIZE)
            .ok_or_else(|| Error::InvalidFile("BAT size overflow".to_string()))?;
        if data.len() < expected_size {
            return Err(Error::InvalidFile(format!(
                "BAT data too small: expected at least {} bytes, got {}",
                expected_size,
                data.len()
            )));
        }
        let chunk_ratio: usize = Self::calculate_chunk_ratio(logical_sector_size, block_size)
            .try_into()
            .map_err(|_| Error::InvalidFile("chunk ratio exceeds usize::MAX".to_string()))?;
        if chunk_ratio == 0 {
            return Err(Error::InvalidFile(
                "default chunk ratio must be non-zero".to_string(),
            ));
        }
        let sector_bitmap_blocks = entry_count.div_ceil(chunk_ratio + 1);
        let payload_blocks = entry_count.saturating_sub(sector_bitmap_blocks);

        // 预解析所有 BAT 条目
        let entries: Vec<BatEntry> = (0..entry_count)
            .map(|i| {
                let offset = i * BAT_ENTRY_SIZE;
                let raw_value = u64::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]);
                let is_sector_bitmap_entry =
                    Self::is_sector_bitmap_entry_index(i, chunk_ratio, payload_blocks);
                BatEntry::from_raw_with_context(raw_value, is_sector_bitmap_entry)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            raw_data: data,
            entry_count,
            entries,
            marker: PhantomData,
        })
    }

    /// 返回 BAT 的原始字节数据
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    /// 获取指定索引处的 BAT 条目
    #[must_use]
    pub fn entry(&self, index: u64) -> Option<BatEntry> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.entries.get(i).copied())
    }

    /// 返回所有预解析的 BAT 条目（拥有所有权的副本）
    #[must_use]
    pub fn entries(&self) -> Vec<BatEntry> {
        self.entries.clone()
    }

    /// 返回 BAT 中的条目总数
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entry_count
    }

    /// 检查 BAT 是否为空（无条目）
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// 计算块比率（MS-VHDX §2.5），公式: (2^23 × 逻辑扇区大小) / 块大小
    #[must_use]
    pub(crate) fn calculate_chunk_ratio(logical_sector_size: u32, block_size: u32) -> u32 {
        let result =
            (CHUNK_RATIO_CONSTANT * u64::from(logical_sector_size)) / u64::from(block_size);
        result.try_into().unwrap_or(u32::MAX)
    }

    /// 计算虚拟磁盘所需的 Payload Block 数量，公式: ceil(虚拟磁盘大小 / 块大小)
    #[must_use]
    pub(crate) fn calculate_payload_blocks(virtual_disk_size: u64, block_size: u32) -> u64 {
        virtual_disk_size.div_ceil(u64::from(block_size))
    }

    /// 计算所需的 Sector Bitmap Block 数量，公式: ceil(Payload Block 数 / 块比率)
    #[must_use]
    pub(crate) fn calculate_sector_bitmap_blocks(payload_blocks: u64, chunk_ratio: u32) -> u64 {
        payload_blocks.div_ceil(u64::from(chunk_ratio))
    }

    /// 计算 BAT 总条目数 = Payload Block 数 + Sector Bitmap Block 数
    #[must_use]
    pub(crate) fn calculate_total_entries(
        virtual_disk_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> u64 {
        let payload_blocks = Self::calculate_payload_blocks(virtual_disk_size, block_size);
        let chunk_ratio = Self::calculate_chunk_ratio(logical_sector_size, block_size);
        let sector_bitmap_blocks =
            Self::calculate_sector_bitmap_blocks(payload_blocks, chunk_ratio);
        payload_blocks + sector_bitmap_blocks
    }

    /// 判断指定 BAT 条目索引是否属于 Sector Bitmap 条目
    ///
    /// BAT 条目按 `chunk_ratio` 个 Payload 条目后接 1 个 Sector Bitmap 条目交错排列。
    /// 因此在 0 基索引下，满足 `index % (chunk_ratio + 1) == chunk_ratio` 的条目为 Sector Bitmap。
    #[must_use]
    pub(crate) const fn is_sector_bitmap_entry_index(
        index: usize, chunk_ratio: usize, payload_blocks: usize,
    ) -> bool {
        if chunk_ratio == 0 {
            return false;
        }

        let chunk_size = chunk_ratio + 1;
        let chunk_index = index / chunk_size;
        let position_in_chunk = index % chunk_size;
        let payload_start = chunk_index * chunk_ratio;

        if payload_start >= payload_blocks {
            return false;
        }

        let remaining_payload = payload_blocks - payload_start;
        let payload_in_chunk = if remaining_payload < chunk_ratio {
            remaining_payload
        } else {
            chunk_ratio
        };

        position_in_chunk == payload_in_chunk
    }
}

/// BAT 条目（MS-VHDX §2.5.1）
///
/// 每个条目为 64 位，位字段布局如下：
/// ```text
/// | 位范围  | 大小   | 含义                      |
/// |---------|--------|---------------------------|
/// | [0:2]   | 3 位   | 块状态（BatState）        |
/// | [3:19]  | 17 位  | 保留（必须为零）          |
/// | [20:63] | 44 位  | 文件偏移量（以 MB 为单位）|
/// ```
///
/// 文件偏移量为 0 时，表示该块没有分配文件空间。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatEntry {
    /// 块状态（Payload 或 Sector Bitmap）
    state: BatState,
    /// 文件偏移量（以 MB 为单位）
    file_offset_mb: u64,
}

impl BatEntry {
    /// 从原始 64 位值解析 BAT 条目，提取低 3 位状态和高 44 位偏移量
    #[allow(dead_code)]
    pub(crate) fn from_raw(raw: u64) -> std::result::Result<Self, Error> {
        Self::from_raw_with_context(raw, false)
    }

    /// 从原始 64 位值解析 BAT 条目，并根据条目类型上下文选择状态映射路径
    pub(crate) fn from_raw_with_context(
        raw: u64, is_sector_bitmap_entry: bool,
    ) -> std::result::Result<Self, Error> {
        // 低 3 位为块状态
        let state_bits = (raw & 0x7) as u8;
        // 高 44 位为文件偏移量（以 MB 为单位）
        let offset_mb = raw >> 20;

        let state = BatState::from_bits_with_context(state_bits, is_sector_bitmap_entry)?;

        Ok(Self {
            state,
            file_offset_mb: offset_mb,
        })
    }

    /// 将 BAT 条目编码回 64 位原始值
    #[must_use]
    pub fn raw(&self) -> u64 {
        let state_bits = u64::from(self.state.to_bits());
        (self.file_offset_mb << 20) | state_bits
    }

    /// 块状态（MS-VHDX §2.5.1）
    #[must_use]
    pub const fn state(&self) -> BatState {
        self.state
    }

    /// 文件偏移量（以 MB 为单位）（MS-VHDX §2.5.1）
    #[must_use]
    pub const fn file_offset_mb(&self) -> u64 {
        self.file_offset_mb
    }

    /// 返回文件偏移量（以字节为单位），将 MB 值转换为字节数
    #[must_use]
    pub const fn file_offset(&self) -> u64 {
        self.file_offset_mb * MiB
    }

    /// 使用指定的状态和偏移量创建新的 BAT 条目（crate 内部）
    #[must_use]
    pub(crate) const fn new(state: BatState, file_offset_mb: u64) -> Self {
        Self {
            state,
            file_offset_mb,
        }
    }

    /// 使用指定的状态和偏移量创建新的 BAT 条目（公共构造入口）
    ///
    /// 适用于需要手动构造 BAT 条目的场景（如测试）。
    #[must_use]
    pub const fn create(state: BatState, file_offset_mb: u64) -> Self {
        Self {
            state,
            file_offset_mb,
        }
    }
}

impl Bat<'_> {
    /// 更新指定索引处的 BAT 条目（同时更新内存缓存和原始数据）
    ///
    /// 用于 Dynamic 类型写入时自动分配 payload block 后更新 BAT 条目。
    /// 更新 `entries` 向量和 `raw_data` 字节数组以保持两者同步。
    pub(crate) fn update_entry(
        &mut self, index: usize, state: BatState, file_offset_mb: u64,
    ) -> Result<()> {
        if index >= self.entry_count {
            return Err(Error::InvalidParameter(format!(
                "BAT update index {index} out of range (entry_count={})",
                self.entry_count
            )));
        }
        let entry = BatEntry::new(state, file_offset_mb);
        self.entries[index] = entry;
        let offset = index * BAT_ENTRY_SIZE;
        self.raw_data[offset..offset + BAT_ENTRY_SIZE].copy_from_slice(&entry.raw().to_le_bytes());
        Ok(())
    }
}

/// BAT 条目的块状态（MS-VHDX §2.5.1）
///
/// 根据状态值的不同，BAT 条目可能代表 Payload Block 或 Sector Bitmap Block。
/// 有效的 Payload 块状态值为 0-3、6、7，无效值（4、5）会触发错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatState {
    /// Payload Block 状态
    Payload(PayloadBlockState),
    /// Sector Bitmap Block 状态
    SectorBitmap(SectorBitmapState),
}

impl BatState {
    /// 从 3 位状态值解析块状态
    ///
    /// 值 0-3、6、7 映射为 Payload Block 状态，值 4、5 为无效状态。
    pub const fn from_bits(bits: u8) -> std::result::Result<Self, Error> {
        Self::from_bits_with_context(bits, false)
    }

    /// 从 3 位状态值解析块状态，并根据条目类型上下文决定 Payload/Sector Bitmap 语义
    pub const fn from_bits_with_context(
        bits: u8, is_sector_bitmap_entry: bool,
    ) -> std::result::Result<Self, Error> {
        if is_sector_bitmap_entry {
            match bits {
                0 | 6 => Ok(Self::SectorBitmap(SectorBitmapState::from_bits(bits))),
                _ => Err(Error::InvalidBlockState(bits)),
            }
        } else {
            match bits {
                0 | 1 | 2 | 3 | 6 | 7 => Ok(Self::Payload(PayloadBlockState::from_bits(bits))),
                _ => Err(Error::InvalidBlockState(bits)),
            }
        }
    }

    /// 将块状态编码回 3 位状态值
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::Payload(state) => state.to_bits(),
            Self::SectorBitmap(state) => state.to_bits(),
        }
    }
}

/// Payload Block 状态枚举（MS-VHDX §2.5.1.1）
///
/// 定义了 Payload Block 的 6 种有效状态，每种状态有不同的读写语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadBlockState {
    /// 块不存在（状态值 0）— 默认状态，块内容未定义，读取时返回零
    NotPresent = 0,
    /// 块未定义（状态值 1）— 块可能包含历史数据，不应依赖其内容
    Undefined = 1,
    /// 块内容为零（状态值 2）— 块的所有字节为零，无对应文件数据
    Zero = 2,
    /// 块已 UNMAP（状态值 3）— 已被 TRIM/UNMAP 操作释放，内容为零或历史数据
    Unmapped = 3,
    /// 块数据完全存在（状态值 6）— 块的全部数据存储在 VHDX 文件中
    FullyPresent = 6,
    /// 块数据部分存在（状态值 7）— 仅用于差分 VHDX，需检查扇区位图确定哪些扇区有效
    PartiallyPresent = 7,
}

impl PayloadBlockState {
    /// 从状态值解析 Payload Block 状态，未知值回退为 `NotPresent`
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Undefined,
            2 => Self::Zero,
            3 => Self::Unmapped,
            6 => Self::FullyPresent,
            7 => Self::PartiallyPresent,
            _ => Self::NotPresent,
        }
    }

    /// 将 Payload Block 状态转换为状态值
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::NotPresent => 0,
            Self::Undefined => 1,
            Self::Zero => 2,
            Self::Unmapped => 3,
            Self::FullyPresent => 6,
            Self::PartiallyPresent => 7,
        }
    }

    /// 检查块是否已分配文件空间（FullyPresent 或 `PartiallyPresent`）
    #[must_use]
    pub const fn is_allocated(&self) -> bool {
        matches!(self, Self::FullyPresent | Self::PartiallyPresent)
    }

    /// 检查读取该块时是否需要从 VHDX 文件中读取数据
    ///
    /// FullyPresent、PartiallyPresent 和 Undefined 状态需要实际 I/O 操作。
    #[must_use]
    pub const fn needs_read(&self) -> bool {
        matches!(
            self,
            Self::FullyPresent | Self::PartiallyPresent | Self::Undefined
        )
    }
}

/// Sector Bitmap Block 状态枚举（MS-VHDX §2.5.1.2）
///
/// Sector Bitmap 用于差分 VHDX 中跟踪每个扇区是否存在于差分文件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorBitmapState {
    /// 扇区位图不存在（状态值 0）— 所有扇区均不存在于差分文件
    NotPresent = 0,
    /// 扇区位图存在（状态值 6）— 位图数据存储在 VHDX 文件中
    Present = 6,
}

impl SectorBitmapState {
    /// 从状态值解析 Sector Bitmap Block 状态，值 6 为 Present，其余为 `NotPresent`
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            6 => Self::Present,
            _ => Self::NotPresent,
        }
    }

    /// 将 Sector Bitmap Block 状态转换为状态值
    #[must_use]
    pub const fn to_bits(&self) -> u8 {
        match self {
            Self::NotPresent => 0,
            Self::Present => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用默认参数（与之前硬编码的默认值一致）
    const TEST_LOGICAL_SECTOR_SIZE: u32 = 512;
    const TEST_BLOCK_SIZE: u32 = 32 * 1024 * 1024;

    #[test]
    fn test_bat_entry_from_raw() {
        let raw = (1u64 << 20) | 6u64;
        let entry = BatEntry::from_raw(raw).unwrap();

        assert_eq!(entry.file_offset_mb, 1);
        assert_eq!(entry.file_offset(), MiB);
        assert!(matches!(
            entry.state,
            BatState::Payload(PayloadBlockState::FullyPresent)
        ));
    }

    #[test]
    fn test_bat_entry_to_raw() {
        let entry = BatEntry::new(BatState::Payload(PayloadBlockState::FullyPresent), 1);
        let raw = entry.raw();
        assert_eq!(raw & 0x7, 6);
        assert_eq!(raw >> 20, 1);
    }

    #[test]
    fn test_bat_entry_from_raw_sector_bitmap_context() {
        let raw = (3u64 << 20) | 6u64;
        let entry = BatEntry::from_raw_with_context(raw, true).unwrap();

        assert_eq!(entry.file_offset_mb, 3);
        assert!(matches!(
            entry.state,
            BatState::SectorBitmap(SectorBitmapState::Present)
        ));
    }

    #[test]
    fn test_bat_entry_from_raw_sector_bitmap_context_rejects_invalid_state() {
        let raw = (3u64 << 20) | 7u64;
        let err = BatEntry::from_raw_with_context(raw, true).unwrap_err();
        assert!(matches!(err, Error::InvalidBlockState(7)));
    }

    #[test]
    fn test_bat_new_parses_sector_bitmap_via_entry_and_entries() {
        let entry_count = 129usize;
        let mut data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        // 索引 128（默认 chunk ratio=128）为 Sector Bitmap 条目
        let raw_bitmap = (3u64 << 20) | 6u64;
        let bitmap_offset = 128 * BAT_ENTRY_SIZE;
        data[bitmap_offset..bitmap_offset + BAT_ENTRY_SIZE]
            .copy_from_slice(&raw_bitmap.to_le_bytes());

        let bat = Bat::new(
            data,
            entry_count as u64,
            TEST_LOGICAL_SECTOR_SIZE,
            TEST_BLOCK_SIZE,
        )
        .expect("BAT creation should succeed");

        let entry = bat.entry(128).expect("entry 128 should exist");
        assert!(matches!(
            entry.state,
            BatState::SectorBitmap(SectorBitmapState::Present)
        ));

        let entries = bat.entries();
        assert!(matches!(
            entries[128].state,
            BatState::SectorBitmap(SectorBitmapState::Present)
        ));
    }

    #[test]
    fn test_bat_new_rejects_invalid_sector_bitmap_state() {
        let entry_count = 129usize;
        let mut data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        // 索引 128（默认 chunk ratio=128）为 Sector Bitmap 条目，状态 7 非法
        let raw_invalid_bitmap = (1u64 << 20) | 7u64;
        let bitmap_offset = 128 * BAT_ENTRY_SIZE;
        data[bitmap_offset..bitmap_offset + BAT_ENTRY_SIZE]
            .copy_from_slice(&raw_invalid_bitmap.to_le_bytes());

        assert!(matches!(
            Bat::new(
                data,
                entry_count as u64,
                TEST_LOGICAL_SECTOR_SIZE,
                TEST_BLOCK_SIZE
            ),
            Err(Error::InvalidBlockState(7))
        ));
    }

    #[test]
    fn test_bat_new_parses_sector_bitmap_in_partial_final_chunk() {
        // 默认 chunk ratio=128，entry_count=131 => payload=129, bitmap=2，bitmap 索引应为 128 和 130
        let entry_count = 131usize;
        let mut data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        let raw_bitmap_first = (1u64 << 20) | 6u64;
        let raw_bitmap_second = (2u64 << 20) | 0u64;

        data[128 * BAT_ENTRY_SIZE..129 * BAT_ENTRY_SIZE]
            .copy_from_slice(&raw_bitmap_first.to_le_bytes());
        data[130 * BAT_ENTRY_SIZE..131 * BAT_ENTRY_SIZE]
            .copy_from_slice(&raw_bitmap_second.to_le_bytes());

        let bat = Bat::new(
            data,
            entry_count as u64,
            TEST_LOGICAL_SECTOR_SIZE,
            TEST_BLOCK_SIZE,
        )
        .expect("BAT creation should succeed");
        let entries = bat.entries();

        assert!(matches!(
            entries[128].state,
            BatState::SectorBitmap(SectorBitmapState::Present)
        ));
        assert!(matches!(
            entries[130].state,
            BatState::SectorBitmap(SectorBitmapState::NotPresent)
        ));
    }

    #[test]
    fn test_payload_block_states() {
        assert_eq!(PayloadBlockState::NotPresent.to_bits(), 0);
        assert_eq!(PayloadBlockState::FullyPresent.to_bits(), 6);
        assert_eq!(PayloadBlockState::PartiallyPresent.to_bits(), 7);

        assert!(PayloadBlockState::FullyPresent.is_allocated());
        assert!(PayloadBlockState::PartiallyPresent.is_allocated());
        assert!(!PayloadBlockState::NotPresent.is_allocated());
        assert!(!PayloadBlockState::Zero.is_allocated());
    }

    #[test]
    fn test_chunk_ratio_calculation() {
        let ratio = Bat::calculate_chunk_ratio(512, 32 * 1024 * 1024);
        assert_eq!(ratio, 128);
    }

    #[test]
    fn test_calculate_payload_blocks() {
        let blocks = Bat::calculate_payload_blocks(10 * 1024 * MiB, 32 * 1024 * 1024);
        assert_eq!(blocks, 320);
    }

    /// 测试 entries() 返回 Vec<BatEntry>：验证长度、顺序和状态一致性
    #[test]
    fn test_entries_returns_vec_with_correct_content() {
        // 构造 4 个 BAT 条目的原始数据（默认 chunk ratio=128 下，索引 0..2 均为 Payload）
        let mut data = vec![0u8; 4 * BAT_ENTRY_SIZE];
        // 条目 0：FullyPresent，偏移 1 MB
        let raw0 = (1u64 << 20) | 6u64;
        data[0..8].copy_from_slice(&raw0.to_le_bytes());
        // 条目 1：NotPresent（全零）
        // 条目 2：Zero 状态，偏移 2 MB
        let raw2 = (2u64 << 20) | 2u64;
        data[16..24].copy_from_slice(&raw2.to_le_bytes());

        let bat = Bat::new(data, 4, TEST_LOGICAL_SECTOR_SIZE, TEST_BLOCK_SIZE)
            .expect("BAT creation should succeed");
        let entries = bat.entries();

        // 返回类型为 Vec<BatEntry>，长度等于条目数
        assert_eq!(entries.len(), 4, "entries() should return 4 entries");

        // 验证条目 0 状态
        assert!(matches!(
            entries[0].state,
            BatState::Payload(PayloadBlockState::FullyPresent)
        ));
        assert_eq!(entries[0].file_offset_mb, 1);

        // 验证条目 1 状态为 NotPresent
        assert!(matches!(
            entries[1].state,
            BatState::Payload(PayloadBlockState::NotPresent)
        ));

        // 验证条目 2 状态为 Zero
        assert!(matches!(
            entries[2].state,
            BatState::Payload(PayloadBlockState::Zero)
        ));
        assert_eq!(entries[2].file_offset_mb, 2);
    }

    /// 测试空 BAT 的 entries() 不 panic 且返回空 Vec
    #[test]
    fn test_entries_empty_bat() {
        let data = vec![0u8; 0];
        let bat = Bat::new(data, 0, TEST_LOGICAL_SECTOR_SIZE, TEST_BLOCK_SIZE)
            .expect("Empty BAT creation should succeed");
        let entries = bat.entries();
        assert!(
            entries.is_empty(),
            "Empty BAT entries() should be empty Vec"
        );
    }

    /// 测试 entries() 返回的是副本：修改返回值不影响原 BAT
    #[test]
    fn test_entries_returns_owned_copy() {
        let mut data = vec![0u8; BAT_ENTRY_SIZE];
        let raw = (1u64 << 20) | 6u64;
        data[0..8].copy_from_slice(&raw.to_le_bytes());

        let bat = Bat::new(data, 1, TEST_LOGICAL_SECTOR_SIZE, TEST_BLOCK_SIZE)
            .expect("BAT creation should succeed");
        let mut entries = bat.entries();

        // 修改返回的 Vec 不应影响原数据
        entries.clear();
        assert_eq!(
            bat.entries().len(),
            1,
            "Original BAT should not be affected by mutation of returned Vec"
        );
    }

    // ── Task 11: BAT 非默认参数（4096 逻辑扇区 + 可变块大小）回归测试 ──

    /// 测试 4096 逻辑扇区大小下 chunk_ratio 计算正确，且与 512 扇区的结果不同
    ///
    /// chunk_ratio = (2^23 × logical_sector_size) / block_size
    /// - 512 + 32MB → 128（旧硬编码默认值）
    /// - 4096 + 32MB → 1024（4096 扇区的正确值）
    /// - 4096 + 1MB → 32768（小块大小极端值）
    ///
    /// 如果代码退化为硬编码 512，此测试将失败。
    #[test]
    fn test_chunk_ratio_calculation_4096_sector() {
        let ratio_4096_32m = Bat::calculate_chunk_ratio(4096, 32 * 1024 * 1024);
        assert_eq!(
            ratio_4096_32m, 1024,
            "4096 sector + 32MB block → chunk_ratio=1024"
        );

        let ratio_4096_1m = Bat::calculate_chunk_ratio(4096, 1024 * 1024);
        assert_eq!(
            ratio_4096_1m, 32768,
            "4096 sector + 1MB block → chunk_ratio=32768"
        );

        // 负向断言：4096 扇区的 chunk_ratio 必须与 512 扇区不同
        let ratio_512_32m = Bat::calculate_chunk_ratio(512, 32 * 1024 * 1024);
        assert_eq!(
            ratio_512_32m, 128,
            "512 sector + 32MB block → chunk_ratio=128"
        );
        assert_ne!(
            ratio_4096_32m, ratio_512_32m,
            "4096 sector chunk_ratio must differ from 512 sector"
        );
    }

    /// 测试 4096 扇区下 BAT 条目交错排列正确
    ///
    /// chunk_ratio=1024 时，每 1024 个 payload 条目后接 1 个 sector bitmap 条目。
    /// 构造 1025 个条目（1024 payload + 1 bitmap），验证索引 1024 为 sector bitmap。
    #[test]
    fn test_bat_sector_bitmap_interleaving_4096_sector() {
        let entry_count = 1025usize;
        let mut data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        // 在索引 1024 处写入 sector bitmap Present 状态
        let raw_bitmap = (3u64 << 20) | 6u64;
        let offset = 1024 * BAT_ENTRY_SIZE;
        data[offset..offset + BAT_ENTRY_SIZE].copy_from_slice(&raw_bitmap.to_le_bytes());

        let bat = Bat::new(data, entry_count as u64, 4096, 32 * 1024 * 1024)
            .expect("BAT creation with 4096 sector should succeed");

        let entry = bat.entry(1024).expect("entry 1024 should exist");
        assert!(
            matches!(
                entry.state,
                BatState::SectorBitmap(SectorBitmapState::Present)
            ),
            "index 1024 should be SectorBitmap(Present) under 4096 sector"
        );

        // 索引 0-1023 应为 Payload 类型
        let entry_0 = bat.entry(0).expect("entry 0 should exist");
        assert!(
            matches!(entry_0.state, BatState::Payload(_)),
            "index 0 should be Payload"
        );

        let entry_1023 = bat.entry(1023).expect("entry 1023 should exist");
        assert!(
            matches!(entry_1023.state, BatState::Payload(_)),
            "index 1023 should be Payload"
        );
    }

    /// 测试 4096 扇区下 BAT 拒绝 sector bitmap 条目的非法状态值
    ///
    /// chunk_ratio=1024 时索引 1024 为 sector bitmap，状态 7（PartiallyPresent）
    /// 不是合法的 sector bitmap 状态，应返回 InvalidBlockState(7)。
    #[test]
    fn test_bat_4096_sector_rejects_invalid_bitmap_state() {
        let entry_count = 1025usize;
        let mut data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        // 索引 1024（chunk_ratio=1024）为 sector bitmap，状态 7 非法
        let raw_invalid = (1u64 << 20) | 7u64;
        let offset = 1024 * BAT_ENTRY_SIZE;
        data[offset..offset + BAT_ENTRY_SIZE].copy_from_slice(&raw_invalid.to_le_bytes());

        assert!(
            matches!(
                Bat::new(data, entry_count as u64, 4096, 32 * 1024 * 1024),
                Err(Error::InvalidBlockState(7))
            ),
            "state 7 at sector bitmap position should be rejected"
        );
    }

    /// 测试 4096 扇区与 512 扇区在相同索引处的 bitmap 判定不同
    ///
    /// 关键负向断言：索引 128 在 512 扇区下为 SectorBitmap（chunk_ratio=128），
    /// 但在 4096 扇区下为 Payload（chunk_ratio=1024）。
    /// 如果代码退化为硬编码 chunk_ratio=128，索引 128 会被错误判定为 bitmap。
    ///
    /// entry_count=130 时：
    /// - 4096 扇区: chunk_ratio=1024, bitmap_blocks=ceil(130/1025)=1,
    ///   payload_blocks=129 → 索引 0-128 为 payload，索引 129 为 bitmap
    /// - 512 扇区: chunk_ratio=128, bitmap_blocks=ceil(130/129)=2,
    ///   payload_blocks=128 → 索引 128 为第一个 bitmap
    #[test]
    fn test_bat_4096_sector_bitmap_position_differs_from_512() {
        let entry_count = 130usize;
        let data = vec![0u8; entry_count * BAT_ENTRY_SIZE];

        // 4096 扇区 + 32MB → chunk_ratio=1024 → 129 个条目全部为 payload
        let bat_4096 = Bat::new(data.clone(), entry_count as u64, 4096, 32 * 1024 * 1024)
            .expect("4096 BAT creation should succeed");
        let entry_128_4096 = bat_4096.entry(128).expect("entry 128 should exist");
        assert!(
            matches!(entry_128_4096.state, BatState::Payload(_)),
            "index 128 should be Payload under 4096 sector (chunk_ratio=1024)"
        );

        // 512 扇区 + 32MB → chunk_ratio=128 → 索引 128 为 sector bitmap
        let bat_512 = Bat::new(data, entry_count as u64, 512, 32 * 1024 * 1024)
            .expect("512 BAT creation should succeed");
        let entry_128_512 = bat_512.entry(128).expect("entry 128 should exist");
        assert!(
            matches!(entry_128_512.state, BatState::SectorBitmap(_)),
            "index 128 should be SectorBitmap under 512 sector (chunk_ratio=128)"
        );
    }

    /// 测试 calculate_total_entries 在 4096 扇区与 512 扇区下结果不同
    ///
    /// 130 个 payload blocks + 32MB block_size:
    /// - 4096 扇区: chunk_ratio=1024, 需 1 bitmap → 总条目 131
    /// - 512 扇区: chunk_ratio=128, 需 2 bitmap → 总条目 132
    #[test]
    fn test_calculate_total_entries_4096_differs_from_512() {
        let block_size = 32 * 1024 * 1024u32;
        // 130 blocks × 32MB = 4160 MiB
        let virtual_size = 130u64 * u64::from(block_size);

        let total_4096 = Bat::calculate_total_entries(virtual_size, block_size, 4096);
        let total_512 = Bat::calculate_total_entries(virtual_size, block_size, 512);

        assert_eq!(total_4096, 131, "4096 sector: 130 payload + 1 bitmap = 131");
        assert_eq!(total_512, 132, "512 sector: 130 payload + 2 bitmap = 132");
        assert_ne!(
            total_4096, total_512,
            "total entries must differ between 4096 and 512 sectors"
        );
    }
}
