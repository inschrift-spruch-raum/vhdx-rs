# 重构：BlockIo 代码去重

## 概述

将 DynamicBlockIo、DifferencingBlockIo 和 FixedBlockIo 之间 80% 的共享逻辑整合到一个通用的 BaseBlockIo 结构中。这可以消除代码重复，降低维护负担，并使代码库更加一致。

## 当前状态

### 代码重复分析

| 组件 | 行数 | 共享逻辑 | 独有逻辑 |
|------|------|----------|----------|
| DynamicBlockIo | 280 | read()、write()、allocate_block() | 无 |
| DifferencingBlockIo | 1014 | read()、write()、allocate_block() | 父盘处理、扇区位图 |
| FixedBlockIo | 111 | read()、write() | 预分配块 |

**重复代码部分**：
- `new()` 构造函数：95% 相同
- `with_log_writer()` 方法：100% 相同（Dynamic/Differencing）
- `read()` 循环结构：80% 相同
- `write()` 块处理：75% 相同
- `allocate_block()` 方法：100% 相同（Dynamic/Differencing）
- 字段定义：80% 相同

### 具体重复点

#### 1. 完全相同的结构体字段
```rust
// DynamicBlockIo (src/block_io/dynamic.rs:16-27)
pub struct DynamicBlockIo<'a> {
    pub file: &'a mut std::fs::File,
    pub bat: &'a mut Bat,
    pub next_free_offset: u64,
    pub virtual_disk_size: u64,
    log_writer: Option<LogWriter>,
}

// DifferencingBlockIo (src/block_io/differencing.rs:17-30)
pub struct DifferencingBlockIo<'a> {
    pub file: &'a mut std::fs::File,
    pub bat: &'a mut Bat,
    pub parent: Option<Box<DifferencingBlockIo<'a>>>,
    pub next_free_offset: u64,
    pub virtual_disk_size: u64,
    log_writer: Option<LogWriter>,
}
```

#### 2. 完全相同的 allocate_block() 方法
DynamicBlockIo 和 DifferencingBlockIo 都有完全相同的 55 行 allocate_block() 方法，仅格式上有差异。

#### 3. 几乎相同的读写循环
核心的读写循环结构是重复的，仅在父盘处理和扇区位图方面有细微变化。

## 建议方案

### 架构

```
BaseBlockIo
    |
    +-- DynamicBlockIo
    +-- DifferencingBlockIo（添加父盘处理）
    +-- FixedBlockIo（只读 BAT）
```

### 实现

#### 步骤 1：创建 BaseBlockIo

```rust
// src/block_io/base.rs

use crate::bat::{Bat, BatEntry, PayloadBlockState};
use crate::error::{Result, VhdxError};
use crate::log::LogWriter;
use std::io::{Read, Seek, SeekFrom, Write};

/// 带共享逻辑的块 I/O 基础实现
pub struct BaseBlockIo<'a> {
    file: &'a mut std::fs::File,
    bat: &'a mut Bat,
    virtual_disk_size: u64,
    next_free_offset: u64,
    log_writer: Option<LogWriter>,
}

impl<'a> BaseBlockIo<'a> {
    pub fn new(
        file: &'a mut std::fs::File,
        bat: &'a mut Bat,
        virtual_disk_size: u64,
    ) -> Self {
        Self {
            file,
            bat,
            virtual_disk_size,
            next_free_offset: crate::constants::layout::METADATA_REGION_OFFSET,
            log_writer: None,
        }
    }
    
    pub fn with_log_writer(mut self, log_writer: LogWriter) -> Self {
        self.log_writer = Some(log_writer);
        self
    }
    
    /// 核心读取操作 - 从完全存在的块中读取
    pub fn read_from_block(
        &mut self,
        entry: &BatEntry,
        offset_in_block: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        let file_offset = entry
            .file_offset()
            .ok_or(VhdxError::InvalidBatEntry)?;
        
        let absolute_offset = file_offset + offset_in_block;
        self.file.seek(SeekFrom::Start(absolute_offset))?;
        self.file.read_exact(buf)?;
        
        Ok(())
    }
    
    /// 核心写入操作 - 写入到完全存在的块
    pub fn write_to_block(
        &mut self,
        entry: &BatEntry,
        offset_in_block: u64,
        buf: &[u8],
    ) -> Result<()> {
        let file_offset = entry
            .file_offset()
            .ok_or(VhdxError::InvalidBatEntry)?;
        
        let absolute_offset = file_offset + offset_in_block;
        self.file.seek(SeekFrom::Start(absolute_offset))?;
        self.file.write_all(buf)?;
        
        Ok(())
    }
    
    /// 为动态/差异磁盘分配新块
    pub fn allocate_block(&mut self, block_idx: u64) -> Result<u64> {
        use crate::constants::{bat, MB};
        
        // 将下一个空闲偏移量对齐到 1MB
        let aligned_offset = (self.next_free_offset + (MB - 1)) & !(MB - 1);
        let block_size = self.bat.block_size;
        let file_offset_mb = aligned_offset / MB;
        
        // 如有必要，扩展文件
        self.file
            .seek(SeekFrom::Start(aligned_offset + block_size - 1))?;
        self.file.write_all(&[0])?;
        
        // 计算 BAT 条目位置
        let bat_index = self
            .bat
            .payload_bat_index(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        let bat_entry_offset = self.bat.get_bat_entry_file_offset(bat_index);
        
        // 创建新的 BAT 条目
        let new_entry = BatEntry::new(PayloadBlockState::FullyPresent, file_offset_mb);
        
        // 写入 BAT 条目（带或不带日志）
        self.write_bat_entry(bat_entry_offset, &new_entry, block_idx)?;
        
        // 更新内存中的 BAT
        self.bat.update_payload_entry(block_idx, new_entry)?;
        
        self.next_free_offset = aligned_offset + block_size;
        
        Ok(aligned_offset)
    }
    
    /// 写入 BAT 条目，可选日志原子更新
    fn write_bat_entry(
        &mut self,
        bat_entry_offset: u64,
        entry: &BatEntry,
        _block_idx: u64,
    ) -> Result<()> {
        use crate::constants::log;
        
        let entry_bytes = entry.to_bytes();
        
        if let Some(ref mut log_writer) = self.log_writer {
            // 通过日志进行原子更新
            let mut sector_data = vec![0u8; log::DATA_SECTOR_SIZE];
            sector_data[0..8].copy_from_slice(&entry_bytes);
            
            log_writer.write_data_entry(&mut self.file, bat_entry_offset, &sector_data)?;
            self.file.flush()?;
            
            // 应用到 BAT
            self.file.seek(SeekFrom::Start(bat_entry_offset))?;
            self.file.write_all(&entry_bytes)?;
            self.file.flush()?;
        } else {
            // 直接写入
            self.file.seek(SeekFrom::Start(bat_entry_offset))?;
            self.file.write_all(&entry_bytes)?;
            self.file.flush()?;
        }
        
        Ok(())
    }
    
    /// 获取虚拟磁盘大小
    pub fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }
    
    /// 获取块大小
    pub fn block_size(&self) -> u64 {
        self.bat.block_size
    }
    
    /// 访问 BAT 用于读取操作
    pub fn bat(&self) -> &Bat {
        self.bat
    }
    
    /// 访问 BAT 用于写入操作
    pub fn bat_mut(&mut self) -> &mut Bat {
        self.bat
    }
}
```

#### 步骤 2：重构 DynamicBlockIo

```rust
// src/block_io/dynamic.rs

use crate::bat::{PayloadBlockState};
use crate::error::{Result, VhdxError};
use super::base::BaseBlockIo;

pub struct DynamicBlockIo<'a> {
    base: BaseBlockIo<'a>,
}

impl<'a> DynamicBlockIo<'a> {
    pub fn new(file: &'a mut std::fs::File, bat: &'a mut Bat, virtual_disk_size: u64) -> Self {
        Self {
            base: BaseBlockIo::new(file, bat, virtual_disk_size),
        }
    }
    
    pub fn with_log_writer(mut self, log_writer: LogWriter) -> Self {
        self.base = self.base.with_log_writer(log_writer);
        self
    }
    
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        if virtual_offset >= self.base.virtual_disk_size() {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        let byt
