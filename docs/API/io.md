# IO Module

← [Back to API Documentation](../API.md)

## Overview

Sector-level read/write operations.

## API Tree

```
├── IO                                      # IO模块 (扇区级操作)
│   └── sector(&self, sector: u64) -> Option<Sector>  # 输入: 全局扇区号
│   │
│   └── Sector                              # 扇区级定位与操作
│       ├── payload(&self) -> &PayloadBlock
│       ├── read(&self, buf: &mut [u8]) -> Result<usize>
│       └── write(&self, data: &[u8]) -> Result<()>
```

## Detailed Design

### IO

扇区级读写操作模块。
输入: 全局扇区号 -> 内部自动计算块索引和块内扇区偏移

```rust
/// IO模块
/// 
/// 扇区级读写操作
/// 输入: 全局扇区号 -> 内部自动计算块索引和块内扇区偏移
pub struct IO;

impl IO {
    /// 通过全局扇区号定位并返回Sector
    /// 内部自动: 1) 通过BAT找到对应块 2) 计算块内扇区偏移
    /// 懒加载: Sector缓存按需从文件读取
    pub fn sector(&self, sector: u64) -> Option<Sector>;
}
```

### Sector

扇区级定位与操作结构体。
封装了PayloadBlock引用和块内扇区索引。

```rust
/// Sector - 扇区级定位与操作
/// 
/// 封装了PayloadBlock引用和块内扇区索引
#[derive(Clone, Debug, PartialEq)]
pub struct Sector {
    // 简单类型字段: 块内扇区索引
    pub block_sector_index: u32,
}

impl Sector {
    /// 获取对应的PayloadBlock
    pub fn payload(&self) -> &PayloadBlock;
    
    /// 读取扇区数据
    /// buf长度必须为扇区大小的整数倍
    pub fn read(&self, buf: &mut [u8]) -> Result<usize>;
    
    /// 写入扇区数据
    /// data长度必须为扇区大小的整数倍
    pub fn write(&self, data: &[u8]) -> Result<()>;
}
```

### PayloadBlock

Payload Block - 内部结构。
用户通过Sector访问，不直接操作。

```rust
/// Payload Block - 内部结构
/// 
/// 用户通过Sector访问，不直接操作
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadBlock;
```

## Design Notes

- **输入**: 全局扇区号 (sector: u64)
- **内部自动**: 
  1. 通过BAT找到对应块
  2. 计算块内扇区偏移
- **懒加载**: Sector缓存按需从文件读取
- **约束**: buf/data长度必须为扇区大小的整数倍
