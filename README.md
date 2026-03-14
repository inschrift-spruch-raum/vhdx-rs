# vhdx-rs - VHDX (Virtual Hard Disk v2) Library for Rust

纯 Rust 实现的 VHDX 虚拟磁盘文件格式库，完整支持 Microsoft MS-VHDX 规范。
> 📋 **[MS-VHDX v20240423 标准合规实现总结](./docs/VHDX-Compliance-Summary.md)** - 查看详细的合规性实现报告


## 功能特性

- ✅ **完整的 VHDX 格式支持**
  - 固定磁盘 (Fixed)
  - 动态磁盘 (Dynamic)
  - 差异磁盘 (Differencing)

- ✅ **崩溃一致性保证**
  - Log 日志系统
  - 日志重放 (Log Replay)
  - 双头安全机制

- ✅ **核心组件**
  - Header 解析与验证
  - Region Table 解析
  - Metadata Region 读取
  - BAT (Block Allocation Table) 管理
  - 块级读写操作

- ✅ **CLI 工具** - `vhdx-tool` 命令行工具

## 快速开始

### 添加依赖

```toml
[dependencies]
vhdx-rs = { path = "path/to/vhdx-rs" }
```

### 示例代码

```rust
use vhdx_rs::{VhdxFile, VhdxBuilder, DiskType};
use std::path::Path;

// 打开现有 VHDX 文件
let mut vhdx = VhdxFile::open(Path::new("disk.vhdx"), true)?;

// 读取数据
let mut buffer = vec![0u8; 4096];
let bytes_read = vhdx.read(0, &mut buffer)?;

// 写入数据
vhdx.write(0, &buffer)?;

// 获取磁盘信息
println!("虚拟磁盘大小: {} bytes", vhdx.virtual_disk_size());
println!("块大小: {} bytes", vhdx.block_size());
println!("磁盘类型: {:?}", vhdx.disk_type());
```

### 创建新 VHDX 文件

```rust
use vhdx_rs::{VhdxBuilder, DiskType};
use std::path::Path;

// 创建动态 VHDX (默认)
VhdxBuilder::new(10 * 1024 * 1024 * 1024) // 10GB
    .disk_type(DiskType::Dynamic)
    .create(Path::new("dynamic.vhdx"))?;

// 创建固定 VHDX
VhdxBuilder::new(10 * 1024 * 1024 * 1024)
    .disk_type(DiskType::Fixed)
    .create(Path::new("fixed.vhdx"))?;
```

## CLI 工具使用

### 安装

```bash
cargo build --release
# 可执行文件在 target/release/vhdx-tool.exe
```

### 查看 VHDX 信息

```bash
vhdx-tool info disk.vhdx
```

输出示例：
```
VHDX File: disk.vhdx
============================
Virtual Disk Size: 10737418240 bytes (10.00 GB)
Block Size: 33554432 bytes (32.00 MB)
Logical Sector Size: 512 bytes
Physical Sector Size: 4096 bytes
Disk Type: Dynamic
Virtual Disk ID: {xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}
```

### 检查 VHDX 完整性

```bash
vhdx-tool check disk.vhdx
```

### 读取数据

```bash
# 读取 1024 字节到 stdout
vhdx-tool read disk.vhdx --offset 0 --length 1024

# 读取到文件
vhdx-tool read disk.vhdx --offset 0 --length 4096 --output data.bin
```

### 创建 VHDX

```bash
# 创建 10GB 动态 VHDX
vhdx-tool create disk.vhdx --size 10G --type dynamic

# 创建 10GB 固定 VHDX
vhdx-tool create disk.vhdx --size 10G --type fixed
```

## 核心 API

### VhdxFile

主 VHDX 文件操作结构。

```rust
// 打开 VHDX 文件 (只读或读写)
pub fn open(path: &Path, read_only: bool) -> Result<Self>;

// 读取数据
pub fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize>;

// 写入数据
pub fn write(&mut self, offset: u64, buf: &[u8]) -> Result<()>;

// 获取虚拟磁盘大小
pub fn virtual_disk_size(&self) -> u64;

// 获取块大小
pub fn block_size(&self) -> u32;

// 获取磁盘类型
pub fn disk_type(&self) -> DiskType;

// 检查是否有父磁盘 (差异磁盘)
pub fn has_parent(&self) -> bool;
```

### VhdxBuilder

用于创建新的 VHDX 文件。

```rust
pub fn new(virtual_disk_size: u64) -> Self;
pub fn block_size(self, size: u32) -> Self;
pub fn sector_sizes(self, logical: u32, physical: u32) -> Self;
pub fn disk_type(self, disk_type: DiskType) -> Self;
pub fn parent_path<P: Into<String>>(self, path: P) -> Self;
pub fn create<P: AsRef<Path>>(self, path: P) -> Result<VhdxFile>;
```

### DiskType

```rust
pub enum DiskType {
    Fixed,        // 固定大小
    Dynamic,      // 动态增长
    Differencing, // 差异磁盘
}
```

## 项目结构

```
src/
├── lib.rs              # 库入口，导出公共 API
├── main.rs             # CLI 工具 (vhdx-tool)
├── error.rs            # 错误类型定义
├── common/             # 通用工具
│   ├── guid.rs         # 128-bit GUID 处理
│   ├── crc32c.rs       # CRC-32C 校验 (Castagnoli)
│   └── disk_type.rs    # 磁盘类型枚举
├── header/             # Header Section
│   ├── file_type.rs    # File Type Identifier ("vhdxfile" 签名)
│   ├── header.rs       # VhdxHeader (双头安全机制)
│   └── region_table.rs # Region Table (BAT/Metadata 位置)
├── bat/                # Block Allocation Table
│   ├── entry.rs        # BatEntry (64位: State + FileOffsetMB)
│   ├── states.rs       # PayloadBlockState, SectorBitmapState 枚举
│   └── table.rs        # Bat 结构，Chunk Ratio 计算
├── log/                # Log 系统
│   ├── entry.rs        # LogEntryHeader, LogSequence
│   ├── descriptor.rs   # ZeroDescriptor, DataDescriptor
│   ├── sector.rs       # DataSector
│   ├── replayer.rs     # LogReplayer (崩溃恢复)
│   └── writer.rs       # LogWriter
├── metadata/           # Metadata Region
│   ├── region.rs       # MetadataRegion 容器
│   ├── table.rs        # MetadataTable 头结构
│   ├── file_parameters.rs  # FileParameters (块大小、是否有父)
│   ├── disk_size.rs    # VirtualDiskSize
│   ├── disk_id.rs      # VirtualDiskId (GUID)
│   ├── sector_size.rs  # LogicalSectorSize, PhysicalSectorSize
│   └── parent_locator.rs   # ParentLocator (差异磁盘)
├── payload/            # Payload Blocks
│   ├── bitmap.rs       # SectorBitmap 操作
│   └── chunk.rs        # Chunk 计算 (2^23 * SectorSize / BlockSize)
├── block_io/           # 块级 I/O
│   ├── traits.rs       # BlockIo trait
│   ├── fixed.rs        # FixedBlockIo (固定磁盘)
│   ├── dynamic.rs      # DynamicBlockIo (动态磁盘)
│   ├── differencing.rs # DifferencingBlockIo (差异磁盘)
│   └── cache.rs        # BlockCache
├── file/               # VHDX 文件操作
│   ├── vhdx_file.rs    # VhdxFile 结构 (打开、读取、写入等)
│   └── builder.rs      # VhdxBuilder (创建 VHDX 文件)
└── utils/              # 工具函数
    └── mod.rs
tests/
└── integration/        # 集成测试
    └── full_workflow.rs
```

## 技术规范

本实现基于 Microsoft MS-VHDX 规范：

- **文档**: `misc/MS-VHDX.md`
- **版本**: v20240423 (8.0)
- **规范 URL**: https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-vhdx/

### 关键特性实现

| 特性 | 状态 |
|------|------|
| File Type Identifier | ✅ |
| Dual Headers with CRC-32C | ✅ |
| Region Table | ✅ |
| Metadata Region | ✅ |
| BAT (Block Allocation Table) | ✅ |
| Log Replay (Crash Recovery) | ✅ |
| Log Writer (Atomic Updates) | ✅ |
| Fixed Disk Support | ✅ |
| Dynamic Disk Support | ✅ |
| Differencing Disk Support | ✅ (基础支持) |
| Sector Bitmap | ✅ (基础支持) |

## 测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_header
cargo test test_bat
cargo test test_vhdx_read_write
```

## 平台支持

- ✅ Windows
- ✅ Linux
- ✅ macOS

## 许可证

MIT

## 注意事项

1. **字节序**: VHDX 使用小端序 (Little-Endian)
2. **对齐**: 所有结构必须 1MB 对齐
3. **CRC**: 使用 CRC-32C (Castagnoli 多项式 0x1EDC6F41)
4. **日志**: 写入操作自动使用日志保证原子性
