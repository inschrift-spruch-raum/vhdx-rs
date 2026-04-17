//! VHDX 虚拟磁盘扇区级和块级 IO 抽象
//!
//! 本模块提供虚拟磁盘的扇区（[`Sector`]）和块（[`PayloadBlock`]）级别的 IO 操作。
//!
//! 扇区和块的地址计算基于 VHDX 文件参数：
//! - **逻辑扇区大小** — 通常为 512 字节（MS-VHDX §2.6.2.4）
//! - **块大小** — 1MB 到 256MB 之间的 2 的幂次（MS-VHDX §2.6.2.1）
//!
//! 每个 Payload Block 包含 `block_size / sector_size` 个逻辑扇区。

use crate::File;
use crate::PayloadBlockState;
use crate::error::{Error, Result};

/// 扇区/块级 IO 操作入口
///
/// 提供对 VHDX 文件的扇区级和批量读写操作。
/// 通过 [`File`](crate::File) 的 [`io()`](crate::File::io) 方法获取实例。
pub struct IO<'a> {
    /// 关联的 VHDX 文件引用
    file: &'a File,
}

impl<'a> IO<'a> {
    /// 从 VHDX 文件引用创建 IO 实例
    pub const fn new(file: &'a File) -> Self {
        Self { file }
    }

    /// 获取指定逻辑扇区号的扇区对象，超出范围返回 None
    #[must_use]
    pub fn sector(&self, sector: u64) -> Option<Sector<'a>> {
        let sector_size = u64::from(self.file.logical_sector_size());
        let block_size = u64::from(self.file.block_size());

        let sectors_per_block = block_size / sector_size; // 每个块包含的扇区数
        let block_idx = sector / sectors_per_block; // 扇区号 → 块索引
        let block_sector_idx = u32::try_from(sector % sectors_per_block) // 块内扇区偏移
            .expect("sector index within block should fit in u32");

        let total_sectors = self.file.virtual_disk_size() / sector_size;
        if sector >= total_sectors {
            return None;
        }

        Some(Sector {
            file: self.file,
            block_idx,
            block_sector_idx,
            size: self.file.logical_sector_size(),
        })
    }

    /// 批量读取连续扇区数据到缓冲区
    pub fn read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<usize> {
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
    pub fn write_sectors(&self, _start_sector: u64, data: &[u8]) -> Result<usize> {
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
/// 扇区大小由 VHDX 文件的逻辑扇区大小决定（通常 512 字节）。
/// 每个扇区属于一个 Payload Block，扇区在该块内有一个偏移索引。
pub struct Sector<'a> {
    /// 关联的 VHDX 文件引用
    file: &'a File,
    /// 所属的 Payload Block 索引
    block_idx: u64,
    /// 在所属 Payload Block 内的扇区索引
    block_sector_idx: u32,
    /// 扇区大小（字节）
    size: u32,
}

impl Sector<'_> {
    /// 获取所属 Payload Block 索引
    #[must_use]
    pub const fn block_idx(&self) -> u64 {
        self.block_idx
    }

    /// 获取在所属 Payload Block 内的扇区索引
    #[must_use]
    pub const fn block_sector_idx(&self) -> u32 {
        self.block_sector_idx
    }

    /// 计算全局扇区号 = block_idx × sectors_per_block + block_sector_idx
    #[must_use]
    pub fn global_sector(&self) -> u64 {
        let sectors_per_block = u64::from(self.file.block_size() / self.size);
        self.block_idx * sectors_per_block + u64::from(self.block_sector_idx)
    }

    /// 读取扇区数据到缓冲区，缓冲区大小必须匹配扇区大小
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() != self.size as usize {
            return Err(Error::InvalidParameter(format!(
                "Buffer size {} does not match sector size {}",
                buf.len(),
                self.size
            )));
        }

        let sector_offset = self.global_sector() * u64::from(self.size);
        self.file.read(sector_offset, buf)
    }

    /// 获取此扇区所属的 Payload Block
    #[must_use]
    pub const fn payload(&self) -> PayloadBlock<'_> {
        PayloadBlock {
            file: self.file,
            block_idx: self.block_idx,
        }
    }
}

/// 虚拟磁盘中的一个 Payload Block（数据块）
///
/// 每个 Payload Block 的大小由 VHDX 文件的块大小决定。
/// Block 的状态由 BAT 条目决定（MS-VHDX §2.5.1.1）。
pub struct PayloadBlock<'a> {
    /// 关联的 VHDX 文件引用
    file: &'a File,
    /// Block 索引
    block_idx: u64,
}

impl PayloadBlock<'_> {
    /// 获取 Block 索引
    #[must_use]
    pub const fn block_idx(&self) -> u64 {
        self.block_idx
    }

    /// 从 Block 的指定偏移量读取数据
    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let block_size = u64::from(self.file.block_size());
        if offset >= block_size {
            return Ok(0);
        }

        let block_offset = self.block_idx * block_size + offset;
        self.file.read(block_offset, buf)
    }

    /// 获取此 Block 的 BAT 条目，用于判断分配状态
    #[must_use]
    pub fn bat_entry(&self) -> Option<crate::BatEntry> {
        if let Ok(bat) = self.file.sections().bat() {
            usize::try_from(self.block_idx)
                .ok()
                .and_then(|idx| bat.entry(idx))
        } else {
            None
        }
    }

    /// 检查此 Block 是否已分配（FullyPresent 状态）
    #[must_use]
    pub fn is_allocated(&self) -> bool {
        if let Some(entry) = self.bat_entry() {
            matches!(
                entry.state,
                crate::BatState::Payload(PayloadBlockState::FullyPresent)
            )
        } else {
            false
        }
    }
}
