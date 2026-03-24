# Block I/O 实现详述

本文档详细介绍 vhdx-rs 旧版的块级 I/O 实现，包括 Trait 设计、三种磁盘类型的具体实现以及缓存机制。

---

## 1. Block I/O 架构概览

### 1.1 设计目标

- **统一接口**: 为 Fixed、Dynamic、Differencing 三种磁盘类型提供一致的读写接口
- **按需加载**: 动态磁盘只在实际写入时才分配块
- **差异链**: 差异磁盘支持父磁盘链的透明读取
- **性能优化**: 可选的块缓存减少重复 I/O

### 1.2 模块结构

```
src/block_io/
├── mod.rs           # 模块入口，导出公共类型
├── traits.rs        # BlockIo Trait 定义
├── fixed.rs         # 固定磁盘实现
├── dynamic.rs       # 动态磁盘实现
├── differencing.rs  # 差异磁盘实现
└── cache.rs         # 块缓存实现
```

---

## 2. BlockIo Trait

**文件**: `src/block_io/traits.rs`

### 2.1 核心 Trait

```rust
/// 块级 I/O 接口
pub trait BlockIo {
    /// 从虚拟偏移读取数据
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;
    
    /// 写入数据到虚拟偏移
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize>;
    
    /// 获取虚拟磁盘大小
    fn virtual_disk_size(&self) -> u64;
    
    /// 获取块大小
    fn block_size(&self) -> u32;
}

/// 块分配接口（用于动态/差异磁盘）
pub trait BlockAllocator {
    /// 分配一个新的块
    fn allocate_block(&mut self, block_idx: u64) -> Result<u64>;
    
    /// 检查块是否已分配
    fn is_block_allocated(&self, block_idx: u64) -> bool;
    
    /// 获取块在文件中的偏移
    fn get_block_offset(&self, block_idx: u64) -> Option<u64>;
}

/// 差异磁盘接口
pub trait DifferencingIo {
    /// 检查块是否存在于当前磁盘（而非父磁盘）
    fn has_block_locally(&self, block_idx: u64) -> bool;
    
    /// 从父磁盘读取
    fn read_from_parent(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;
}
```

### 2.2 设计决策

**为什么使用 Trait？**

1. **多态性**: VhdxFile 不需要知道具体磁盘类型，只需调用 BlockIo 方法
2. **扩展性**: 易于添加新的磁盘类型（如精简配置磁盘）
3. **测试**: 可以为测试实现 Mock BlockIo

**方法签名设计**:

- `&mut self`: 读写操作可能修改内部状态（如分配新块、更新缓存）
- `virtual_offset`: 虚拟磁盘上的逻辑偏移（而非文件偏移）
- 返回 `Result<usize>`: 符合 Rust I/O 惯例，支持部分读取

---

## 3. FixedBlockIo

**文件**: `src/block_io/fixed.rs`

### 3.1 实现原理

固定磁盘在创建时就分配了所有块，因此虚拟偏移到文件偏移的转换是直接的数学计算：

```
file_offset = data_offset + (block_idx * block_size) + offset_in_block
```

其中 `data_offset` 是数据区域起始偏移（通常是 3MB 或更大，取决于 BAT 大小）。

### 3.2 数据结构

```rust
pub struct FixedBlockIo<'a> {
    file: &'a mut File,           // 底层文件引用
    bat: &'a Bat,                 // BAT 引用（用于获取数据偏移）
    virtual_disk_size: u64,       // 虚拟磁盘大小
}

impl<'a> FixedBlockIo<'a> {
    pub fn new(
        file: &'a mut File,
        bat: &'a Bat,
        virtual_disk_size: u64,
    ) -> Self {
        FixedBlockIo {
            file,
            bat,
            virtual_disk_size,
        }
    }
    
    /// 计算数据区域起始偏移
    fn data_offset(&self) -> u64 {
        // data_offset = bat.bat_file_offset + bat.bat_size_bytes()
        // 简化实现：假设数据紧跟在 BAT 后面
        self.bat.bat_file_offset + self.bat.bat_size_bytes()
    }
}
```

### 3.3 读取实现

```rust
impl<'a> BlockIo for FixedBlockIo<'a> {
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        let offset_in_block = virtual_offset % self.bat.block_size;
        
        // 3. 检查块状态
        let entry = self.bat.get_payload_entry(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        
        match entry.state {
            PayloadBlockState::FullyPresent => {
                // 4. 计算文件偏移并读取
                let file_offset = entry.file_offset_mb * 1024 * 1024 + offset_in_block;
                self.file.seek(SeekFrom::Start(file_offset))?;
                
                // 5. 限制读取长度不超过块边界
                let bytes_remaining = self.bat.block_size - offset_in_block;
                let to_read = buf.len().min(bytes_remaining as usize);
                
                self.file.read_exact(&mut buf[..to_read])?;
                Ok(to_read)
            }
            _ => {
                // 固定磁盘不应出现其他状态
                Err(VhdxError::InvalidBatEntry)
            }
        }
    }
}
```

### 3.4 写入实现

```rust
impl<'a> BlockIo for FixedBlockIo<'a> {
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        let offset_in_block = virtual_offset % self.bat.block_size;
        
        // 3. 获取 BAT 条目
        let entry = self.bat.get_payload_entry(block_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        
        match entry.state {
            PayloadBlockState::FullyPresent => {
                // 4. 计算文件偏移并写入
                let file_offset = entry.file_offset_mb * 1024 * 1024 + offset_in_block;
                self.file.seek(SeekFrom::Start(file_offset))?;
                
                // 5. 限制写入长度
                let bytes_remaining = self.bat.block_size - offset_in_block;
                let to_write = buf.len().min(bytes_remaining as usize);
                
                self.file.write_all(&buf[..to_write])?;
                self.file.flush()?;
                Ok(to_write)
            }
            _ => Err(VhdxError::InvalidBatEntry),
        }
    }
}
```

### 3.5 性能特点

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 读取 | O(1) | 直接偏移计算 + 单次 seek |
| 写入 | O(1) | 直接偏移计算 + 单次 seek |
| 空间 | 预先分配 | 创建时分配完整大小 |

**优点**:
- 最快的读写性能（无 BAT 查询开销）
- 简单的偏移计算
- 无碎片化问题

**缺点**:
- 占用更多磁盘空间
- 创建时间较长

---

## 4. DynamicBlockIo

**文件**: `src/block_io/dynamic.rs`

### 4.1 实现原理

动态磁盘按需分配块。首次写入某个块时，需要在文件中分配空间并更新 BAT。

### 4.2 数据结构

```rust
pub struct DynamicBlockIo<'a> {
    file: &'a mut File,
    bat: &'a mut Bat,              // 可变引用（需要更新 BAT）
    virtual_disk_size: u64,
    current_file_size: u64,        // 当前文件大小（用于分配新块）
    log_writer: Option<LogWriter>, // 可选的日志写入器
}

impl<'a> DynamicBlockIo<'a> {
    pub fn new(
        file: &'a mut File,
        bat: &'a mut Bat,
        virtual_disk_size: u64,
    ) -> Self {
        DynamicBlockIo {
            file,
            bat,
            virtual_disk_size,
            current_file_size: 0, // 延迟初始化
            log_writer: None,
        }
    }
    
    /// 附加日志写入器
    pub fn with_log_writer(mut self, log_writer: LogWriter) -> Self {
        self.log_writer = Some(log_writer);
        self
    }
    
    /// 获取当前文件大小
    fn ensure_file_size(&mut self) -> Result<u64> {
        if self.current_file_size == 0 {
            self.current_file_size = self.file.seek(SeekFrom::End(0))?;
        }
        Ok(self.current_file_size)
    }
}
```

### 4.3 块分配实现

```rust
impl<'a> BlockAllocator for DynamicBlockIo<'a> {
    fn allocate_block(&mut self, block_idx: u64) -> Result<u64> {
        // 1. 检查是否已分配
        if let Some(entry) = self.bat.get_payload_entry(block_idx) {
            if entry.state == PayloadBlockState::FullyPresent {
                return Ok(entry.file_offset_mb * 1024 * 1024);
            }
        }
        
        // 2. 计算新块位置（文件末尾）
        let file_size = self.ensure_file_size()?;
        let block_offset_mb = file_size / (1024 * 1024);
        
        // 3. 分配空间（写入零或截断扩展）
        let block_size = self.bat.block_size;
        self.file.seek(SeekFrom::Start(file_size + block_size - 1))?;
        self.file.write_all(&[0])?;
        
        // 4. 创建新 BAT Entry
        let new_entry = BatEntry::new(
            PayloadBlockState::FullyPresent,
            block_offset_mb,
        );
        
        // 5. 更新 BAT（通过日志保证原子性）
        if let Some(ref mut log) = self.log_writer {
            // 使用日志写入
            let bat_file_offset = self.bat.get_bat_entry_file_offset(
                self.bat.payload_bat_index(block_idx).unwrap()
            );
            log.write_data(bat_file_offset, &new_entry.to_bytes())?;
        } else {
            // 直接写入（不安全，仅用于只读场景）
            self.bat.update_payload_entry(block_idx, new_entry)?;
            let bat_data = self.bat.to_bytes();
            self.file.seek(SeekFrom::Start(self.bat.bat_file_offset))?;
            self.file.write_all(&bat_data)?;
        }
        
        // 6. 更新文件大小
        self.current_file_size = file_size + block_size;
        
        Ok(file_size)
    }
    
    fn is_block_allocated(&self, block_idx: u64) -> bool {
        match self.bat.get_payload_entry(block_idx) {
            Some(entry) => entry.state == PayloadBlockState::FullyPresent,
            None => false,
        }
    }
    
    fn get_block_offset(&self, block_idx: u64) -> Option<u64> {
        self.bat.get_payload_entry(block_idx).and_then(|entry| {
            match entry.state {
                PayloadBlockState::FullyPresent => {
                    Some(entry.file_offset_mb * 1024 * 1024)
                }
                _ => None,
            }
        })
    }
}
```

### 4.4 读取实现

```rust
impl<'a> BlockIo for DynamicBlockIo<'a> {
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        let offset_in_block = virtual_offset % self.bat.block_size;
        
        // 3. 检查块状态
        match self.bat.get_payload_entry(block_idx) {
            Some(entry) if entry.state == PayloadBlockState::FullyPresent => {
                // 块已分配，读取数据
                let file_offset = entry.file_offset_mb * 1024 * 1024 + offset_in_block;
                self.file.seek(SeekFrom::Start(file_offset))?;
                
                let bytes_remaining = self.bat.block_size - offset_in_block;
                let to_read = buf.len().min(bytes_remaining as usize);
                
                self.file.read_exact(&mut buf[..to_read])?;
                Ok(to_read)
            }
            _ => {
                // 块未分配，返回零
                buf.fill(0);
                Ok(buf.len())
            }
        }
    }
}
```

### 4.5 写入实现

```rust
impl<'a> BlockIo for DynamicBlockIo<'a> {
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        let offset_in_block = virtual_offset % self.bat.block_size;
        
        // 3. 确保块已分配
        let file_offset = if !self.is_block_allocated(block_idx) {
            self.allocate_block(block_idx)?
        } else {
            self.get_block_offset(block_idx).unwrap()
        };
        
        // 4. 写入数据
        let write_offset = file_offset + offset_in_block;
        self.file.seek(SeekFrom::Start(write_offset))?;
        
        let bytes_remaining = self.bat.block_size - offset_in_block;
        let to_write = buf.len().min(bytes_remaining as usize);
        
        self.file.write_all(&buf[..to_write])?;
        self.file.flush()?;
        
        Ok(to_write)
    }
}
```

### 4.6 性能特点

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 读取（已分配） | O(1) | 直接 BAT 查询 + seek |
| 读取（未分配） | O(1) | BAT 查询，返回零 |
| 写入（已分配） | O(1) | 直接写入 |
| 写入（未分配） | O(1) + 扩展 | 分配 + 写入 |
| 空间 | 按需增长 | 仅分配已写入块 |

**优点**:
- 节省磁盘空间
- 创建速度快
- 适合稀疏数据

**缺点**:
- 随机写入可能导致碎片化
- 需要维护 BAT

---

## 5. DifferencingBlockIo

**文件**: `src/block_io/differencing.rs`

### 5.1 实现原理

差异磁盘存储与父磁盘的差异。读取时优先检查当前磁盘，如不存在则递归查询父磁盘。

### 5.2 数据结构

```rust
pub struct DifferencingBlockIo<'a> {
    file: &'a mut File,
    bat: &'a mut Bat,
    parent: Option<Box<dyn BlockIo>>,  // 父磁盘 BlockIo
    virtual_disk_size: u64,
    // 内部 DynamicBlockIo 用于处理新块分配
}

impl<'a> DifferencingBlockIo<'a> {
    pub fn new(
        file: &'a mut File,
        bat: &'a mut Bat,
        parent: Option<Box<dyn BlockIo>>,
        virtual_disk_size: u64,
    ) -> Self {
        DifferencingBlockIo {
            file,
            bat,
            parent,
            virtual_disk_size,
        }
    }
    
    /// 检查块是否存在于本地
    fn has_local_block(&self, block_idx: u64) -> bool {
        match self.bat.get_payload_entry(block_idx) {
            Some(entry) => matches!(entry.state, 
                PayloadBlockState::FullyPresent | 
                PayloadBlockState::PartiallyPresent
            ),
            None => false,
        }
    }
    
    /// 从父磁盘读取
    fn read_from_parent(
        &mut self, 
        virtual_offset: u64, 
        buf: &mut [u8]
    ) -> Result<usize> {
        match &mut self.parent {
            Some(parent) => parent.read(virtual_offset, buf),
            None => {
                // 无父磁盘，返回零
                buf.fill(0);
                Ok(buf.len())
            }
        }
    }
}
```

### 5.3 读取实现

```rust
impl<'a> BlockIo for DifferencingBlockIo<'a> {
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        let offset_in_block = virtual_offset % self.bat.block_size;
        
        // 3. 检查本地块
        match self.bat.get_payload_entry(block_idx) {
            Some(entry) => match entry.state {
                PayloadBlockState::FullyPresent => {
                    // 完全存在于本地，直接读取
                    let file_offset = entry.file_offset_mb * 1024 * 1024 + offset_in_block;
                    self.file.seek(SeekFrom::Start(file_offset))?;
                    
                    let bytes_remaining = self.bat.block_size - offset_in_block;
                    let to_read = buf.len().min(bytes_remaining as usize);
                    
                    self.file.read_exact(&mut buf[..to_read])?;
                    Ok(to_read)
                }
                PayloadBlockState::PartiallyPresent => {
                    // 部分存在，需要检查 Sector Bitmap
                    // 简化实现：读取整个块然后合并
                    self.read_partial(virtual_offset, buf, entry)
                }
                _ => {
                    // 不存在于本地，查询父磁盘
                    self.read_from_parent(virtual_offset, buf)
                }
            }
            None => {
                // BAT 中无条目，查询父磁盘
                self.read_from_parent(virtual_offset, buf)
            }
        }
    }
}
```

### 5.4 写入实现

写入差异磁盘时，如果块不存在于本地，需要先分配（Copy-on-Write）：

```rust
impl<'a> BlockIo for DifferencingBlockIo<'a> {
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 计算块索引
        let block_idx = virtual_offset / self.bat.block_size;
        
        // 3. Copy-on-Write: 如果块不存在，从父磁盘复制
        if !self.has_local_block(block_idx) {
            self.copy_block_from_parent(block_idx)?;
        }
        
        // 4. 现在块一定存在，执行写入
        let entry = self.bat.get_payload_entry(block_idx).unwrap();
        let file_offset = entry.file_offset_mb * 1024 * 1024 
            + (virtual_offset % self.bat.block_size);
        
        self.file.seek(SeekFrom::Start(file_offset))?;
        
        let bytes_remaining = self.bat.block_size - (virtual_offset % self.bat.block_size);
        let to_write = buf.len().min(bytes_remaining as usize);
        
        self.file.write_all(&buf[..to_write])?;
        self.file.flush()?;
        
        // 5. 更新 Sector Bitmap（如需要）
        self.update_sector_bitmap(block_idx, virtual_offset, to_write)?;
        
        Ok(to_write)
    }
    
    /// Copy-on-Write：从父磁盘复制整个块
    fn copy_block_from_parent(&mut self, block_idx: u64) -> Result<()> {
        // 1. 分配新块
        let block_offset = self.allocate_block(block_idx)?;
        
        // 2. 从父磁盘读取整个块
        let virtual_offset = block_idx * self.bat.block_size;
        let mut block_data = vec![0u8; self.bat.block_size as usize];
        
        if let Some(parent) = &mut self.parent {
            parent.read(virtual_offset, &mut block_data)?;
        }
        
        // 3. 写入本地块
        self.file.seek(SeekFrom::Start(block_offset))?;
        self.file.write_all(&block_data)?;
        
        // 4. 初始化 Sector Bitmap（全部置 1）
        self.initialize_sector_bitmap(block_idx)?;
        
        Ok(())
    }
}
```

### 5.5 Sector Bitmap 处理

Sector Bitmap 用于跟踪块内哪些扇区已被修改：

```rust
impl DifferencingBlockIo<'_> {
    /// 初始化 Sector Bitmap（全部已写入）
    fn initialize_sector_bitmap(&mut self, chunk_idx: u64) -> Result<()> {
        let sb_entry = self.bat.get_sector_bitmap_entry(chunk_idx)
            .ok_or(VhdxError::InvalidBatEntry)?;
        
        if sb_entry.state != PayloadBlockState::FullyPresent {
            // 分配 Sector Bitmap 块
            // 每个位对应一个扇区，1 = 已写入
            let num_sectors = self.bat.chunk_size / self.bat.logical_sector_size as u64;
            let bitmap_size = (num_sectors + 7) / 8; // 向上取整到字节
            let bitmap = vec![0xFFu8; bitmap_size as usize];
            
            // 写入到文件...
        }
        
        Ok(())
    }
    
    /// 更新 Sector Bitmap
    fn update_sector_bitmap(
        &mut self, 
        block_idx: u64, 
        offset: u64, 
        length: usize
    ) -> Result<()> {
        let chunk_idx = block_idx / self.bat.chunk_ratio;
        let sector_idx = offset / self.bat.logical_sector_size as u64;
        let num_sectors = (length + self.bat.logical_sector_size as usize - 1) 
            / self.bat.logical_sector_size as usize;
        
        // 设置 Sector Bitmap 中的对应位...
        
        Ok(())
    }
}
```

### 5.6 性能特点

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 读取（本地存在） | O(1) | 直接读取 |
| 读取（父磁盘） | O(depth) | 递归查询父链 |
| 写入（CoW） | O(block_size) | 复制整个块 |
| 空间 | 增量 | 仅存储差异 |

**优点**:
- 节省空间（仅存储差异）
- 支持快照链
- 快速创建

**缺点**:
- 读取可能需要查询父链
- 首次写入有 CoW 开销
- 深层链影响性能

---

## 6. BlockCache

**文件**: `src/block_io/cache.rs`

### 6.1 设计目标

- 缓存最近访问的数据块，减少重复 I/O
- LRU 淘汰策略
- 可选功能（非必需）

### 6.2 数据结构

```rust
use std::collections::HashMap;
use std::collections::VecDeque;

pub struct BlockCache {
    /// 缓存数据: block_idx -> block_data
    cache: HashMap<u64, Vec<u8>>,
    /// LRU 顺序: 最近使用在尾部
    lru_order: VecDeque<u64>,
    /// 最大缓存块数
    capacity: usize,
    /// 块大小
    block_size: u32,
}

impl BlockCache {
    pub fn new(capacity: usize, block_size: u32) -> Self {
        BlockCache {
            cache: HashMap::with_capacity(capacity),
            lru_order: VecDeque::with_capacity(capacity),
            capacity,
            block_size,
        }
    }
    
    /// 获取缓存块
    pub fn get(&mut self, block_idx: u64) -> Option<&Vec<u8>> {
        if self.cache.contains_key(&block_idx) {
            // 更新 LRU 顺序
            self.lru_order.retain(|&x| x != block_idx);
            self.lru_order.push_back(block_idx);
            self.cache.get(&block_idx)
        } else {
            None
        }
    }
    
    /// 插入缓存块
    pub fn put(&mut self, block_idx: u64, data: Vec<u8>) {
        if self.cache.len() >= self.capacity {
            // LRU 淘汰
            if let Some(oldest) = self.lru_order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        
        self.lru_order.push_back(block_idx);
        self.cache.insert(block_idx, data);
    }
    
    /// 使缓存项失效
    pub fn invalidate(&mut self, block_idx: u64) {
        self.lru_order.retain(|&x| x != block_idx);
        self.cache.remove(&block_idx);
    }
    
    /// 清空缓存
    pub fn clear(&mut self) {
        self.cache.clear();
        self.lru_order.clear();
    }
}
```

### 6.3 与 BlockIo 集成

缓存可以包装在 BlockIo 实现外部：

```rust
pub struct CachedBlockIo<'a> {
    inner: Box<dyn BlockIo>,
    cache: BlockCache,
}

impl<'a> BlockIo for CachedBlockIo<'a> {
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        let block_idx = virtual_offset / self.block_size() as u64;
        let offset_in_block = virtual_offset % self.block_size() as u64;
        
        // 检查缓存
        if let Some(cached) = self.cache.get(block_idx) {
            let start = offset_in_block as usize;
            let end = (offset_in_block as usize + buf.len()).min(cached.len());
            let to_copy = end - start;
            buf[..to_copy].copy_from_slice(&cached[start..end]);
            return Ok(to_copy);
        }
        
        // 缓存未命中，读取整个块
        let mut block_data = vec![0u8; self.block_size() as usize];
        self.inner.read(virtual_offset - offset_in_block, &mut block_data)?;
        
        // 填充请求
        let start = offset_in_block as usize;
        let to_copy = buf.len().min(block_data.len() - start);
        buf[..to_copy].copy_from_slice(&block_data[start..start + to_copy]);
        
        // 存入缓存
        self.cache.put(block_idx, block_data);
        
        Ok(to_copy)
    }
    
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        // 写入数据
        let result = self.inner.write(virtual_offset, buf)?;
        
        // 使对应缓存块失效
        let block_idx = virtual_offset / self.block_size() as u64;
        self.cache.invalidate(block_idx);
        
        Ok(result)
    }
    
    // ... other methods
}
```

---

## 7. 使用示例

### 7.1 直接使用 BlockIo

```rust
use vhdx_rs::block_io::{FixedBlockIo, DynamicBlockIo};
use vhdx_rs::bat::Bat;
use std::fs::File;

// 打开 VHDX 文件并创建 BlockIo
let mut file = File::open("disk.vhdx")?;
let bat = Bat::from_bytes(&bat_data, virtual_disk_size, block_size, sector_size)?;

// 根据磁盘类型创建对应实现
let mut block_io = match disk_type {
    DiskType::Fixed => FixedBlockIo::new(&mut file, &bat, virtual_disk_size),
    DiskType::Dynamic => DynamicBlockIo::new(&mut file, &mut bat, virtual_disk_size),
    // ...
};

// 读取数据
let mut buffer = vec![0u8; 4096];
block_io.read(0, &mut buffer)?;

// 写入数据
block_io.write(0, &buffer)?;
```

### 7.2 跨块读取

```rust
/// 读取可能跨越多个块的数据
fn read_spanning_blocks(
    block_io: &mut dyn BlockIo,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(length);
    let mut current_offset = offset;
    let mut remaining = length;
    
    while remaining > 0 {
        let mut buf = vec![0u8; remaining.min(4096)];
        let bytes_read = block_io.read(current_offset, &mut buf)?;
        
        if bytes_read == 0 {
            break;
        }
        
        result.extend_from_slice(&buf[..bytes_read]);
        current_offset += bytes_read as u64;
        remaining -= bytes_read;
    }
    
    Ok(result)
}
```

---

## 8. 性能优化建议

1. **顺序读写**: 尽可能按顺序访问块，减少 seek 次数
2. **批量操作**: 合并小写入为块级别的写入
3. **缓存策略**: 对于随机读取频繁的场景启用 BlockCache
4. **差异链深度**: 限制差异磁盘链深度（建议不超过 3-5 层）
5. **块大小选择**: 
   - 小文件/随机 I/O：1MB 块
   - 大文件/顺序 I/O：32MB 或更大块

---

## 9. 参考文档

- [01-architecture-overview.md](./01-architecture-overview.md) - 架构概述
- [02-core-modules.md](./02-core-modules.md) - 核心模块
- [04-file-operations.md](./04-file-operations.md) - 文件操作
