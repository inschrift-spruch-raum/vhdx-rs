# linkfs - VHDX (Virtual Hard Disk v2) Library for Rust

纯 Rust 实现的 VHDX 虚拟磁盘文件格式库，完整支持 Microsoft MS-VHDX 规范。

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
linkfs = { path = "path/to/linkfs" }
```

### 示例代码

```rust
use linkfs::{VhdxFile, VhdxBuilder, DiskType};
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
use linkfs::{VhdxBuilder, DiskType};
use std::path::Path;

// 创建动态 VHDX (默认)
VhdxBuilder::new()
    .with_size(10 * 1024 * 1024 * 1024) // 10GB
    .with_type(DiskType::Dynamic)
    .create(Path::new("dynamic.vhdx"))?;

// 创建固定 VHDX
VhdxBuilder::new()
    .with_size(10 * 1024 * 1024 * 1024)
    .with_type(DiskType::Fixed)
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
pub fn new() -> Self;
pub fn with_size(self, size: u64) -> Self;
pub fn with_type(self, disk_type: DiskType) -> Self;
pub fn with_block_size(self, block_size: u32) -> Self;
pub fn with_logical_sector_size(self, size: u32) -> Self;
pub fn with_physical_sector_size(self, size: u32) -> Self;
pub fn create(self, path: &Path) -> Result<VhdxFile>;
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
├── lib.rs       # 库入口，导出公共 API
├── main.rs      # CLI 工具 (vhdx-tool)
├── error.rs     # 错误类型定义
├── guid.rs      # GUID 处理
├── crc32c.rs    # CRC-32C 校验 (Castagnoli)
├── header.rs    # VHDX Header 结构
├── region.rs    # Region Table 解析
├── metadata.rs  # Metadata Region 读取
├── bat.rs       # Block Allocation Table
├── log.rs       # Log 系统 (LogReplayer + LogWriter)
├── block.rs     # 块级 I/O (BlockIo + FixedBlockIo)
└── vhdx.rs      # 主 VHDX 文件操作
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
