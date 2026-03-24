# 重构：复杂函数分解

## 概述

将过于复杂的函数分解为更小、更专注且可测试的单元。这可以提高代码的可读性、可维护性和可测试性，同时使代码库更易于理解和修改。

## 当前状态

### 目标函数

#### 1. VhdxFile::open() - 149 行，12 个职责
**位置**：`src/file/vhdx_file.rs:64-212`

**当前职责**：
1. 文件打开（第 66-73 行）
2. 文件类型读取（第 76-78 行）
3. 头部读取/验证（第 81-87 行）
4. 区域表读取（第 90 行）
5. 日志重放（第 93-95 行）
6. 元数据读取（第 98-105 行）
7. BAT 读取（第 108-132 行）
8. 磁盘类型检测（第 135-150 行）
9. 父级加载（第 153-174 行）
10. VhdxFile 构建（第 176-194 行）
11. LogWriter 初始化（第 197-204 行）
12. 头部 GUID 更新（第 207-209 行）

#### 2. VhdxBuilder::create() - 335 行，10 个职责
**位置**：`src/file/builder.rs:82-416`

**当前职责**：
1. 参数验证（第 86-115 行）
2. GUID 生成（第 121-123 行）
3. 大小计算（第 126-151 行）
4. 文件类型写入（第 154-155 行）
5. 头部创建/写入（第 158-184 行）
6. 区域表创建（第 186-235 行）
7. BAT 创建/写入（第 238-294 行）
8. 元数据创建（第 296-398 行）
9. 固定磁盘分配（第 401-406 行）
10. 文件重新打开（第 415 行）

## 建议方案

### 重构策略

#### 方法：提取方法 + 构建器模式

将每个大函数分解为：
1. **协调器方法**：协调工作流
2. **辅助方法**：每个方法具有单一职责
3. **构建器结构体**：用于复杂构造任务

### VhdxFile::open() 重构

#### 新结构

```rust
impl VhdxFile {
    /// 打开一个现有的 VHDX 文件
    pub fn open<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        // 阶段 1：打开文件并读取静态数据
        let mut file = Self::open_file(&path, read_only)?;
        let file_type = Self::read_file_type(&mut file)?;
        
        // 阶段 2：读取并验证头部
        let header = Self::read_and_validate_headers(&mut file)?;
        let sequence_number = header.sequence_number;
        
        // 阶段 3：如有需要则重放日志
        Self::replay_log_if_needed(&mut file, &header, read_only)?;
        
        // 阶段 4：读取元数据区域
        let region_table = Self::read_region_tables(&mut file)?;
        let metadata = Self::read_metadata_region(&mut file, &region_table)?;
        let bat = Self::read_bat(&mut file, &region_table, &metadata)?;
        
        // 阶段 5：确定磁盘特性
        let disk_type = Self::detect_disk_type(&bat, &metadata)?;
        let parent = Self::load_parent_if_needed(&path, disk_type, &metadata)?;
        
        // 阶段 6：初始化文件句柄
        let mut vhdx = Self::initialize(
            file, path, file_type, header, region_table,
            metadata, bat, disk_type, sequence_number, 
            read_only, parent
        )?;
        
        // 阶段 7：初始化后设置
        vhdx.initialize_log_writer()?;
        if !read_only {
            vhdx.update_header_guids()?;
        }
        
        Ok(vhdx)
    }
    
    // 辅助方法...
}
```

#### 提取的方法

```rust
impl VhdxFile {
    /// 打开底层文件
    fn open_file(path: &Path, read_only: bool) -> Result<File> {
        if read_only {
            File::open(path)
        } else {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
        }.map_err(|e| e.into())
    }
    
    /// 读取文件类型标识符
    fn read_file_type(file: &mut File) -> Result<FileTypeIdentifier> {
        let mut data = vec![0u8; FileTypeIdentifier::SIZE];
        file.read_exact(&mut data)?;
        FileTypeIdentifier::from_bytes(&data)
    }
    
    /// 读取并验证两个头部
    fn read_and_validate_headers(file: &mut File) -> Result<VhdxHeader> {
        let (_idx, header, _) = read_headers(file)?;
        header.check_version()?;
        Ok(header)
    }
    
    /// 如果头部指示需要，则重放日志
    fn replay_log_if_needed(
        file: &mut File, 
        header: &VhdxHeader,
        read_only: bool
    ) -> Result<()> {
        if header.log_guid.is_nil() {
            return Ok(());
        }
        Self::replay_log(file, header, read_only)
    }
    
    /// 读取区域表
    fn read_region_tables(file: &mut File) -> Result<RegionTable> {
        let (table, _) = read_region_tables(file)?;
        Ok(table)
    }
    
    /// 读取元数据区域
    fn read_metadata_region(
        file: &mut File,
        region_table: &RegionTable
    ) -> Result<MetadataRegion> {
        let entry = region_table
            .find_metadata()
            .ok_or_else(|| VhdxError::RequiredRegionNotFound("Metadata".to_string()))?;
        
        let mut data = vec![0u8; entry.length as usize];
        file.seek(SeekFrom::Start(entry.file_offset))?;
        file.read_exact(&mut data)?;
        
        MetadataRegion::from_bytes(&data)
    }
    
    /// 读取 BAT
    fn read_bat(
        file: &mut File,
        region_table: &RegionTable,
        metadata: &MetadataRegion
    ) -> Result<Bat> {
        let entry = region_table
            .find_bat()
            .ok_or_else(|| VhdxError::RequiredRegionNotFound("BAT".to_string()))?;
        
        let mut data = vec![0u8; entry.length as usize];
        file.seek(SeekFrom::Start(entry.file_offset))?;
        file.read_exact(&mut data)?;
        
        let file_params = metadata.file_parameters()?;
        let virtual_disk_size = metadata.virtual_disk_size()?.size;
        let logical_sector_size = metadata.logical_sector_size()?.size;
        
        let mut bat = Bat::from_bytes(
            &data,
            virtual_disk_size,
            file_params.block_size as u64,
            logical_sector_size,
        )?;
        
        bat.set_bat_file_offset(entry.file_offset);
        Ok(bat)
    }
    
    /// 基于 BAT 条目和元数据检测磁盘类型
    fn detect_disk_type(bat: &Bat, metadata: &MetadataRegion) -> Result<DiskType> {
        let file_params = metadata.file_parameters()?;
        
        if file_params.has_parent {
            return Ok(DiskType::Differencing);
        }
        
        // 检查第一个有效负载块以确定固定与动态
        if let Some(first_entry) = bat.get_payload_entry(0) {
            if first_entry.state == PayloadBlockState::FullyPresent {
                Ok(DiskType::Fixed)
            } else {
                Ok(DiskType::Dynamic)
            }
        } else {
            Ok(DiskType::Dynamic)
        }
    }
    
    /// 为差异磁盘加载父磁盘
    fn load_parent_if_needed(
        path: &Path,
        disk_type: DiskType,
        metadata: &MetadataRegion
    ) -> Result<Option<Box<VhdxFile>>> {
        if disk_type != DiskType::Differencing {
            return Ok(None);
        }
        
        let locator = metadata.parent_locator()
            .map_err(|_| VhdxError::InvalidMetadata("Missing parent locator".to_string()))?;
        
        let parent_path = locator.parent_path()
            .ok_or_else(|| VhdxError::InvalidMetadata("Empty parent path".to_string()))?;
        
        let parent_full_path = if Path::new(parent_path).is_absolute() {
            PathBuf::from(parent_path)
        } else {
            path.parent()
                .map(|p| p.join(parent_path))
                .unwrap_or_else(|| PathBuf::from(parent_path))
        };
        
        Ok(Some(Box::new(Self::open(parent_full_path, true)?)))
    }
    
    /// 初始化
