//! VHDX 虚拟磁盘扇区级和块级 IO 抽象
//!
//! 本模块提供虚拟磁盘的扇区（[`Sector`]）和块（[`PayloadBlock`]）级别的 IO 操作。
//!
//! 扇区和块的地址计算基于 VHDX 文件参数：
//! - **逻辑扇区大小** — 通常为 4096 字节（MS-VHDX §2.6.2.4）
//! - **块大小** — 1MB 到 256MB 之间的 2 的幂次（MS-VHDX §2.6.2.1）
//!
//! 每个 Payload Block 包含 `block_size / sector_size` 个逻辑扇区。
//!
//! # 设计约束
//!
//! 唯一数据平面入口：
//! - [`File`](crate::File) 层不提供 read/write/flush（公共数据面）
//! - 所有虚拟磁盘读写必须经由 [`IO::sector`] → [`Sector::read`]/[`Sector::write`]
//! - 输入: 全局扇区号 → 内部自动计算块索引和块内扇区偏移

use std::fmt;

use crate::File;
use crate::error::{Error, Result};

/// 扇区/块级 IO 操作入口
///
/// 提供对 VHDX 文件的扇区级和批量读写操作。
/// 通过 [`File`](crate::File) 的 [`io()`](crate::File::io) 方法获取实例。
///
/// 输入: 全局扇区号 → 内部自动计算块索引和块内扇区偏移。
pub struct IO<'a> {
    /// 关联的 VHDX 文件引用
    file: &'a File,
}

impl<'a> IO<'a> {
    /// 从 VHDX 文件引用创建 IO 实例
    pub const fn new(file: &'a File) -> Self {
        Self { file }
    }

    /// 通过全局扇区号定位并返回 [`Sector`]
    ///
    /// 内部自动执行：
    /// 1. 通过 BAT 找到对应块
    /// 2. 计算块内扇区偏移
    ///
    /// 扇区缓存按需从文件读取（懒加载）。
    /// 超出虚拟磁盘范围时返回 `None`。
    #[must_use]
    pub fn sector(&self, sector: u64) -> Option<Sector<'a>> {
        let sector_size = u64::from(self.file.logical_sector_size());
        let block_size = u64::from(self.file.block_size());

        let sectors_per_block = block_size / sector_size; // 每个块包含的扇区数
        let block_idx = sector / sectors_per_block; // 扇区号 → 块索引
        let block_sector_index = u32::try_from(sector % sectors_per_block) // 块内扇区偏移
            .expect("sector index within block should fit in u32");

        // 使用向上取整除法（兼容模式）：即使虚拟磁盘大小不是扇区大小的整数倍，
        // 最后一个部分扇区仍可寻址，与 File::read 的字节级边界语义一致。
        let total_sectors = self.file.virtual_disk_size().div_ceil(sector_size);
        if sector >= total_sectors {
            return None;
        }

        Some(Sector {
            file: self.file,
            block_idx,
            block_sector_index,
            size: self.file.logical_sector_size(),
            payload: PayloadBlock { bytes: &[] },
        })
    }

    /// 批量读取连续扇区数据到缓冲区
    pub(crate) fn read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let num_sectors = buf.len() / sector_size;

        if !buf.len().is_multiple_of(sector_size) {
            return Err(Error::InvalidParameter(
                "Buffer size must be a multiple of sector size".to_string(),
            ));
        }

        let mut total_read = 0;
        for i in 0..num_sectors {
            let sector_num = start_sector + i as u64;
            if let Some(sector) = self.sector(sector_num) {
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                let bytes_read = sector.read(sector_buf)?;
                total_read += bytes_read;
            } else {
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                for item in sector_buf.iter_mut() {
                    *item = 0;
                }
                total_read += sector_size;
            }
        }

        Ok(total_read)
    }

    /// 批量写入连续扇区（当前未完全实现）
    pub(crate) fn write_sectors(&self, _start_sector: u64, data: &[u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let _num_sectors = data.len() / sector_size;

        if !data.len().is_multiple_of(sector_size) {
            return Err(Error::InvalidParameter(
                "Data size must be a multiple of sector size".to_string(),
            ));
        }

        Err(Error::InvalidParameter(
            "IO::write_sectors requires mutable access (not yet fully implemented)".to_string(),
        ))
    }
}

/// 虚拟磁盘中的一个逻辑扇区
///
/// 封装了 [`PayloadBlock`] 引用和块内扇区索引。
/// 扇区大小由 VHDX 文件的逻辑扇区大小决定（通常 4096 字节）。
/// 每个扇区属于一个 Payload Block，扇区在该块内有一个偏移索引。
pub struct Sector<'a> {
    /// 关联的 VHDX 文件引用（内部实现细节）
    file: &'a File,
    /// 所属的 Payload Block 索引（内部实现细节）
    block_idx: u64,
    /// 在所属 Payload Block 内的扇区索引
    pub block_sector_index: u32,
    /// 扇区大小（字节，内部实现细节）
    size: u32,
    /// 所属的 Payload Block 视图
    pub payload: PayloadBlock<'a>,
}

impl Clone for Sector<'_> {
    fn clone(&self) -> Self {
        Self {
            file: self.file,
            block_idx: self.block_idx,
            block_sector_index: self.block_sector_index,
            size: self.size,
            payload: self.payload.clone(),
        }
    }
}

impl fmt::Debug for Sector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sector")
            .field("block_sector_index", &self.block_sector_index)
            .field("payload", &self.payload)
            .finish()
    }
}

impl PartialEq for Sector<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.block_sector_index == other.block_sector_index && self.payload == other.payload
    }
}

impl Sector<'_> {
    /// 读取扇区数据到缓冲区，缓冲区大小必须匹配扇区大小
    ///
    /// 兼容模式行为：当扇区跨越虚拟磁盘末尾（尾部非整扇区）时，
    /// 自动将越界部分零填充，仅返回虚拟磁盘范围内的有效数据。
    /// 方法始终返回完整扇区大小（`self.size`），确保调用者无需处理部分读取。
    ///
    /// # 错误
    /// 返回 [`Error::InvalidParameter`] 当缓冲区长度不等于扇区大小时。
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() != self.size as usize {
            return Err(Error::InvalidParameter(format!(
                "Buffer size {} does not match sector size {}",
                buf.len(),
                self.size
            )));
        }

        let sectors_per_block = u64::from(self.file.block_size() / self.size);
        let global_sector = self.block_idx * sectors_per_block + u64::from(self.block_sector_index);
        let sector_offset = global_sector * u64::from(self.size);

        // 兼容模式：检查扇区是否跨越虚拟磁盘末尾
        let virtual_disk_size = self.file.virtual_disk_size();
        let sector_end = sector_offset + u64::from(self.size);

        if sector_offset >= virtual_disk_size {
            // 完全超出虚拟磁盘范围（防御性处理，正常情况下 IO::sector 已过滤）
            buf.fill(0);
        } else if sector_end > virtual_disk_size {
            // 尾部非整扇区：扇区部分在虚拟磁盘范围内
            // 先读取完整扇区数据，再将越界部分零填充
            let valid_len = (virtual_disk_size - sector_offset) as usize;
            self.file.read_raw(sector_offset, &mut buf[..valid_len])?;
            buf[valid_len..].fill(0);
        } else {
            // 完全在虚拟磁盘范围内，正常读取
            self.file.read_raw(sector_offset, buf)?;
        }

        Ok(self.size as usize)
    }

    /// 将数据写入扇区，数据长度必须匹配扇区大小
    ///
    /// 写入逻辑与 [`read()`](Sector::read) 对称：
    /// 计算虚拟偏移量后通过内部写入方法完成实际 I/O。
    /// Fixed 类型直接写入文件，Dynamic 类型通过 BAT 查找块位置。
    ///
    /// 兼容模式行为：当扇区跨越虚拟磁盘末尾（尾部非整扇区）时，
    /// `write_raw` 自动截断到虚拟磁盘边界，仅写入有效范围内的数据。
    ///
    /// # 错误
    /// 返回 [`Error::InvalidParameter`] 当数据长度不等于扇区大小时。
    pub fn write(&self, data: &[u8]) -> Result<()> {
        if data.len() != self.size as usize {
            return Err(Error::InvalidParameter(format!(
                "Data size {} does not match sector size {}",
                data.len(),
                self.size
            )));
        }

        let sectors_per_block = u64::from(self.file.block_size() / self.size);
        let global_sector = self.block_idx * sectors_per_block + u64::from(self.block_sector_index);
        let sector_offset = global_sector * u64::from(self.size);
        self.file.write_raw(sector_offset, data)?;
        Ok(())
    }

    /// 获取此扇区所属的 Payload Block
    #[must_use]
    pub fn payload(&self) -> PayloadBlock<'_> {
        self.payload.clone()
    }
}

/// 虚拟磁盘中的一个 Payload Block 视图
///
/// 用户通过 [`Sector`] 访问，不直接操作。
/// `bytes` 字段为块数据的字节切片视图；
/// 对于懒加载场景（数据按需通过 [`Sector::read`]/[`Sector::write`] 访问），
/// `bytes` 可能为空切片。
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadBlock<'a> {
    /// 块数据的字节切片视图
    pub bytes: &'a [u8],
}
