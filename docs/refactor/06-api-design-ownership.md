# 重构：API 设计与所有权模式

## 概述

修复 LogWriter 消耗导致的所有权问题，并改进父磁盘处理。这次重构解决了造成所有权冲突并使库难以使用的 API 设计问题。

## 当前问题

### 问题 1：LogWriter 在首次使用时被消耗

**当前实现**：
```rust
// src/file/vhdx_file.rs:358-368
pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
    // ...
    let result = match self.disk_type {
        _ => {
            let mut block_io = DynamicBlockIo::new(&mut self.file, &mut self.bat, ...);
            
            // 问题：LogWriter 被取出并消耗！
            if let Some(log_writer) = self.log_writer.take() {
                let result = block_io
                    .with_log_writer(log_writer)
                    .write(virtual_offset, buf);
                // LogWriter 已消失 - 无法再次使用！
                result
            } else {
                block_io.write(virtual_offset, buf)
            }
        }
    };
    // ...
}
```

**问题**：
- LogWriter 只能使用一次
- 后续写入没有日志保护
- 注释承认了问题但没有修复它

### 问题 2：递归父磁盘加载

**当前实现**：
```rust
// src/file/vhdx_file.rs:153-174
let parent = if disk_type == DiskType::Differencing {
    if let Ok(locator) = metadata.parent_locator() {
        if let Some(parent_path) = locator.parent_path() {
            // 问题：递归打开 - 栈溢出风险
            Some(Box::new(Self::open(parent_full_path, true)?))
        } else { None }
    } else { None }
} else { None };
```

**问题**：
- 在每个差异磁盘上递归打开父磁盘
- 深层链可能导致栈溢出
- 没有延迟加载
- 没有循环依赖检测

### 问题 3：BlockIo 借用模式

**当前实现**：
```rust
// src/block_io/dynamic.rs:16-27
pub struct DynamicBlockIo<'a> {
    pub file: &'a mut std::fs::File,
    pub bat: &'a mut Bat,
    // ...
}

// 在 VhdxFile::read 中使用 (src/file/vhdx_file.rs:320-328)
match self.disk_type {
    DiskType::Fixed => {
        let mut fixed_io = FixedBlockIo::new(&mut self.file, &self.bat, ...);
        // 问题：每次调用都创建新实例 - 效率低下
    }
    _ => {
        let mut block_io = DynamicBlockIo::new(&mut self.file, &mut self.bat, ...);
        // 问题：两次可变借用！
    }
}
```

**问题**：
- 每次读/写都创建 BlockIo 实例
- 多次可变借用（File + BAT）
- 重复操作效率低下

## 建议方案

### 方案 1：使用内部可变性的共享 LogWriter

```rust
// src/file/vhdx_file.rs

use std::cell::RefCell;
use std::rc::Rc;

pub struct VhdxFile {
    // ... 其他字段 ...
    
    // 共享 LogWriter - 可以克隆并可变借用
    log_writer: Option<Rc<RefCell<LogWriter>>>,
}

impl VhdxFile {
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        if self.read_only {
            return Err(VhdxError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "文件是只读的",
            )));
        }
        
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 克隆 Rc - 代价低，不是克隆 LogWriter 本身
        let log_writer = self.log_writer.clone();
        
        let result = match self.disk_type {
            DiskType::Fixed => {
                let mut fixed_io = FixedBlockIo::new(&mut self.file, &self.bat, ...);
                fixed_io.write(virtual_offset, buf)
            }
            _ => {
                let mut block_io = DynamicBlockIo::new(&mut self.file, &mut self.bat, ...);
                
                // 传递共享的 LogWriter - 不会被消耗！
                if let Some(lw) = log_writer {
                    block_io.with_log_writer(lw).write(virtual_offset, buf)
                } else {
                    block_io.write(virtual_offset, buf)
                }
            }
        };
        
        // LogWriter 仍然在 self.log_writer 中可用
        
        if result.is_ok() {
            self.header.data_write_guid = Guid::new_v4();
            self.update_headers()?;
        }
        
        result
    }
}

// 更新 BlockIo 以接受 Rc<RefCell<LogWriter>>
pub struct DynamicBlockIo<'a> {
    file: &'a mut File,
    bat: &'a mut Bat,
    virtual_disk_size: u64,
    log_writer: Option<Rc<RefCell<LogWriter>>>,
}

impl<'a> DynamicBlockIo<'a> {
    pub fn with_log_writer(mut self, log_writer: Rc<RefCell<LogWriter>>) -> Self {
        self.log_writer = Some(log_writer);
        self
    }
    
    fn allocate_block(&mut self, block_idx: u64) -> Result<u64> {
        // ...
        if let Some(lw) = &self.log_writer {
            // 只在需要时可变借用
            lw.borrow_mut().write_data_entry(...)?;
        }
        // ...
    }
}
```

### 方案 2：延迟父磁盘加载

```rust
// src/file/vhdx_file.rs

use std::sync::{Arc, RwLock};

pub struct VhdxFile {
    // ... 其他字段 ...
    
    // 父磁盘路径，用于延迟加载
    parent_path: Option<PathBuf>,
    
    // 延迟加载的父磁盘 - 线程安全
    parent: Option<Arc<RwLock<VhdxFile>>>,
    
    // 防止循环依赖
    loaded_parents: Arc<Mutex<HashSet<PathBuf>>>,
}

impl VhdxFile {
    pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
        // ... 打开逻辑 ...
        
        // 存储父路径，暂不加载
        let parent_path = if disk_type == DiskType::Differencing {
            Self::extract_parent_path(&metadata, &path)?
        } else {
            None
        };
        
        Ok(VhdxFile {
            // ...
            parent_path,
            parent: None,
            loaded_parents: Arc::new(Mutex::new(HashSet::new())),
        })
    }
    
    /// 获取父磁盘，如有必要则加载
    pub fn get_parent(&mut self) -> Result<Option<Arc<RwLock<VhdxFile>>>> {
        if self.parent.is_some() {
            return Ok(self.parent.clone());
        }
        
        let parent_path = match &self.parent_path {
            Some(p) => p.clone(),
            None => return Ok(None),
        };
        
        // 检查循环依赖
        {
            let loaded = self.loaded_parents.lock()?;
            if loaded.contains(&parent_path) {
                return Err(VhdxError::InvalidMetadata(
                    "检测到循环父依赖".to_string()
                ));
            }
        }
        
        // 加载父磁盘
        let parent = Self::open(&parent_path, true)?;
        
        // 跟踪已加载的父磁盘
        {
            let mut loaded = self.loaded_parents.lock()?;
            loaded.insert(parent_path);
        }
        
        self.parent = Some(Arc::new(RwLock::new(parent)));
        Ok(self.parent.clone())
    }
    
    /// 从父磁盘读取或返回零
    pub fn read_from_parent(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        match self.get_parent()? {
            Some(parent) => {
                parent.write()?.read(offset, buf)
            }
            None => {
                buf.fill(0);
                Ok(buf.len())
            }
        }
    }
}
```

### 方案 3：缓存 BlockIo 实例

```rust
// src/file/vhdx_file.rs

pub struct VhdxFile {
    // ... 其他字段 ...
    
    // 缓存的 BlockIo - 避免每次操作都重新创建
    cached_block_io: Option<BlockIoCache>,
}

/// 块 I/O 缓存，避免重复分配
struct BlockIoCache {
    disk_type: DiskType,
    // 使用枚举存储不同类型的 BlockIo
    inner:
