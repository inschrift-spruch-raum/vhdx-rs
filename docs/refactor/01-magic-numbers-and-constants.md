# 魔法数字与常量重构文档

## 1. 问题描述

### 1.1 什么是魔法数字

魔法数字（Magic Numbers）是指代码中直接使用的字面数值，而未通过命名常量来表达其含义。这些数字散落在代码各处，缺乏统一的定义和管理。

### 1.2 当前代码库中的问题

在 vhdx-rs-old 代码库中，存在大量魔法数字：

- **可读性差**：`1024 * 1024` 出现在 20+ 处，但读者无法立即判断这是 1MB 的文件对齐单位还是其他含义
- **维护困难**：修改一个常量需要在多处手动同步，容易遗漏
- **语义模糊**：`1u64 << 23` 没有解释这是 MS-VHDX 规范定义的 Chunk 大小计算因子
- **类型安全缺失**：直接使用数字字面量无法进行编译时验证

### 1.3 具体影响示例

```rust
// 当前代码 (src/file/builder.rs:40)
block_size: 1024 * 1024 * 32, // 32MB default

// 问题：
// 1. 注释与代码重复
// 2. 无法在其他地方复用这个值
// 3. 如果要改为 64MB，需要全文搜索替换
```

## 2. 现有魔法数字清单

### 2.1 布局常量 (Layout Constants)

| 位置 | 代码 | 含义 | 规范来源 |
|------|------|------|----------|
| `src/file/builder.rs:135` | `let header_size = 1024 * 1024;` | 1MB Header 区域大小 | MS-VHDX 2.2.3 |
| `src/file/builder.rs:136` | `let metadata_size = 1024 * 1024;` | 1MB Metadata 区域大小 | MS-VHDX 2.3 |
| `src/file/builder.rs:140` | `let metadata_offset = header_size * 2;` | Metadata 起始偏移 (2MB) | MS-VHDX 2.3 |
| `src/file/builder.rs:141` | `let bat_offset = metadata_offset + metadata_size;` | BAT 起始偏移 (3MB) | 实现定义 |
| `src/header/header.rs:36` | `pub const OFFSET_1: u64 = 64 * 1024;` | Header 1 偏移 (64KB) | MS-VHDX 2.2 |
| `src/header/header.rs:38` | `pub const OFFSET_2: u64 = 128 * 1024;` | Header 2 偏移 (128KB) | MS-VHDX 2.2 |
| `src/header/header.rs:34` | `pub const SIZE: usize = 4096;` | Header 结构大小 (4KB) | MS-VHDX 2.2 |

### 2.2 大小常量 (Size Constants)

#### 2.2.1 存储单位

| 位置 | 代码 | 含义 |
|------|------|------|
| `src/file/builder.rs:40` | `block_size: 1024 * 1024 * 32` | 默认块大小 32MB |
| `src/file/builder.rs:93` | `self.block_size < 1024 * 1024` | 最小块大小 1MB |
| `src/file/builder.rs:93` | `self.block_size > 256 * 1024 * 1024` | 最大块大小 256MB |
| `src/file/builder.rs:99` | `self.block_size % (1024 * 1024) != 0` | 块大小 1MB 对齐 |
| `src/file/builder.rs:137` | `let bat_size = ((num_bat_entries * 8 + 1024 * 1024 - 1) / (1024 * 1024)) * (1024 * 1024);` | BAT 大小 1MB 对齐计算 |
| `src/file/builder.rs:192` | `let mut region_data = vec![0u8; 64 * 1024];` | Region Table 大小 64KB |
| `src/file/builder.rs:297` | `let metadata_table_size = 64 * 1024;` | Metadata Table 大小 64KB |

#### 2.2.2 扇区大小

**512 字节扇区出现位置：**

| 位置 | 代码 |
|------|------|
| `src/file/builder.rs:41` | `logical_sector_size: 512` |
| `src/metadata/sector_size.rs:47` | `if size != 512 && size != 4096` |
| `src/payload/chunk.rs` | 多处测试代码 |
| `src/bat/table.rs:236` | `let logical_sector_size = 512;` (测试) |

**4096 字节扇区出现位置：**

| 位置 | 代码 |
|------|------|
| `src/file/builder.rs:42` | `physical_sector_size: 4096` |
| `src/metadata/sector_size.rs:47` | `if size != 512 && size != 4096` |
| `src/log/writer.rs:67` | `let data_sectors_size = (num_data_descriptors * 4096) as u32;` |
| `src/log/sector.rs:18` | `pub const SIZE: usize = 4096;` |
| `src/header/header.rs:34` | `pub const SIZE: usize = 4096;` |

### 2.3 VHDX 特定限制 (VHDX-Specific Limits)

| 位置 | 代码 | 含义 | 规范来源 |
|------|------|------|----------|
| `src/bat/table.rs:38` | `let chunk_size = (1u64 << 23) * logical_sector_size as u64;` | Chunk 大小 = 2^23 * 扇区大小 | MS-VHDX 2.4.2 |
| `src/payload/chunk.rs` | `2^23 = 8,388,608 sectors` | 每个 Chunk 的扇区数 | MS-VHDX 2.4.2 |
| `src/file/builder.rs:126` | `(1u64 << 23) * self.logical_sector_size as u64` | Chunk 大小计算 | MS-VHDX 2.4.2 |

### 2.4 对齐常量 (Alignment Values)

| 位置 | 代码 | 含义 |
|------|------|------|
| `src/block_io/dynamic.rs:200` | `(self.next_free_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1)` | 1MB 对齐计算 |
| `src/block_io/dynamic.rs:35` | `next_free_offset: 1024 * 1024` | 数据区起始偏移 1MB |
| `src/bat/table.rs:180` | `entry.file_offset_mb * 1024 * 1024` | MB 到字节转换 |

### 2.5 日志相关常量 (Log Constants)

| 位置 | 代码 | 含义 | 规范来源 |
|------|------|------|----------|
| `src/log/writer.rs:65` | `let header_size = 64u32;` | Log Entry Header 大小 | MS-VHDX 2.6.1 |
| `src/log/writer.rs:66` | `let descriptors_size = (num_data_descriptors * 32) as u32;` | Descriptor 大小 (32 bytes) | MS-VHDX 2.6.2 |
| `src/log/writer.rs:67` | `let data_sectors_size = (num_data_descriptors * 4096) as u32;` | Data Sector 大小 (4KB) | MS-VHDX 2.6.3 |
| `src/log/writer.rs:71` | `((total + 4095) / 4096) * 4096` | 4KB 对齐 | MS-VHDX 2.6 |
| `src/log/writer.rs:122` | `let sector_offset = 4096usize;` | Data Sector 在 Entry 中的偏移 | MS-VHDX 2.6 |
| `src/log/entry.rs:59` | `if entry_length == 0 || entry_length % 4096 != 0` | Entry 4KB 对齐验证 | MS-VHDX 2.6.1 |

## 3. 重构方案

### 3.1 创建统一常量模块

创建 `src/constants.rs`：

```rust
//! VHDX 全局常量定义
//!
//! 本模块包含所有 MS-VHDX 规范定义的常量以及实现相关的常量。
//! 所有常量按功能分组，并带有详细的文档注释。

// ============================================================================
// 基础存储单位 (Base Storage Units)
// ============================================================================

/// 1 KB = 1024 bytes
pub const KB: u64 = 1024;

/// 1 MB = 1024 * 1024 bytes
pub const MB: u64 = 1024 * 1024;

/// 1 GB = 1024 * 1024 * 1024 bytes
pub const GB: u64 = 1024 * 1024 * 1024;

// ============================================================================
// VHDX 文件布局常量 (File Layout Constants)
// ============================================================================

/// VHDX 文件头部区域大小 (1MB)
///
/// 根据 MS-VHDX 2.2.3，Header Section 大小为 1MB
pub const HEADER_SECTION_SIZE: u64 = MB;

/// Metadata 区域大小 (1MB)
///
/// 根据 MS-VHDX 2.3，Metadata Region 最大大小为 1MB
pub const METADATA_REGION_SIZE: u64 = MB;

/// VHDX 文件对齐单位 (1MB)
///
/// 所有区域必须 1MB 对齐，这是 VHDX 文件的基本对齐粒度
pub const VHDX_ALIGNMENT: u64 = MB;

/// Header 1 的文件偏移 (64KB)
///
/// 根据 MS-VHDX 2.2，Header 1 位于文件偏移 64KB 处
pub const HEADER_1_OFFSET: u64 = 64 * KB;

/// Header 2 的文件偏移 (128KB)
///
/// 根据 MS-VHDX 2.2，Header 2 位于文件偏移 128KB 处
pub const HEADER_2_OFFSET: u64 = 128 * KB;

/// Header 结构的大小 (4KB)
///
/// 每个 Header 占用 4KB，其余填充为零
pub const HEADER_SIZE: usize = 4096;

/// Region Table 的文件偏移 (192KB 和 256KB)
///
/// 根据 MS-VHDX 2.2.4，有两个 Region Table 副本
pub const REGION_TABLE_1_OFFSET: u64 = 192 * KB;
pub const REGION_TABLE_2_OFFSET: u64 = 256 * KB;

/// Region Table 的大小 (64KB)
pub const REGION_TABLE_SIZE: usize = 64 * 1024;

/// Metadata Table 的大小 (64KB)
pub const METADATA_TABLE_SIZE: usize = 64 * 1024;

// ============================================================================
// 扇区大小常量 (Sector Size Constants)
// ============================================================================

/// 标准逻辑扇区大小: 512 字节
///
/// 这是传统的磁盘扇区大小
pub const LOGICAL_SECTOR_SIZE_512: u32 = 512;

/// 大逻辑扇区大小: 4096 字节 (4K)
///
/// 现代磁盘使用的大扇区大小
pub const LOGICAL_SECTOR_SIZE_4096: u32 = 4096;

/// 标准物理扇区大小: 512 字节
pub const PHYSICAL_SECTOR_SIZE_512: u32 = 512;

/// 大物理扇区大小: 4096 字节 (4K)
pub const PHYSICAL_SECTOR_SIZE_4096: u32 = 4096;

/// 默认逻辑扇区大小
pub const DEFAULT_LOGICAL_SECTOR_SIZE: u32 = LOGICAL_SECTOR_SIZE_512;

/// 默认物理扇区大小
pub const DEFAULT_PHYSICAL_SECTOR_SIZE: u32 = PHYSICAL_SECTOR_SIZE_4096;

// ============================================================================
// 块大小常量 (Block Size Constants)
// ============================================================================

/// 默认块大小: 32MB
///
/// Windows 创建动态 VHDX 的默认块大小
pub const DEFAULT_BLOCK_SIZE: u32 = 32 * MB as u32;

/// 最小块大小: 1MB
///
/// 根据 MS-VHDX，最小支持的块大小为 1MB
pub const MIN_BLOCK_SIZE: u32 = MB as u32;

/// 最大块大小: 256MB
///
/// 根据 MS-VHDX，最大支持的块大小为 256MB
pub const MAX_BLOCK_SIZE: u32 = 256 * MB as u32;

// ============================================================================
// Chunk 相关常量 (Chunk Constants)
// ============================================================================

/// Chunk 大小计算因子: 2^23
///
/// 根据 MS-VHDX 2.4.2，ChunkSize = 2^23 * LogicalSectorSize
/// 对于 512 字节扇区，ChunkSize = 4GB
/// 对于 4096 字节扇区，ChunkSize = 32GB
pub const CHUNK_SIZE_SHIFT: u32 = 23;

/// 每个 Chunk 的扇区数: 2^23 = 8,388,608
pub const SECTORS_PER_CHUNK: u64 = 1u64 << 23;

/// 计算 Chunk 大小
#[inline]
pub const fn chunk_size(logical_sector_size: u32) -> u64 {
    SECTORS_PER_CHUNK * logical_sector_size as u64
}

/// 计算 Chunk Ratio (每个 Chunk 包含的 Block 数)
#[inline]
pub const fn chunk_ratio(block_size: u64, logical_sector_size: u32) -> u64 {
    chunk_size(logical_sector_size) / block_size
}

// ============================================================================
// BAT 相关常量 (BAT Constants)
// ============================================================================

/// BAT Entry 大小 (8 bytes)
///
/// 每个 BAT Entry 是 64 位值
pub const BAT_ENTRY_SIZE: usize = 8;

/// BAT Entry 中文件偏移的单位: MB
///
/// Entry 中的 file_offset_mb 字段以 MB 为单位
pub const BAT_OFFSET_UNIT: u64 = MB;

// ============================================================================
// 日志相关常量 (Log Constants)
// ============================================================================

/// Log Entry Header 大小 (64 bytes)
///
/// 根据 MS-VHDX 2.6.1
pub const LOG_ENTRY_HEADER_SIZE: usize = 64;

/// Log Data Descriptor 大小 (32 bytes)
///
/// 根据 MS-VHDX 2.6.2
pub const LOG_DATA_DESCRIPTOR_SIZE: usize = 32;

/// Log Zero Descriptor 大小 (32 bytes)
///
/// 根据 MS-VHDX 2.6.2
pub const LOG_ZERO_DESCRIPTOR_SIZE: usize = 32;

/// Log Data Sector 大小 (4096 bytes = 4KB)
///
/// 根据 MS-VHDX 2.6.3，每个 Data Sector 为 4KB
pub const LOG_DATA_SECTOR_SIZE: usize = 4096;

/// Log Entry 的对齐粒度 (4KB)
pub const LOG_ENTRY_ALIGNMENT: usize = 4096;

/// Log Entry 最小大小 (4KB)
pub const LOG_ENTRY_MIN_SIZE: usize = 4096;

// ============================================================================
// 数据区域常量 (Data Region Constants)
// ============================================================================

/// 数据区域起始偏移
///
/// 数据从 1MB 之后开始 (Header Section 之后)
pub const DATA_REGION_START_OFFSET: u64 = MB;

// ============================================================================
// 验证函数 (Validation Functions)
// ============================================================================

/// 验证扇区大小是否有效
pub const fn is_valid_sector_size(size: u32) -> bool {
    size == LOGICAL_SECTOR_SIZE_512 || size == LOGICAL_SECTOR_SIZE_4096
}

/// 验证块大小是否在有效范围内
pub const fn is_valid_block_size(size: u32) -> bool {
    size >= MIN_BLOCK_SIZE && size <= MAX_BLOCK_SIZE && size % MB as u32 == 0
}

/// 验证是否 1MB 对齐
pub const fn is_1mb_aligned(offset: u64) -> bool {
    offset % MB == 0
}

/// 验证是否 4KB 对齐
pub const fn is_4kb_aligned(offset: u64) -> bool {
    offset % 4096 == 0
}

/// 计算 1MB 对齐后的偏移
#[inline]
pub const fn align_1mb(offset: u64) -> u64 {
    (offset + MB - 1) & !(MB - 1)
}

/// 计算 4KB 对齐后的偏移
#[inline]
pub const fn align_4kb(offset: u64) -> u64 {
    (offset + 4096 - 1) & !(4096 - 1)
}
```

### 3.2 修改 Cargo.toml

添加常量模块到库导出：

```toml
[lib]
name = "vhdx_rs"
path = "src/lib.rs"
```

### 3.3 更新 lib.rs

在 `src/lib.rs` 中添加常量模块导出：

```rust
// 在文件顶部添加
pub mod constants;

// 重新导出常用常量以便使用
pub use constants::{
    MB, GB, KB,
    DEFAULT_BLOCK_SIZE, MIN_BLOCK_SIZE, MAX_BLOCK_SIZE,
    LOGICAL_SECTOR_SIZE_512, LOGICAL_SECTOR_SIZE_4096,
    DEFAULT_LOGICAL_SECTOR_SIZE, DEFAULT_PHYSICAL_SECTOR_SIZE,
    HEADER_1_OFFSET, HEADER_2_OFFSET, HEADER_SIZE,
    // ... 其他常用常量
};
```

### 3.4 分阶段替换计划

#### 阶段 1: 创建常量模块并添加基础常量

**目标文件**: `src/constants.rs` (新建)

**涉及修改**: 创建文件，定义所有常量

#### 阶段 2: 替换 Header 相关常量

**目标文件**: `src/header/header.rs`

**修改内容**:
```rust
// 修改前 (line 34-38)
pub const SIZE: usize = 4096;
pub const OFFSET_1: u64 = 64 * 1024;
pub const OFFSET_2: u64 = 128 * 1024;

// 修改后
use crate::constants::{HEADER_SIZE, HEADER_1_OFFSET, HEADER_2_OFFSET};
pub const SIZE: usize = HEADER_SIZE;
pub const OFFSET_1: u64 = HEADER_1_OFFSET;
pub const OFFSET_2: u64 = HEADER_2_OFFSET;
```

#### 阶段 3: 替换 Builder 中的布局常量

**目标文件**: `src/file/builder.rs`

**修改内容**:
```rust
// 修改前 (line 40)
block_size: 1024 * 1024 * 32, // 32MB default

// 修改后
block_size: constants::DEFAULT_BLOCK_SIZE,
```

```rust
// 修改前 (line 41-42)
logical_sector_size: 512,
physical_sector_size: 4096,

// 修改后
logical_sector_size: constants::DEFAULT_LOGICAL_SECTOR_SIZE,
physical_sector_size: constants::DEFAULT_PHYSICAL_SECTOR_SIZE,
```

```rust
// 修改前 (line 93)
if self.block_size < 1024 * 1024 || self.block_size > 256 * 1024 * 1024 {

// 修改后
if !constants::is_valid_block_size(self.block_size) {
```

```rust
// 修改前 (line 135-142)
let header_size = 1024 * 1024;
let metadata_size = 1024 * 1024;
let metadata_offset = header_size * 2;
let bat_offset = metadata_offset + metadata_size;

// 修改后
let header_section_size = constants::HEADER_SECTION_SIZE;
let metadata_size = constants::METADATA_REGION_SIZE;
let metadata_offset = header_section_size * 2;
let bat_offset = metadata_offset + metadata_size;
```

#### 阶段 4: 替换 BAT 中的 Chunk 计算

**目标文件**: `src/bat/table.rs`

**修改内容**:
```rust
// 修改前 (line 38)
let chunk_size = (1u64 << 23) * logical_sector_size as u64;

// 修改后
let chunk_size = constants::chunk_size(logical_sector_size);
```

```rust
// 修改前 (line 60)
let chunk_size = (1u64 << 23) * logical_sector_size as u64;

// 修改后
let chunk_size = constants::chunk_size(logical_sector_size);
```

```rust
// 修改前 (line 180)
let file_offset = entry.file_offset_mb * 1024 * 1024;

// 修改后
let file_offset = entry.file_offset_mb * constants::MB;
```

#### 阶段 5: 替换对齐计算

**目标文件**: `src/block_io/dynamic.rs`

**修改内容**:
```rust
// 修改前 (line 35)
next_free_offset: 1024 * 1024,

// 修改后
next_free_offset: constants::DATA_REGION_START_OFFSET,
```

```rust
// 修改前 (line 200)
let aligned_offset = (self.next_free_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1);

// 修改后
let aligned_offset = constants::align_1mb(self.next_free_offset);
```

```rust
// 修改前 (line 204)
let file_offset_mb = aligned_offset / (1024 * 1024);

// 修改后
let file_offset_mb = aligned_offset / constants::MB;
```

#### 阶段 6: 替换日志相关常量

**目标文件**: `src/log/writer.rs`

**修改内容**:
```rust
// 修改前 (line 64-71)
let header_size = 64u32;
let descriptors_size = (num_data_descriptors * 32) as u32;
let data_sectors_size = (num_data_descriptors * 4096) as u32;
let total = header_size + descriptors_size + data_sectors_size;
((total + 4095) / 4096) * 4096

// 修改后
use crate::constants::{
    LOG_ENTRY_HEADER_SIZE, LOG_DATA_DESCRIPTOR_SIZE,
    LOG_DATA_SECTOR_SIZE, LOG_ENTRY_ALIGNMENT, align_4kb
};
let header_size = LOG_ENTRY_HEADER_SIZE as u32;
let descriptors_size = (num_data_descriptors * LOG_DATA_DESCRIPTOR_SIZE) as u32;
let data_sectors_size = (num_data_descriptors * LOG_DATA_SECTOR_SIZE) as u32;
let total = header_size + descriptors_size + data_sectors_size;
align_4kb(total as u64) as u32
```

```rust
// 修改前 (line 84)
if data.len() != 4096 {

// 修改后
if data.len() != LOG_DATA_SECTOR_SIZE {
```

```rust
// 修改前 (line 122)
let sector_offset = 4096usize;

// 修改后
let sector_offset = LOG_DATA_SECTOR_SIZE;
```

#### 阶段 7: 替换扇区大小验证

**目标文件**: `src/file/builder.rs`, `src/metadata/sector_size.rs`

**修改内容**:
```rust
// 修改前 (src/file/builder.rs:106)
if self.logical_sector_size != 512 && self.logical_sector_size != 4096 {

// 修改后
if !constants::is_valid_sector_size(self.logical_sector_size) {
```

```rust
// 修改前 (src/metadata/sector_size.rs:47)
if size != 512 && size != 4096 {

// 修改后
if !constants::is_valid_sector_size(size) {
```

## 4. 代码示例

### 4.1 文件布局常量重构示例

**重构前** (`src/file/builder.rs`):

```rust
// 第 135-142 行
let header_size = 1024 * 1024; // 1MB header section
let metadata_size = 1024 * 1024; // 1MB metadata
let bat_size = ((num_bat_entries * 8 + 1024 * 1024 - 1) / (1024 * 1024)) * (1024 * 1024);

let metadata_offset = header_size * 2; // Metadata at 2MB
let bat_offset = metadata_offset + metadata_size; // BAT after metadata (3MB)
let data_offset = bat_offset + bat_size; // Payload data after BAT
```

**重构后**:

```rust
use crate::constants::{
    HEADER_SECTION_SIZE, METADATA_REGION_SIZE, VHDX_ALIGNMENT, align_1mb
};

let header_section_size = HEADER_SECTION_SIZE;
let metadata_size = METADATA_REGION_SIZE;
let bat_size = align_1mb(num_bat_entries * constants::BAT_ENTRY_SIZE as u64);

let metadata_offset = header_section_size * 2; // Metadata at 2MB
let bat_offset = metadata_offset + metadata_size; // BAT after metadata (3MB)
let data_offset = bat_offset + bat_size; // Payload data after BAT
```

### 4.2 Header 偏移量重构示例

**重构前** (`src/header/header.rs`):

```rust
// 第 34-38 行
pub const SIZE: usize = 4096;
pub const OFFSET_1: u64 = 64 * 1024;  // 64KB
pub const OFFSET_2: u64 = 128 * 1024; // 128KB
```

**重构后**:

```rust
use crate::constants::{HEADER_SIZE, HEADER_1_OFFSET, HEADER_2_OFFSET};

pub const SIZE: usize = HEADER_SIZE;
pub const OFFSET_1: u64 = HEADER_1_OFFSET;
pub const OFFSET_2: u64 = HEADER_2_OFFSET;
```

### 4.3 Chunk 计算重构示例

**重构前** (`src/bat/table.rs`):

```rust
// 第 38 行
let chunk_size = (1u64 << 23) * logical_sector_size as u64; // 2^23 = 8,388,608
```

**重构后**:

```rust
use crate::constants::{SECTORS_PER_CHUNK, chunk_size};

let chunk_size = chunk_size(logical_sector_size);
// 或者展开形式：
// let chunk_size = SECTORS_PER_CHUNK * logical_sector_size as u64;
```

### 4.4 对齐计算重构示例

**重构前** (`src/block_io/dynamic.rs`):

```rust
// 第 200 行
let aligned_offset = (self.next_free_offset + (1024 * 1024 - 1)) & !(1024 * 1024 - 1);
```

**重构后**:

```rust
use crate::constants::align_1mb;

let aligned_offset = align_1mb(self.next_free_offset);
```

### 4.5 日志常量重构示例

**重构前** (`src/log/writer.rs`):

```rust
// 第 64-72 行
fn calculate_entry_size(&self, num_data_descriptors: usize) -> u32 {
    let header_size = 64u32;
    let descriptors_size = (num_data_descriptors * 32) as u32;
    let data_sectors_size = (num_data_descriptors * 4096) as u32;
    let total = header_size + descriptors_size + data_sectors_size;

    // Round up to 4KB
    ((total + 4095) / 4096) * 4096
}
```

**重构后**:

```rust
use crate::constants::{
    LOG_ENTRY_HEADER_SIZE, LOG_DATA_DESCRIPTOR_SIZE,
    LOG_DATA_SECTOR_SIZE, LOG_ENTRY_ALIGNMENT, align_4kb
};

fn calculate_entry_size(&self, num_data_descriptors: usize) -> u32 {
    let header_size = LOG_ENTRY_HEADER_SIZE as u32;
    let descriptors_size = (num_data_descriptors * LOG_DATA_DESCRIPTOR_SIZE) as u32;
    let data_sectors_size = (num_data_descriptors * LOG_DATA_SECTOR_SIZE) as u32;
    let total = header_size + descriptors_size + data_sectors_size;

    align_4kb(total as u64) as u32
}
```

## 5. 验证策略

### 5.1 编译时验证

使用 Rust 的 `const` 断言确保常量值正确：

```rust
// 在 constants.rs 末尾添加
#[cfg(test)]
mod static_assertions {
    use super::*;

    // 验证基本单位
    const_assert!(KB == 1024);
    const_assert!(MB == 1024 * 1024);
    const_assert!(GB == 1024 * 1024 * 1024);

    // 验证 Header 偏移
    const_assert!(HEADER_1_OFFSET == 64 * KB);
    const_assert!(HEADER_2_OFFSET == 128 * KB);

    // 验证 VHDX 规范值
    const_assert!(SECTORS_PER_CHUNK == 1u64 << 23);

    // 验证块大小范围
    const_assert!(MIN_BLOCK_SIZE == MB as u32);
    const_assert!(MAX_BLOCK_SIZE == 256 * MB as u32);
}
```

### 5.2 运行时验证

添加单元测试确保常量使用正确：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_sizes() {
        assert!(is_valid_sector_size(512));
        assert!(is_valid_sector_size(4096));
        assert!(!is_valid_sector_size(1024));
        assert!(!is_valid_sector_size(2048));
    }

    #[test]
    fn test_block_size_validation() {
        assert!(is_valid_block_size(1 * MB as u32));
        assert!(is_valid_block_size(32 * MB as u32));
        assert!(is_valid_block_size(256 * MB as u32));
        assert!(!is_valid_block_size(512 * MB as u32)); // 超过最大值
        assert!(!is_valid_block_size(512 * 1024)); // 不是 1MB 对齐
    }

    #[test]
    fn test_alignment_functions() {
        assert_eq!(align_1mb(0), 0);
        assert_eq!(align_1mb(1), MB);
        assert_eq!(align_1mb(MB - 1), MB);
        assert_eq!(align_1mb(MB), MB);
        assert_eq!(align_1mb(MB + 1), 2 * MB);

        assert_eq!(align_4kb(0), 0);
        assert_eq!(align_4kb(1), 4096);
        assert_eq!(align_4kb(4095), 4096);
        assert_eq!(align_4kb(4096), 4096);
    }

    #[test]
    fn test_chunk_calculation() {
        // 512 字节扇区 -> 4GB Chunk
        assert_eq!(chunk_size(512), 4 * GB);

        // 4096 字节扇区 -> 32GB Chunk
        assert_eq!(chunk_size(4096), 32 * GB);

        // Chunk Ratio: 1MB 块，512 字节扇区
        assert_eq!(chunk_ratio(MB, 512), 4096);
    }
}
```

### 5.3 代码审查清单

替换完成后，使用以下清单验证：

- [ ] `src/constants.rs` 已创建并包含所有常量
- [ ] 所有 `1024 * 1024` 字面量已被替换
- [ ] 所有 `64 * 1024` 字面量已被替换
- [ ] 所有 `128 * 1024` 字面量已被替换
- [ ] 所有 `256 * 1024 * 1024` 字面量已被替换
- [ ] 所有 `(1u64 << 23)` 已被替换为 `SECTORS_PER_CHUNK`
- [ ] 所有扇区大小验证使用 `is_valid_sector_size()`
- [ ] 所有块大小验证使用 `is_valid_block_size()`
- [ ] 所有 1MB 对齐计算使用 `align_1mb()`
- [ ] 所有 4KB 对齐计算使用 `align_4kb()`
- [ ] 注释中不再需要对魔法数字进行解释（常量名已自解释）
- [ ] `cargo test` 全部通过
- [ ] `cargo clippy` 无警告

### 5.4 自动化检查脚本

创建 `scripts/check_magic_numbers.sh`：

```bash
#!/bin/bash

# 检查是否还有未替换的魔法数字
echo "检查剩余的魔法数字..."

# 常见的魔法数字模式
patterns=(
    "1024 \* 1024"
    "64 \* 1024"
    "128 \* 1024"
    "256 \* 1024 \* 1024"
    "1u64 << 23"
    "!= 512 && != 4096"
)

found=0
for pattern in "${patterns[@]}"; do
    if grep -r "$pattern" src/ --include="*.rs" | grep -v "constants.rs" | grep -v "//"; then
        echo "找到未替换的魔法数字: $pattern"
        found=1
    fi
done

if [ $found -eq 0 ]; then
    echo "所有魔法数字已替换完成！"
    exit 0
else
    exit 1
fi
```

### 5.5 性能回归测试

确保重构不会引入运行时开销：

```rust
#[cfg(test)]
mod benches {
    use super::*;
    use test::Bencher;

    #[bench]
    fn bench_align_1mb(b: &mut Bencher) {
        b.iter(|| {
            align_1mb(12345678)
        });
    }

    #[bench]
    fn bench_align_4kb(b: &mut Bencher) {
        b.iter(|| {
            align_4kb(12345678)
        });
    }

    #[bench]
    fn bench_chunk_calculation(b: &mut Bencher) {
        b.iter(|| {
            chunk_size(512);
            chunk_size(4096);
        });
    }
}
```

## 6. 实施时间线

| 阶段 | 任务 | 预计时间 | 依赖 |
|------|------|----------|------|
| 1 | 创建 `src/constants.rs` | 2 小时 | 无 |
| 2 | 更新 `src/lib.rs` 导出常量 | 30 分钟 | 阶段 1 |
| 3 | 重构 `src/header/header.rs` | 1 小时 | 阶段 2 |
| 4 | 重构 `src/file/builder.rs` | 2 小时 | 阶段 2 |
| 5 | 重构 `src/bat/table.rs` | 1 小时 | 阶段 2 |
| 6 | 重构 `src/block_io/dynamic.rs` | 1 小时 | 阶段 2 |
| 7 | 重构 `src/log/writer.rs` | 1 小时 | 阶段 2 |
| 8 | 重构其他文件中的扇区大小验证 | 1 小时 | 阶段 2 |
| 9 | 运行测试和 clippy | 30 分钟 | 阶段 3-8 |
| 10 | 代码审查和文档更新 | 1 小时 | 阶段 9 |

**总计**: 约 11 小时

## 7. 相关文档

- [MS-VHDX 规范](../spec/MS-VHDX.md)
- [VHDX 文件格式概述](../spec/vhdx-overview.md)
- [Rust 常量最佳实践](https://doc.rust-lang.org/book/ch03-05-control-flow.html)

---

**文档版本**: 1.0  
**创建日期**: 2026-03-24  
**最后更新**: 2026-03-24  
**作者**: vhdx-rs 重构团队
