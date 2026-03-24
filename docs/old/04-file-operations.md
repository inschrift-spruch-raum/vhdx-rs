# 文件操作与 Builder 模式

本文档详细介绍 vhdx-rs 旧版的高级文件操作，包括 VhdxFile 结构、Builder 模式创建文件、以及文件生命周期管理。

---

## 1. VhdxFile 结构

**文件**: `src/file/vhdx_file.rs`

### 1.1 结构定义

```rust
pub struct VhdxFile {
    /// 底层文件句柄
    pub(crate) file: File,
    /// 文件路径
    pub(crate) path: PathBuf,
    /// 文件类型标识符
    pub(crate) file_type: FileTypeIdentifier,
    /// 当前 Header（索引 0 或 1）
    pub(crate) header: VhdxHeader,
    /// Region Table
    pub(crate) region_table: RegionTable,
    /// Metadata Region
    pub(crate) metadata: MetadataRegion,
    /// Block Allocation Table
    pub(crate) bat: Bat,
    /// 磁盘类型
    pub(crate) disk_type: DiskType,
    /// 虚拟磁盘大小
    pub(crate) virtual_disk_size: u64,
    /// 块大小
    pub(crate) block_size: u32,
    /// 逻辑扇区大小
    pub(crate) logical_sector_size: u32,
    /// 物理扇区大小
    pub(crate) physical_sector_size: u32,
    /// 虚拟磁盘 ID
    pub(crate) virtual_disk_id: Guid,
    /// 当前序列号（用于 Header 更新）
    pub(crate) sequence_number: u64,
    /// 是否只读
    pub(crate) read_only: bool,
    /// 父文件（差异磁盘）
    pub(crate) parent: Option<Box<VhdxFile>>,
    /// 日志写入器
    pub(crate) log_writer: Option<LogWriter>,
}
```

### 1.2 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `file` | `File` | 底层 std::fs::File，所有 I/O 操作的基础 |
| `path` | `PathBuf` | 文件路径，用于差异磁盘解析相对路径 |
| `file_type` | `FileTypeIdentifier` | 文件签名和创建者信息 |
| `header` | `VhdxHeader` | 当前活动的 Header（已通过校验） |
| `region_table` | `RegionTable` | BAT 和 Metadata 的位置信息 |
| `metadata` | `MetadataRegion` | 磁盘参数（大小、扇区等） |
| `bat` | `Bat` | 块分配表，虚拟偏移到文件偏移的映射 |
| `disk_type` | `DiskType` | Fixed/Dynamic/Differencing |
| `parent` | `Option<Box<VhdxFile>>` | 差异磁盘的父磁盘，递归链 |
| `log_writer` | `Option<LogWriter>` | 日志写入器，用于原子更新 |

---

## 2. 打开文件

### 2.1 VhdxFile::open 方法

```rust
impl VhdxFile {
    /// 打开现有 VHDX 文件
    /// 
    /// # Arguments
    /// * `path` - 文件路径
    /// * `read_only` - 是否以只读模式打开
    /// 
    /// # Returns
    /// * `Ok(VhdxFile)` - 成功打开的文件句柄
    /// * `Err(VhdxError)` - 打开失败（文件损坏、格式错误等）
    pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
        // 实现见下文
    }
}
```

### 2.2 打开流程详解

```rust
pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
    let path = path.as_ref().to_path_buf();
    
    // 1. 打开底层文件
    let mut file = if read_only {
        File::open(&path)?
    } else {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?
    };
    
    // 2. 读取文件类型标识符 (0-64KB)
    let mut ft_data = vec![0u8; FileTypeIdentifier::SIZE];
    file.read_exact(&mut ft_data)?;
    let file_type = FileTypeIdentifier::from_bytes(&ft_data)?;
    
    // 3. 读取 Headers（双头选择）
    let (_header_idx, mut header, _) = read_headers(&mut file)?;
    let sequence_number = header.sequence_number;
    
    // 4. 验证版本
    header.check_version()?;  // version 必须为 1，log_version 为 0
    
    // 5. 读取 Region Tables
    let (region_table, _) = read_region_tables(&mut file)?;
    
    // 6. 重放日志（如有需要）
    if !header.log_guid.is_nil() {
        Self::replay_log(&mut file, &mut header, read_only)?;
    }
    
    // 7. 读取 Metadata
    let metadata_entry = region_table
        .find_metadata()
        .ok_or_else(|| VhdxError::RequiredRegionNotFound("Metadata".to_string()))?;
    
    let mut metadata_data = vec![0u8; metadata_entry.length as usize];
    file.seek(SeekFrom::Start(metadata_entry.file_offset))?;
    file.read_exact(&mut metadata_data)?;
    let metadata = MetadataRegion::from_bytes(&metadata_data)?;
    
    // 8. 读取 BAT
    let bat_entry = region_table
        .find_bat()
        .ok_or_else(|| VhdxError::RequiredRegionNotFound("BAT".to_string()))?;
    
    let mut bat_data = vec![0u8; bat_entry.length as usize];
    file.seek(SeekFrom::Start(bat_entry.file_offset))?;
    file.read_exact(&mut bat_data)?;
    
    // 9. 解析元数据
    let file_params = metadata.file_parameters()?;
    let virtual_disk_size = metadata.virtual_disk_size()?.size;
    let logical_sector_size = metadata.logical_sector_size()?.size;
    let physical_sector_size = metadata.physical_sector_size()?.size;
    let virtual_disk_id = metadata.virtual_disk_id()?.guid;
    
    // 10. 解析 BAT
    let mut bat = Bat::from_bytes(
        &bat_data,
        virtual_disk_size,
        file_params.block_size as u64,
        logical_sector_size,
    )?;
    bat.set_bat_file_offset(bat_entry.file_offset);
    
    // 11. 确定磁盘类型
    let disk_type = if file_params.has_parent {
        DiskType::Differencing
    } else {
        // 根据第一个 BAT Entry 判断 Fixed/Dynamic
        if let Some(first_entry) = bat.get_payload_entry(0) {
            if first_entry.state == PayloadBlockState::FullyPresent {
                DiskType::Fixed
            } else {
                DiskType::Dynamic
            }
        } else {
            DiskType::Dynamic
        }
    };
    
    // 12. 加载父磁盘（差异磁盘）
    let parent = if disk_type == DiskType::Differencing {
        if let Ok(locator) = metadata.parent_locator() {
            if let Some(parent_path) = locator.parent_path() {
                // 解析相对路径
                let parent_full_path = if Path::new(parent_path).is_absolute() {
                    PathBuf::from(parent_path)
                } else {
                    path.parent()
                        .map(|p| p.join(parent_path))
                        .unwrap_or_else(|| PathBuf::from(parent_path))
                };
                Some(Box::new(Self::open(parent_full_path, true)?))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    
    // 13. 构建 VhdxFile
    let mut vhdx = VhdxFile {
        file,
        path,
        file_type,
        header,
        region_table,
        metadata,
        bat,
        disk_type,
        virtual_disk_size,
        block_size: file_params.block_size,
        logical_sector_size,
        physical_sector_size,
        virtual_disk_id,
        sequence_number,
        read_only,
        parent,
        log_writer: None,
    };
    
    // 14. 初始化 LogWriter（非只读模式）
    if !read_only && vhdx.header.log_length > 0 {
        vhdx.log_writer = Some(LogWriter::new(
            vhdx.header.log_offset,
            vhdx.header.log_length,
            vhdx.header.log_guid,
            vhdx.current_file_size()?,
        ));
    }
    
    // 15. 更新 Header GUIDs（可写模式）
    if !read_only {
        vhdx.update_header_guids()?;
    }
    
    Ok(vhdx)
}
```

### 2.3 错误处理

打开文件可能遇到的错误：

| 错误 | 原因 | 处理建议 |
|------|------|----------|
| `InvalidSignature` | 不是有效的 VHDX 文件 | 检查文件是否损坏或被篡改 |
| `InvalidChecksum` | Header 或 Region Table 损坏 | 尝试从另一个 Header 副本恢复 |
| `NoValidHeader` | 两个 Header 都无效 | 文件可能严重损坏 |
| `UnsupportedVersion` | 不支持的 VHDX 版本 | 需要升级库版本 |
| `RequiredRegionNotFound` | 缺少 BAT 或 Metadata | 文件结构不完整 |
| `LogReplayFailed` | 日志重放失败 | 可能需要手动修复 |
| `ParentNotFound` | 差异磁盘的父磁盘不存在 | 检查父磁盘路径 |

---

## 3. 数据读写

### 3.1 读取数据

```rust
impl VhdxFile {
    /// 从虚拟偏移读取数据
    /// 
    /// # Arguments
    /// * `virtual_offset` - 虚拟磁盘偏移（字节）
    /// * `buf` - 接收数据的缓冲区
    /// 
    /// # Returns
    /// 实际读取的字节数（可能少于 buf.len()）
    pub fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize> {
        // 1. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 2. 根据磁盘类型选择 BlockIo 实现
        match self.disk_type {
            DiskType::Fixed => {
                let mut fixed_io = FixedBlockIo::new(
                    &mut self.file, 
                    &self.bat, 
                    self.virtual_disk_size
                );
                fixed_io.read(virtual_offset, buf)
            }
            _ => {
                let mut block_io = DynamicBlockIo::new(
                    &mut self.file,
                    &mut self.bat,
                    self.virtual_disk_size,
                );
                block_io.read(virtual_offset, buf)
            }
        }
    }
}
```

### 3.2 写入数据

```rust
impl VhdxFile {
    /// 写入数据到虚拟偏移
    /// 
    /// # Arguments
    /// * `virtual_offset` - 虚拟磁盘偏移（字节）
    /// * `buf` - 要写入的数据
    /// 
    /// # Returns
    /// 实际写入的字节数
    pub fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize> {
        // 1. 检查只读
        if self.read_only {
            return Err(VhdxError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File is read-only",
            )));
        }
        
        // 2. 边界检查
        if virtual_offset >= self.virtual_disk_size {
            return Err(VhdxError::InvalidOffset(virtual_offset));
        }
        
        // 3. 执行写入
        let result = match self.disk_type {
            DiskType::Fixed => {
                let mut fixed_io = FixedBlockIo::new(
                    &mut self.file,
                    &self.bat,
                    self.virtual_disk_size,
                );
                fixed_io.write(virtual_offset, buf)
            }
            _ => {
                let mut block_io = DynamicBlockIo::new(
                    &mut self.file,
                    &mut self.bat,
                    self.virtual_disk_size,
                );
                
                // 附加 LogWriter 保证原子性
                if let Some(log_writer) = self.log_writer.take() {
                    let result = block_io
                        .with_log_writer(log_writer)
                        .write(virtual_offset, buf);
                    // 注意：简化实现，实际应保留 LogWriter
                    result
                } else {
                    block_io.write(virtual_offset, buf)
                }
            }
        };
        
        // 4. 更新 DataWriteGuid
        if result.is_ok() {
            self.header.data_write_guid = Guid::new_v4();
            self.update_headers()?;
        }
        
        result
    }
}
```

### 3.3 批量读写优化

```rust
impl VhdxFile {
    /// 读取整个虚拟磁盘
    pub fn read_all(&mut self) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(self.virtual_disk_size as usize);
        let mut buffer = vec![0u8; self.block_size as usize];
        
        let mut offset = 0u64;
        while offset < self.virtual_disk_size {
            let to_read = buffer.len().min((self.virtual_disk_size - offset) as usize);
            let bytes_read = self.read(offset, &mut buffer[..to_read])?;
            result.extend_from_slice(&buffer[..bytes_read]);
            offset += bytes_read as u64;
        }
        
        Ok(result)
    }
    
    /// 按块写入（适合初始化）
    pub fn write_blocks(&mut self, data: &[u8]) -> Result<usize> {
        if data.len() as u64 > self.virtual_disk_size {
            return Err(VhdxError::InvalidMetadata("Data too large".to_string()));
        }
        
        let mut written = 0usize;
        let block_size = self.block_size as usize;
        
        for (block_idx, chunk) in data.chunks(block_size).enumerate() {
            let offset = block_idx as u64 * self.block_size as u64;
            written += self.write(offset, chunk)?;
        }
        
        Ok(written)
    }
}
```

---

## 4. VhdxBuilder

**文件**: `src/file/builder.rs`

### 4.1 Builder 模式

使用 Builder 模式创建 VHDX 文件，提供清晰、可配置的 API：

```rust
let vhdx = VhdxBuilder::new(10 * 1024 * 1024 * 1024)  // 10GB
    .disk_type(DiskType::Dynamic)
    .block_size(32 * 1024 * 1024)  // 32MB
    .sector_sizes(512, 4096)
    .create("disk.vhdx")?;
```

### 4.2 结构定义

```rust
pub struct VhdxBuilder {
    virtual_disk_size: u64,      // 虚拟磁盘大小（必需）
    block_size: u32,             // 块大小（默认 32MB）
    logical_sector_size: u32,    // 逻辑扇区（默认 512）
    physical_sector_size: u32,   // 物理扇区（默认 4096）
    disk_type: DiskType,         // 磁盘类型（默认 Dynamic）
    parent_path: Option<String>, // 父磁盘路径（差异磁盘）
    creator: Option<String>,     // 创建者字符串
}
```

### 4.3 Builder 方法

```rust
impl VhdxBuilder {
    /// 创建新的 Builder
    /// 
    /// # Arguments
    /// * `virtual_disk_size` - 虚拟磁盘大小（字节）
    pub fn new(virtual_disk_size: u64) -> Self {
        VhdxBuilder {
            virtual_disk_size,
            block_size: 32 * 1024 * 1024,  // 32MB
            logical_sector_size: 512,
            physical_sector_size: 4096,
            disk_type: DiskType::Dynamic,
            parent_path: None,
            creator: Some("Rust VHDX Library".to_string()),
        }
    }
    
    /// 设置块大小
    /// 
    /// 有效范围：1MB - 256MB，必须是 1MB 的倍数
    pub fn block_size(mut self, size: u32) -> Self {
        self.block_size = size;
        self
    }
    
    /// 设置扇区大小
    /// 
    /// 有效值：512 或 4096
    pub fn sector_sizes(mut self, logical: u32, physical: u32) -> Self {
        self.logical_sector_size = logical;
        self.physical_sector_size = physical;
        self
    }
    
    /// 设置磁盘类型
    pub fn disk_type(mut self, disk_type: DiskType) -> Self {
        self.disk_type = disk_type;
        self
    }
    
    /// 设置父磁盘路径（自动设置为差异磁盘）
    pub fn parent_path<P: Into<String>>(mut self, path: P) -> Self {
        self.parent_path = Some(path.into());
        self.disk_type = DiskType::Differencing;
        self
    }
    
    /// 设置创建者字符串
    pub fn creator(mut self, creator: String) -> Self {
        self.creator = Some(creator);
        self
    }
}
```

### 4.4 创建实现

```rust
impl VhdxBuilder {
    /// 构建并创建 VHDX 文件
    pub fn create<P: AsRef<Path>>(self, path: P) -> Result<VhdxFile> {
        let path = path.as_ref();
        
        // 1. 参数验证
        self.validate()?;
        
        // 2. 创建文件
        let mut file = File::create(path)?;
        
        // 3. 计算布局参数
        let chunk_size = (1u64 << 23) * self.logical_sector_size as u64;
        let chunk_ratio = chunk_size / self.block_size as u64;
        let num_payload_blocks = (self.virtual_disk_size + self.block_size as u64 - 1) 
            / self.block_size as u64;
        let num_sector_bitmap_blocks = (num_payload_blocks + chunk_ratio - 1) / chunk_ratio;
        let num_bat_entries = num_payload_blocks + num_sector_bitmap_blocks;
        
        // 4. 计算文件布局
        let header_size = 1024 * 1024;      // 1MB
        let metadata_size = 1024 * 1024;    // 1MB
        let bat_size = ((num_bat_entries * 8 + 1024 * 1024 - 1) / (1024 * 1024)) * (1024 * 1024);
        
        let metadata_offset = header_size * 2;  // 2MB
        let bat_offset = metadata_offset + metadata_size;  // 3MB
        let data_offset = bat_offset + bat_size;
        
        // 5. 生成 GUIDs
        let file_write_guid = Guid::new_v4();
        let data_write_guid = Guid::new_v4();
        let virtual_disk_id = Guid::new_v4();
        
        // 6. 写入 File Type Identifier
        let file_type = FileTypeIdentifier::new(self.creator.as_deref());
        file.write_all(&file_type.to_bytes())?;
        
        // 7. 写入 Headers
        self.write_headers(&mut file, file_write_guid, data_write_guid)?;
        
        // 8. 写入 Region Tables
        self.write_region_tables(&mut file, bat_offset, bat_size, metadata_offset, metadata_size)?;
        
        // 9. 写入 BAT
        self.write_bat(&mut file, bat_offset, bat_size, data_offset, num_payload_blocks,
            num_sector_bitmap_blocks, chunk_ratio)?;
        
        // 10. 写入 Metadata
        self.write_metadata(&mut file, metadata_offset, virtual_disk_id)?;
        
        // 11. 固定磁盘：分配所有块
        if self.disk_type == DiskType::Fixed {
            let payload_size = num_payload_blocks * self.block_size as u64;
            let payload_data = vec![0u8; payload_size as usize];
            file.seek(SeekFrom::Start(data_offset))?;
            file.write_all(&payload_data)?;
        }
        
        // 12. 刷新并关闭
        file.flush()?;
        drop(file);
        
        // 13. 重新打开文件
        VhdxFile::open(path, false)
    }
}
```

### 4.5 参数验证

```rust
impl VhdxBuilder {
    fn validate(&self) -> Result<()> {
        // 1. 虚拟磁盘大小
        if self.virtual_disk_size == 0 {
            return Err(VhdxError::InvalidMetadata(
                "Virtual disk size cannot be zero".to_string()
            ));
        }
        
        // 2. 块大小范围
        if self.block_size < 1024 * 1024 || self.block_size > 256 * 1024 * 1024 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Block size {} out of range (1MB-256MB)",
                self.block_size
            )));
        }
        
        // 3. 块大小对齐
        if self.block_size % (1024 * 1024) != 0 {
            return Err(VhdxError::InvalidMetadata(
                "Block size must be 1MB aligned".to_string()
            ));
        }
        
        // 4. 扇区大小
        if self.logical_sector_size != 512 && self.logical_sector_size != 4096 {
            return Err(VhdxError::InvalidMetadata(
                "Logical sector size must be 512 or 4096".to_string()
            ));
        }
        if self.physical_sector_size != 512 && self.physical_sector_size != 4096 {
            return Err(VhdxError::InvalidMetadata(
                "Physical sector size must be 512 or 4096".to_string()
            ));
        }
        
        // 5. 差异磁盘父路径
        if self.disk_type == DiskType::Differencing && self.parent_path.is_none() {
            return Err(VhdxError::InvalidMetadata(
                "Differencing disk requires a parent".to_string()
            ));
        }
        
        Ok(())
    }
}
```

### 4.6 创建示例

**创建固定磁盘**:

```rust
use vhdx_rs::{VhdxBuilder, DiskType};

let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)  // 100GB
    .disk_type(DiskType::Fixed)
    .block_size(32 * 1024 * 1024)  // 32MB
    .sector_sizes(512, 4096)
    .creator("MyApp".to_string())
    .create("fixed.vhdx")?;

// 文件大小 = 100GB + 3MB（元数据）
```

**创建动态磁盘**:

```rust
let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)  // 100GB
    .disk_type(DiskType::Dynamic)
    .block_size(32 * 1024 * 1024)
    .create("dynamic.vhdx")?;

// 初始文件大小 ≈ 3MB（仅元数据）
```

**创建差异磁盘**:

```rust
let vhdx = VhdxBuilder::new(100 * 1024 * 1024 * 1024)  // 必须与父磁盘相同大小
    .disk_type(DiskType::Differencing)
    .parent_path("parent.vhdx")
    .create("diff.vhdx")?;

// 初始文件大小 ≈ 3MB（仅元数据）
```

---

## 5. Header 更新

### 5.1 更新 Header GUIDs

```rust
impl VhdxFile {
    /// 更新 Header GUIDs（文件打开时调用）
    fn update_header_guids(&mut self) -> Result<()> {
        self.header.file_write_guid = Guid::new_v4();
        self.sequence_number += 1;
        self.header.sequence_number = self.sequence_number;
        
        self.update_both_headers()?;
        Ok(())
    }
    
    /// 安全更新两个 Headers
    fn update_both_headers(&mut self) -> Result<()> {
        use crate::common::crc32c::crc32c_with_zero_field;
        
        // Header 1（较低序列号 - 视为"旧"）
        let mut header1 = self.header.clone();
        header1.sequence_number = self.sequence_number;
        let mut data1 = header1.to_bytes();
        let checksum1 = crc32c_with_zero_field(&data1, 4, 4);
        LittleEndian::write_u32(&mut data1[4..8], checksum1);
        self.file.seek(SeekFrom::Start(VhdxHeader::OFFSET_1))?;
        self.file.write_all(&data1)?;
        
        // Header 2（较高序列号 - 视为"当前"）
        let mut header2 = self.header.clone();
        header2.sequence_number = self.sequence_number + 1;
        let mut data2 = header2.to_bytes();
        let checksum2 = crc32c_with_zero_field(&data2, 4, 4);
        LittleEndian::write_u32(&mut data2[4..8], checksum2);
        self.file.seek(SeekFrom::Start(VhdxHeader::OFFSET_2))?;
        self.file.write_all(&data2)?;
        
        self.file.flush()?;
        
        // 更新内部状态
        self.sequence_number += 1;
        self.header.sequence_number = self.sequence_number;
        
        Ok(())
    }
}
```

### 5.2 更新策略

```
初始状态: Header1(seq=100), Header2(seq=101) → 使用 Header2

更新过程:
1. 更新 Header1: seq=102（先写"旧"头）
2. 失败安全：如果此时断电，Header1(102) > Header2(101)，使用 Header1

3. 更新 Header2: seq=103（再写"当前"头）
4. 完成：Header1(102), Header2(103) → 使用 Header2
```

---

## 6. 文件关闭

```rust
impl Drop for VhdxFile {
    fn drop(&mut self) {
        // Rust 自动处理文件关闭
        // 如有需要，可添加清理逻辑
    }
}

impl VhdxFile {
    /// 显式关闭并确保所有数据写入
    pub fn close(mut self) -> Result<()> {
        // 1. 刷新文件
        self.file.flush()?;
        
        // 2. 最终更新 Headers
        if !self.read_only {
            self.update_headers()?;
        }
        
        // 3. 文件在 self 被 drop 时自动关闭
        Ok(())
    }
}
```

---

## 7. 参考文档

- [01-architecture-overview.md](./01-architecture-overview.md) - 架构概述
- [02-core-modules.md](./02-core-modules.md) - 核心模块
- [03-block-io.md](./03-block-io.md) - Block I/O
