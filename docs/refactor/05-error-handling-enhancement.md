# 重构：错误处理增强

## 概述

为错误消息添加丰富的上下文信息，并改进整个代码库的错误处理。此次重构将通用错误变体转换为描述性、可操作性的错误消息，有助于调试并提升开发体验。

## 当前状态

### 存在的问题

#### 1. 通用错误变体
```rust
// src/error.rs:40-44
#[error("Invalid BAT entry")]
InvalidBatEntry,

#[error("Block not present")]
BlockNotPresent,

#[error("Invalid sector bitmap")]
InvalidSectorBitmap,
```

**问题所在**：
- 没有关于哪个块失败的信息
- 没有指示正在尝试什么操作
- 难以调试，需要添加打印语句

#### 2. 缺少错误上下文
```rust
// src/block_io/dynamic.rs:84
PayloadBlockState::Undefined => {
    return Err(VhdxError::InvalidBatEntry);  // 哪个块？
}

// src/bat/table.rs:74
if offset + 8 > data.len() {
    return Err(VhdxError::InvalidBatEntry);  // 哪个条目？
}
```

#### 3. 没有错误链
```rust
// src/file/vhdx_file.rs:165
Some(Box::new(Self::open(parent_full_path, true)?))
// 父级打开失败信息丢失 - 只返回通用错误
```

## 建议的解决方案

### 增强的错误类型

```rust
// src/error.rs

use thiserror::Error;
use std::io;

pub type Result<T> = std::result::Result<T, VhdxError>;

#[derive(Error, Debug)]
pub enum VhdxError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Invalid signature: expected {expected:?}, got {got:?}")]
    InvalidSignature { expected: String, got: String },
    
    #[error("Invalid checksum for {component} at offset {offset}")]
    InvalidChecksum { component: String, offset: u64 },
    
    #[error("Invalid file type identifier")]
    InvalidFileType,
    
    #[error("No valid header found (tried header 1 and 2)")]
    NoValidHeader,
    
    #[error("Corrupt VHDX file: {0}")]
    Corrupt(String),
    
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),
    
    #[error("Invalid region: {0}")]
    InvalidRegion(String),
    
    #[error("Required region not found: {0}")]
    RequiredRegionNotFound(String),
    
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),
    
    // 增强的 BAT 错误
    #[error("Invalid BAT entry at index {index}: {reason}")]
    InvalidBatEntry { index: u64, reason: String },
    
    #[error("BAT entry not found for block {block_idx}")]
    BatEntryNotFound { block_idx: u64 },
    
    #[error("Block {block_idx} is {state:?}, expected FullyPresent")]
    BlockNotPresent { block_idx: u64, state: PayloadBlockState },
    
    // 增强的日志错误
    #[error("Log replay failed at sequence {sequence}: {reason}")]
    LogReplayFailed { sequence: u64, reason: String },
    
    #[error("Invalid log entry at offset {offset}: {reason}")]
    InvalidLogEntry { offset: u64, reason: String },
    
    // 增强的父级错误
    #[error("Parent disk not found at path: {path}")]
    ParentNotFound { path: String },
    
    #[error("Parent GUID mismatch: expected {expected}, got {actual}")]
    ParentGuidMismatch { expected: String, actual: String },
    
    // 增强的扇区位图错误
    #[error("Invalid sector bitmap for chunk {chunk_idx}: {reason}")]
    InvalidSectorBitmap { chunk_idx: u64, reason: String },
    
    // 增强的偏移量错误
    #[error("Invalid virtual offset {offset} (max: {max_size})")]
    InvalidOffset { offset: u64, max_size: u64 },
    
    #[error("Offset {offset} is not aligned to {alignment}")]
    Alignment { offset: u64, alignment: u64 },
    
    #[error("File too small: expected at least {expected} bytes, got {actual}")]
    FileTooSmall { expected: u64, actual: u64 },
    
    // 上下文包装器
    #[error("{operation} failed: {source}")]
    WithContext {
        operation: String,
        #[source]
        source: Box<VhdxError>,
    },
}

impl VhdxError {
    /// 用上下文包装错误
    pub fn with_context(self, operation: impl Into<String>) -> Self {
        Self::WithContext {
            operation: operation.into(),
            source: Box::new(self),
        }
    }
    
    /// 获取错误的根本原因
    pub fn root_cause(&self) -> &Self {
        match self {
            Self::WithContext { source, .. } => source.root_cause(),
            _ => self,
        }
    }
}
```

### 上下文宏

```rust
// src/error.rs

/// 为结果添加上下文的宏
#[macro_export]
macro_rules! with_context {
    ($result:expr, $fmt:literal $(, $arg:expr)*) => {
        $result.map_err(|e| e.with_context(format!($fmt $(, $arg)*)))
    };
}

/// 用操作上下文包装错误的宏
#[macro_export]
macro_rules! wrap_err {
    ($result:expr, $operation:expr) => {
        $result.map_err(|e| VhdxError::WithContext {
            operation: $operation.to_string(),
            source: Box::new(e),
        })
    };
}
```

## 前后对比示例

### 示例 1：BAT 条目未找到

**之前**：
```rust
// src/block_io/dynamic.rs:72-84
match self.bat.get_payload_entry(block_idx) {
    Some(entry) => { /* ... */ }
    None => {
        return Err(VhdxError::InvalidBatEntry);
    }
}
```

**之后**：
```rust
match self.bat.get_payload_entry(block_idx) {
    Some(entry) => { /* ... */ }
    None => {
        return Err(VhdxError::BatEntryNotFound { block_idx });
    }
}
```

### 示例 2：块分配失败

**之前**：
```rust
// src/block_io/dynamic.rs:151-176
let file_offset = match entry.state {
    PayloadBlockState::FullyPresent => {
        entry.file_offset().ok_or(VhdxError::InvalidBatEntry)?
    }
    PayloadBlockState::NotPresent | PayloadBlockState::Zero => {
        self.allocate_block(block_idx)?;
        self.bat.get_payload_entry(block_idx)
            .and_then(|e| e.file_offset())
            .ok_or(VhdxError::InvalidBatEntry)?
    }
    PayloadBlockState::Undefined => {
        return Err(VhdxError::InvalidBatEntry);
    }
};
```

**之后**：
```rust
let file_offset = match entry.state {
    PayloadBlockState::FullyPresent => {
        entry.file_offset().ok_or_else(|| {
            VhdxError::InvalidBatEntry {
                index: block_idx,
                reason: "FullyPresent block has no file offset".to_string(),
            }
        })?
    }
    PayloadBlockState::NotPresent | PayloadBlockState::Zero => {
        self.allocate_block(block_idx)
            .with_context(format!("allocating block {}", block_idx))?;
        self.bat.get_payload_entry(block_idx)
            .and_then(|e| e.file_offset())
            .ok_or_else(|| {
                VhdxError::InvalidBatEntry {
                    index: block_idx,
                    reason: "Block not found after allocation".to_string(),
                }
            })?
    }
    PayloadBlockState::Undefined => {
        return Err(VhdxError::InvalidBatEntry {
            index: block_idx,
            reason: "Block state is Undefined".to_string(),
        });
    }
};
```

### 示例 3：父磁盘加载

**之前**：
```rust
// src/file/vhdx_file.rs:165
Some(Box::new(Self::open(parent_full_path, true)?))
```

**之后**：
```rust
Some(Box::new(
    Self::open(parent_full_path, true)
        .with_context(format!("loading parent disk from {}", parent_full_path.display()))?
))
```

### 示例 4：扇区位图验证

**之前**：
```rust
// src/block_io/differencing.rs:76-78
let state_bits = bitmap_entry.raw & 0x7;
if state_bits != SectorBitmapState::Present as u64 {
    return Err(VhdxError::InvalidSectorBitmap);
}
```

**之后**：
```rust
let state_bits = bitmap_entry.raw & 0x7;
if state_bits != SectorBitmapState::Present as u64 {
    return Err(VhdxError::InvalidSectorBitmap {
        chunk_idx,
        reason: format!("expected state Present (6), got {}", state_bits),
    });
}
```

## 迁移策略

### Phase 1: Up
