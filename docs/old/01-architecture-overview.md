# VHDX-RS 旧版架构概述

## 项目简介

vhdx-rs 是一个纯 Rust 实现的 VHDX（Virtual Hard Disk v2）虚拟磁盘文件格式库，完整支持 Microsoft MS-VHDX 规范。

**版本**: 0.1.0  
**Rust Edition**: 2021  
**许可证**: MIT

## 项目结构

```
vhdx-rs/
├── Cargo.toml          # 项目配置
├── README.md           # 项目说明
├── src/
│   ├── lib.rs          # 库入口，导出公共 API
│   ├── main.rs         # CLI 工具 (vhdx-tool)
│   ├── error.rs        # 错误类型定义
│   ├── common/         # 通用工具模块
│   ├── header/         # Header Section 处理
│   ├── bat/            # Block Allocation Table
│   ├── log/            # Log 系统
│   ├── metadata/       # Metadata Region
│   ├── payload/        # Payload Blocks (占位)
│   ├── block_io/       # 块级 I/O 实现
│   └── file/           # VHDX 文件操作
└── tests/
    └── integration/    # 集成测试
```

## 核心依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| uuid | 1.22 | GUID 生成与处理 |
| thiserror | 2.0 | 错误处理宏 |
| byteorder | 1.5 | 字节序转换 |
| clap | 4.6 | CLI 参数解析 |
| crc32c | 0.6 | CRC-32C 校验 |

## 架构层次

### 1. 底层数据结构层 (Data Structures)

位于 `src/common/`、`src/header/`、`src/bat/`、`src/metadata/`、`src/log/`

- **GUID 处理**: 128-bit GUID 的小端序序列化/反序列化
- **CRC-32C**: 使用 Castagnoli 多项式 (0x1EDC6F41) 的校验和计算
- **Header**: 双头安全机制，支持版本检测和校验
- **BAT**: 块分配表，管理虚拟块到文件块的映射
- **Metadata**: 磁盘参数存储（大小、扇区大小、父磁盘等）
- **Log**: 崩溃恢复日志系统

### 2. I/O 抽象层 (I/O Abstraction)

位于 `src/block_io/`

- **BlockIo Trait**: 统一的块读写接口
- **FixedBlockIo**: 固定磁盘 I/O（直接偏移计算）
- **DynamicBlockIo**: 动态磁盘 I/O（按需分配）
- **DifferencingBlockIo**: 差异磁盘 I/O（父磁盘链）
- **BlockCache**: 块缓存优化

### 3. 文件管理层 (File Management)

位于 `src/file/`

- **VhdxFile**: 已打开文件的句柄，提供读写接口
- **VhdxBuilder**: Builder 模式创建新 VHDX 文件
- **DiskType**: 磁盘类型枚举（Fixed/Dynamic/Differencing）

### 4. 应用层 (Application)

位于 `src/main.rs`

- CLI 工具 `vhdx-tool`
- 支持 info/create/read/write/check 命令

## 关键设计决策

### 1. 双头安全机制 (Dual Headers)

VHDX 规范要求维护两个 Header 副本（64KB 和 128KB），通过 sequence_number 判断当前有效版本。在更新时，先写入非当前头，再写入当前头，确保断电安全。

```rust
// src/header/header.rs
pub const OFFSET_1: u64 = 64 * 1024;   // Header 1
pub const OFFSET_2: u64 = 128 * 1024;  // Header 2
```

### 2. BAT 布局策略

BAT 采用交错布局：每个 Chunk 包含 payload_blocks + sector_bitmap_block

```
Chunk 0: [payload_0]...[payload_N] [sector_bitmap_0]
Chunk 1: [payload_N+1]...[payload_2N] [sector_bitmap_1]
...
```

其中 N = ChunkRatio = ChunkSize / BlockSize = 2^23 * SectorSize / BlockSize

### 3. 日志系统 (Log Replay)

打开文件时自动检测并重放日志：

1. 检查 header.log_guid 是否非空
2. 读取日志区域数据
3. 查找活动序列 (active sequence)
4. 重放日志条目到文件
5. 清空日志区域

### 4. 差异磁盘链

差异磁盘通过 `parent` 字段链接到父磁盘：

```rust
pub struct VhdxFile {
    // ...
    pub parent: Option<Box<VhdxFile>>,
}
```

读取时优先检查当前磁盘的 BAT，如果不存在则递归查询父磁盘。

## 模块依赖关系

```
lib.rs
├── common/ (guid, crc32c, disk_type)
├── error/ (VhdxError)
├── header/ ─────┬──> common/
│   ├── file_type
│   ├── header
│   └── region_table
├── bat/ ────────┬──> common/
│   ├── entry    ├──> error/
│   ├── states
│   └── table
├── log/ ────────┬──> common/
│   ├── entry    ├──> error/
│   ├── descriptor
│   ├── sector
│   ├── replayer
│   └── writer
├── metadata/ ───┬──> common/
│   ├── ...      ├──> error/
├── block_io/ ───┬──> bat/
│   ├── traits   ├──> common/
│   ├── fixed    ├──> error/
│   ├── dynamic  └──> log/
│   ├── differencing
│   └── cache
└── file/ ───────┬──> header/
    ├── vhdx_file ├──> bat/
    ├── builder   ├──> metadata/
    └── mod       ├──> log/
                  └──> block_io/
```

## 内存管理策略

### 1. 数据加载

- Header、Region Table、Metadata：打开时一次性加载
- BAT：打开时完整加载（通常几百 KB 到几 MB）
- Payload Blocks：按需读写（通过 Block I/O 层）
- Parent Disk：差异磁盘打开时递归加载

### 2. 缓存策略

BlockCache 提供简单的 LRU 缓存机制，缓存最近访问的数据块。

## 线程安全

当前版本未实现 `Send`/`Sync` trait，VhdxFile 实例不能在多线程间共享。如需多线程访问，建议：

1. 每个线程打开独立的文件句柄
2. 或使用 `Arc<Mutex<VhdxFile>>` 包装

## 与 VHDX 规范的对应关系

| VHDX 规范结构 | 实现文件 | 说明 |
|--------------|----------|------|
| File Type Identifier | `header/file_type.rs` | "vhdxfile" 签名 |
| VHDX Header | `header/header.rs` | 4KB 双头结构 |
| Region Table | `header/region_table.rs` | BAT/Metadata 位置表 |
| Metadata Region | `metadata/` | 磁盘参数 |
| Block Allocation Table | `bat/` | 块映射表 |
| Log Entries | `log/` | 日志条目/描述符/扇区 |
| Payload Blocks | `block_io/` | 数据块读写 |

## 已知限制

1. **Sector Bitmap**: 基础支持，完整实现待完善
2. **Differencing**: 支持父子链读取，写入优化待完善
3. **Resize**: 动态调整磁盘大小未实现
4. **Encryption**: 不支持加密 VHDX

## 后续阅读

- [02-core-modules.md](./02-core-modules.md) - 核心模块详述
- [03-block-io.md](./03-block-io.md) - Block I/O 实现
- [04-file-operations.md](./04-file-operations.md) - 文件操作与 Builder
- [05-cli-tool.md](./05-cli-tool.md) - CLI 工具使用
- [06-api-guide.md](./06-api-guide.md) - API 使用指南
- [07-error-handling.md](./07-error-handling.md) - 错误处理与测试
