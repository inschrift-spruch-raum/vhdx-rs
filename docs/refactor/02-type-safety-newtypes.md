# 重构: 使用 Newtype 包装器增强类型安全

## 概述

将偏移量和大小使用的原始 `u64`/`u32` 类型替换为语义化的 newtype 包装器。这可以在编译时确保虚拟偏移量、文件偏移量和块大小不会被混淆，从而防止潜在的隐蔽错误。

## 当前存在的问题

### 类型混淆问题

**问题 1: 虚拟偏移量与文件偏移量混淆** (都是 `u64`)
```rust
// src/block_io/traits.rs:21,33
fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;
fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize>;

// src/bat/table.rs:166
pub fn translate(&self, virtual_offset: u64) -> Result<Option<u64>>;
// 返回 file_offset 作为 u64 - 很容易与 virtual_offset 混淆!
```

**问题 2: BlockSize 类型不一致** (`u32` 与 `u64`)
```rust
// src/file/vhdx_file.rs:43
pub(crate) block_size: u32,

// src/block_io/fixed.rs:99
pub fn block_size(&self) -> u64 {
    self.bat.block_size  // 返回 u64
}

// src/bat/table.rs:17
pub block_size: u64,

// src/file/builder.rs:22
pub(crate) block_size: u32,
```

**问题 3: 缺少对齐验证**
```rust
// src/bat/table.rs:166-169
pub fn translate(&self, virtual_offset: u64) -> Result<Option<u64>> {
    if virtual_offset >= self.virtual_disk_size {
        return Err(VhdxError::InvalidOffset(virtual_offset));
    }
    // 没有检查偏移量是否按扇区对齐!
```

**问题 4: MB 与字节混用**
```rust
// src/bat/entry.rs - file_offset_mb 以 MB 为单位存储
// src/bat/table.rs:180 - 转换回字节
let file_offset = entry.file_offset_mb * 1024 * 1024;
```

## 建议的解决方案

### Newtype 定义

创建 `src/types.rs`:

```rust
//! VHDX 偏移量和大小的类型安全包装器
//!
//! 这些 newtype 在编译时提供保证，确保不同类型的
//! 偏移量不会被混淆，并在运行时验证对齐等不变式。

use std::ops::{Add, Sub, Mul, Div};
use std::fmt;
use crate::error::{Result, VhdxError};
use crate::constants::sector;

/// 磁盘镜像内的虚拟偏移量 (客户机视角)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualOffset(u64);

impl VirtualOffset {
    /// 创建新的虚拟偏移量，验证扇区对齐
    pub fn new(offset: u64) -> Result<Self> {
        if offset % sector::LOGICAL_DEFAULT as u64 != 0 {
            return Err(VhdxError::Alignment(offset, sector::LOGICAL_DEFAULT as u64));
        }
        Ok(Self(offset))
    }
    
    /// 不验证直接创建 (谨慎使用)
    pub fn new_unchecked(offset: u64) -> Self {
        Self(offset)
    }
    
    /// 获取底层值
    pub fn value(&self) -> u64 {
        self.0
    }
    
    /// 检查偏移量是否按给定边界对齐
    pub fn is_aligned(&self, alignment: u64) -> bool {
        self.0 % alignment == 0
    }
    
    /// 根据块大小计算块索引
    pub fn block_index(&self, block_size: BlockSize) -> u64 {
        self.0 / block_size.bytes()
    }
    
    /// 计算块内的偏移量
    pub fn offset_in_block(&self, block_size: BlockSize) -> u64 {
        self.0 % block_size.bytes()
    }
}

impl Add for VirtualOffset {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for VirtualOffset {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

/// 实际 VHDX 文件内的文件偏移量
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileOffset(u64);

impl FileOffset {
    pub fn new(offset: u64) -> Result<Self> {
        // 文件偏移量必须按 1MB 对齐
        if offset % (1024 * 1024) != 0 {
            return Err(VhdxError::Alignment(offset, 1024 * 1024));
        }
        Ok(Self(offset))
    }
    
    pub fn new_unchecked(offset: u64) -> Self {
        Self(offset)
    }
    
    pub fn value(&self) -> u64 {
        self.0
    }
    
    /// 转换为 MB 用于 BAT 存储
    pub fn to_mb(&self) -> u64 {
        self.0 / (1024 * 1024)
    }
    
    /// 从 MB 值创建 (来自 BAT)
    pub fn from_mb(mb: u64) -> Self {
        Self(mb * 1024 * 1024)
    }
}

impl Add for FileOffset {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

/// 块大小，单位为字节 (1MB 到 256MB)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockSize(u32);

impl BlockSize {
    pub fn new(size: u32) -> Result<Self> {
        use crate::constants::block;
        if size < block::MIN_SIZE || size > block::MAX_SIZE {
            return Err(VhdxError::InvalidMetadata(
                format!("块大小 {} 超出范围", size)
            ));
        }
        if size % block::ALIGNMENT != 0 {
            return Err(VhdxError::InvalidMetadata(
                "块大小必须按 1MB 对齐".to_string()
            ));
        }
        Ok(Self(size))
    }
    
    pub fn bytes(&self) -> u64 {
        self.0 as u64
    }
    
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl From<BlockSize> for u64 {
    fn from(bs: BlockSize) -> Self {
        bs.0 as u64
    }
}

impl From<BlockSize> for u32 {
    fn from(bs: BlockSize) -> Self {
        bs.0
    }
}

/// 扇区大小 (512 或 4096 字节)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorSize(u32);

impl SectorSize {
    pub fn new(size: u32) -> Result<Self> {
        if size != 512 && size != 4096 {
            return Err(VhdxError::InvalidMetadata(
                "扇区大小必须是 512 或 4096".to_string()
            ));
        }
        Ok(Self(size))
    }
    
    pub fn logical_default() -> Self {
        Self(512)
    }
    
    pub fn physical_default() -> Self {
        Self(4096)
    }
    
    pub fn value(&self) -> u32 {
        self.0
    }
}

/// 磁盘大小，单位为字节
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskSize(u64);

impl DiskSize {
    pub fn new(size: u64) -> Result<Self> {
        if size == 0 {
            return Err(VhdxError::InvalidMetadata(
                "磁盘大小不能为零".to_string()
            ));
        }
        Ok(Self(size))
    }
    
    pub fn value(&self) -> u64 {
        self.0
    }
}
```

## 迁移策略

### 阶段 1: 创建 types.rs
1. 创建 `src/types.rs`，包含所有 newtype
2. 添加全面的单元测试
3. 添加到 `src/lib.rs`

### 阶段 2: 更新公共 API
按优先级顺序更新函数签名:
1. `src/block_io/traits.rs` - BlockIo trait
2. `src/bat/table.rs` - translate() 和 index 方法
3. `src/file/vhdx_file.rs` - read/write 方法
4. `src/file/builder.rs` - Builder 方法

### 阶段 3: 内部更新
更新内部实现:
1. `src/block_io/dynamic.rs`
2. `src/block_io/differencing.rs`
3. `src/block_io/fixed.rs`
4. 所有 BAT 操作

## 前后对比示例

### 示例 1: BlockIo::read() 签名

**之前** (`src/block_io/traits.rs:21`):
```rust
fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;
```

**之后**:
```rust
use crate::types::VirtualOffset;

fn read(&mut self, offset: VirtualOffset, buf: &mut [u8]) -> Result<usize>;
```

### 示例 2: VhdxFile::read() 实现

**之前** (`src/file/vhdx_file.rs:312-314`):
```rust
pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
    if virtual_offset >= self.virtual_disk_size {
        return Err(VhdxError::InvalidOffset(virtual_offset));
    }
```

**之后**:
```rust
use crate::types::VirtualOffset;

pub fn read(&mut self, offset: VirtualOffset, buf: &mut [u8]) -> Result<usize> {
    // 验证在 VirtualOffset::new 中完成，这里不需要
    if offset.value() >= self.virtual_disk_size {
        return Err(VhdxError::InvalidOffset(offset.value()));
```
