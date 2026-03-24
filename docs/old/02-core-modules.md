# 核心模块详述

本文档详细介绍 vhdx-rs 旧版的核心模块实现：Header、Region Table、BAT、Log 和 Metadata。

---

## 1. Header 模块

**文件位置**: `src/header/`

### 1.1 File Type Identifier

**文件**: `src/header/file_type.rs`

VHDX 文件以 64KB 的文件类型标识符开头：

```rust
pub struct FileTypeIdentifier {
    pub signature: [u8; 8],      // "vhdxfile"
    pub creator: [u8; 512],      // 创建者字符串
}
```

**关键常量**:
- `SIGNATURE`: `[0x76, 0x68, 0x64, 0x78, 0x66, 0x69, 0x6c, 0x65]` ("vhdxfile")
- `SIZE`: 64KB (65536 bytes)

创建者字符串是可选的，通常包含创建工具的名称和版本。

### 1.2 VHDX Header

**文件**: `src/header/header.rs`

VHDX 维护两个 Header 副本以实现崩溃安全：

```rust
pub struct VhdxHeader {
    pub signature: [u8; 4],        // "head"
    pub checksum: u32,             // CRC-32C 校验和
    pub sequence_number: u64,      // 序列号（用于选择当前头）
    pub file_write_guid: Guid,     // 文件写入 GUID
    pub data_write_guid: Guid,     // 数据写入 GUID
    pub log_guid: Guid,            // 日志 GUID（非空表示有待重放日志）
    pub log_version: u16,          // 日志版本（应为 0）
    pub version: u16,              // VHDX 版本（应为 1，表示 VHDX v2）
    pub log_length: u32,           // 日志区域长度
    pub log_offset: u64,           // 日志区域偏移
}
```

**内存布局** (4KB):

| 偏移 | 大小 | 字段 | 说明 |
|------|------|------|------|
| 0 | 4 | signature | "head" |
| 4 | 4 | checksum | CRC-32C（计算时此字段置零） |
| 8 | 8 | sequence_number | 单调递增序列号 |
| 16 | 16 | file_write_guid | 唯一标识文件实例 |
| 32 | 16 | data_write_guid | 唯一标识数据状态 |
| 48 | 16 | log_guid | 唯一标识日志序列 |
| 64 | 2 | log_version | 日志格式版本 |
| 66 | 2 | version | VHDX 规范版本 |
| 68 | 4 | log_length | 日志区域字节数 |
| 72 | 8 | log_offset | 日志区域文件偏移 |
| 80 | 4016 | reserved | 保留（必须为零） |

**双头选择算法**:

```rust
pub fn read_headers(file: &mut File) -> Result<(usize, VhdxHeader, VhdxHeader)> {
    // 读取 Header 1 (64KB) 和 Header 2 (128KB)
    let header1 = read_at(file, 64*1024)?;
    let header2 = read_at(file, 128*1024)?;
    
    // 验证签名和校验和
    let valid1 = header1.is_valid();
    let valid2 = header2.is_valid();
    
    match (valid1, valid2) {
        (true, true) => {
            // 两者都有效，选择 sequence_number 较大的
            if header1.sequence_number > header2.sequence_number {
                Ok((0, header1, header2))
            } else {
                Ok((1, header2, header1))
            }
        }
        (true, false) => Ok((0, header1, header2)),
        (false, true) => Ok((1, header2, header1)),
        (false, false) => Err(VhdxError::NoValidHeader),
    }
}
```

**安全更新策略**:

1. 先写入非当前 Header（较低 sequence_number）
2. 再写入当前 Header（较高 sequence_number）
3. 确保每次写入后调用 `fsync`

这样即使在写入过程中断电，至少有一个 Header 是有效的。

### 1.3 Region Table

**文件**: `src/header/region_table.rs`

Region Table 描述 BAT 和 Metadata 的位置：

```rust
pub struct RegionTable {
    pub signature: [u8; 4],        // "regi"
    pub checksum: u32,
    pub entry_count: u32,          // 条目数量（通常为 2）
    pub entries: Vec<RegionEntry>,
}

pub struct RegionEntry {
    pub guid: Guid,                // 区域类型 GUID
    pub file_offset: u64,          // 区域偏移（1MB 对齐）
    pub length: u32,               // 区域长度
    pub required: u32,             // 是否必需（1=必需，0=可选）
}
```

**预定义 GUID**:

| GUID | 名称 | 说明 |
|------|------|------|
| `2DC27766-F623-4200-9D64-115E9BFD4A08` | BAT | 块分配表 |
| `8B7CA206-4790-4B9A-B8FE-575F050F886E` | Metadata | 元数据区域 |

**内存布局** (64KB):

| 偏移 | 大小 | 字段 |
|------|------|------|
| 0 | 4 | signature ("regi") |
| 4 | 4 | checksum |
| 8 | 4 | entry_count |
| 12 | 4 | reserved |
| 16 | - | entries[entry_count] |

每个 Entry 32 字节：
- 0-15: guid
- 16-23: file_offset
- 24-27: length
- 28-31: required

**Region Table 同样有两个副本**（192KB 和 256KB），选择逻辑与 Header 相同。

---

## 2. BAT (Block Allocation Table) 模块

**文件位置**: `src/bat/`

### 2.1 BAT Entry

**文件**: `src/bat/entry.rs`

每个 BAT Entry 是 64 位：

```rust
pub struct BatEntry {
    pub raw: u64,                  // 原始值
    pub state: PayloadBlockState,  // 块状态（低 3 位）
    pub file_offset_mb: u64,       // 文件偏移（MB 为单位，高 61 位）
}
```

**位布局**:

```
63 62 61 60 ... 3 2 1 0
 └────────┘      └──┘
   Offset         State
   (61 bits)      (3 bits)
```

### 2.2 块状态 (PayloadBlockState)

**文件**: `src/bat/states.rs`

```rust
pub enum PayloadBlockState {
    NotPresent = 0,      // 未分配（读为零）
    Undefined = 1,       // 未定义（错误状态）
    Zero = 2,            // 显式零块（读为零）
    Unmapped = 3,        // 未映射（保留）
    FullyPresent = 4,    // 完全存在（正常数据）
    PartiallyPresent = 5, // 部分存在（差异磁盘）
    // 6-7: 保留
}
```

**用途说明**:

- **NotPresent**: 动态/差异磁盘中的未分配块，读取返回零
- **Zero**: 显式清零的块，与 NotPresent 类似但语义不同
- **FullyPresent**: 正常的数据块，file_offset_mb 指向实际数据
- **PartiallyPresent**: 差异磁盘中的部分块，需检查 Sector Bitmap

### 2.3 BAT Table

**文件**: `src/bat/table.rs`

```rust
pub struct Bat {
    pub entries: Vec<BatEntry>,           // 所有条目
    pub virtual_disk_size: u64,           // 虚拟磁盘大小
    pub block_size: u64,                  // 块大小
    pub logical_sector_size: u32,         // 逻辑扇区大小
    pub num_payload_blocks: u64,          // Payload 块数量
    pub num_sector_bitmap_blocks: u64,    // Sector Bitmap 块数量
    pub chunk_ratio: u64,                 // 每 Chunk 的 payload 块数
    pub chunk_size: u64,                  // Chunk 大小
    pub bat_file_offset: u64,             // BAT 在文件中的偏移
}
```

**核心算法**:

**Chunk Ratio 计算**:

```rust
pub fn calculate_chunk_ratio(block_size: u64, logical_sector_size: u32) -> u64 {
    let chunk_size = (1u64 << 23) * logical_sector_size as u64;  // 8MB * SectorSize
    chunk_size / block_size
}
```

示例（512 字节扇区，32MB 块）：
- ChunkSize = 2^23 * 512 = 4GB
- ChunkRatio = 4GB / 32MB = 128

**BAT 索引计算**:

```rust
// Payload 块索引 -> BAT 索引
pub fn payload_bat_index(&self, block_idx: u64) -> Option<usize> {
    let chunk_idx = block_idx / self.chunk_ratio;
    let block_in_chunk = block_idx % self.chunk_ratio;
    let bat_idx = chunk_idx * (self.chunk_ratio + 1) + block_in_chunk;
    Some(bat_idx as usize)
}

// Sector Bitmap 索引 -> BAT 索引
pub fn sector_bitmap_bat_index(&self, chunk_idx: u64) -> Option<usize> {
    let bat_idx = chunk_idx * (self.chunk_ratio + 1) + self.chunk_ratio;
    Some(bat_idx as usize)
}
```

**BAT 布局示例**（ChunkRatio = 4）:

```
索引:    0      1      2      3      4      5      6      7      8      9
内容: [PB_0] [PB_1] [PB_2] [PB_3] [SB_0] [PB_4] [PB_5] [PB_6] [PB_7] [SB_1]
       └──────── Chunk 0 ────────┘ └──────── Chunk 1 ────────┘

PB = Payload Block, SB = Sector Bitmap Block
```

**虚拟偏移转换**:

```rust
pub fn translate(&self, virtual_offset: u64) -> Result<Option<u64>> {
    let block_idx = virtual_offset / self.block_size;
    let offset_in_block = virtual_offset % self.block_size;
    
    let entry = self.get_payload_entry(block_idx)?;
    
    match entry.state {
        PayloadBlockState::FullyPresent => {
            let file_offset = entry.file_offset_mb * 1024 * 1024;
            Ok(Some(file_offset + offset_in_block))
        }
        PayloadBlockState::PartiallyPresent => {
            // 需要检查 Sector Bitmap
            Ok(None)
        }
        _ => Ok(None), // 返回零
    }
}
```

---

## 3. Log 模块

**文件位置**: `src/log/`

Log 系统确保元数据更新的原子性和崩溃恢复能力。

### 3.1 Log Entry Header

**文件**: `src/log/entry.rs`

```rust
pub struct LogEntryHeader {
    pub signature: [u8; 4],        // "loge"
    pub checksum: u32,
    pub entry_length: u32,         // 整个 Entry 的长度（含数据）
    pub tail: u32,                 // 到下一个 Entry 的偏移
    pub sequence_number: u64,      // 序列号（严格递增）
    pub descriptor_count: u32,     // 描述符数量
}
```

### 3.2 描述符 (Descriptors)

**文件**: `src/log/descriptor.rs`

**Zero Descriptor** - 清零操作:

```rust
pub struct ZeroDescriptor {
    pub signature: [u8; 4],        // "zero"
    pub reserved: u32,
    pub zero_length: u64,          // 清零字节数
    pub file_offset: u64,          // 目标文件偏移
    pub sequence_number: u64,      // 序列号（必须与 Entry 匹配）
}
```

**Data Descriptor** - 数据写入:

```rust
pub struct DataDescriptor {
    pub signature: [u8; 4],        // "desc"
    pub trailing_bytes: [u8; 4],   // 尾部字节（扇区未对齐部分）
    pub leading_bytes: [u8; 8],    // 头部字节（扇区未对齐部分）
    pub file_offset: u64,          // 目标文件偏移
    pub sequence_number: u64,      // 序列号
}
```

### 3.3 Data Sector

**文件**: `src/log/sector.rs`

Data Descriptor 后跟随一个或多个 Data Sector（4KB 对齐）:

```rust
pub struct DataSector {
    pub signature: [u8; 4],        // "data"
    pub sequence_high: u32,        // 序列号高 32 位
    // ... 4084 字节数据 ...
    pub sequence_low: u32,         // 序列号低 32 位
}
```

序列号同时存储在头部和尾部，用于验证数据完整性。

### 3.4 Log Replayer

**文件**: `src/log/replayer.rs`

**重放流程**:

```rust
impl LogReplayer {
    /// 查找活动日志序列
    pub fn find_active_sequence(
        log_data: &[u8],
        log_length: u32,
        log_guid: &Guid,
    ) -> Result<Option<LogSequence>> {
        // 扫描日志区域，查找具有有效 GUID 的序列
        // 验证每个条头的校验和
        // 按 sequence_number 排序
        // 返回最新的完整序列
    }
    
    /// 重放序列
    pub fn replay_sequence(
        sequence: &LogSequence,
        file: &mut File,
    ) -> Result<u64> {
        // 遍历序列中的所有条目
        // 根据描述符类型执行相应操作
        // 返回最后写入的文件偏移
    }
}
```

**崩溃恢复场景**:

1. **写入 Header 时崩溃**: 使用双头机制恢复
2. **写入 BAT 时崩溃**: 重放日志恢复
3. **写入 Payload 时崩溃**: 日志不保护 payload 数据（由上层应用处理）

### 3.5 Log Writer

**文件**: `src/log/writer.rs`

用于创建新的日志条目：

```rust
pub struct LogWriter {
    log_offset: u64,
    log_length: u32,
    log_guid: Guid,
    next_sequence: u64,
}

impl LogWriter {
    /// 开始新的日志序列
    pub fn new_sequence(&mut self) -> LogSequenceBuilder;
    
    /// 写入 Data Descriptor
    pub fn write_data(&mut self, offset: u64, data: &[u8]) -> Result<()>;
    
    /// 写入 Zero Descriptor
    pub fn write_zero(&mut self, offset: u64, length: u64) -> Result<()>;
    
    /// 提交序列
    pub fn commit(&mut self) -> Result<()>;
}
```

---

## 4. Metadata 模块

**文件位置**: `src/metadata/`

### 4.1 Metadata Region

**文件**: `src/metadata/region.rs`

Metadata Region 包含磁盘的所有配置参数：

```rust
pub struct MetadataRegion {
    pub table: MetadataTable,      // 条目表
    pub file_parameters: FileParameters,
    pub virtual_disk_size: VirtualDiskSize,
    pub virtual_disk_id: VirtualDiskId,
    pub logical_sector_size: SectorSize,
    pub physical_sector_size: SectorSize,
    pub parent_locator: Option<ParentLocator>,
}
```

### 4.2 Metadata Table

**文件**: `src/metadata/table.rs`

```rust
pub struct MetadataTable {
    pub signature: [u8; 8],        // "metadata"
    pub entry_count: u16,          // 条目数量
    pub entries: Vec<MetadataTableEntry>,
}

pub struct MetadataTableEntry {
    pub item_id: Guid,             // 参数类型 GUID
    pub offset: u32,               // 数据在 Metadata Region 中的偏移
    pub length: u32,               // 数据长度
    pub flags: u32,                // 标志位
}
```

**标志位**:
- Bit 0: `is_user` - 是否为用户元数据
- Bit 1: `is_virtual_disk` - 是否为虚拟磁盘元数据
- Bit 2: `is_required` - 是否必需

### 4.3 元数据项

**File Parameters** - 文件参数:

```rust
pub struct FileParameters {
    pub block_size: u32,           // 块大小（1MB-256MB）
    pub has_parent: bool,          // 是否为差异磁盘
}
// GUID: CAA16737-FA36-4D43-B3B6-33F0AA44E76B
```

**Virtual Disk Size** - 虚拟磁盘大小:

```rust
pub struct VirtualDiskSize {
    pub size: u64,                 // 虚拟磁盘大小（字节）
}
// GUID: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
```

**Sector Sizes** - 扇区大小:

```rust
pub struct SectorSize {
    pub size: u32,                 // 512 或 4096
}
// Logical Sector GUID: 8141BF1D-A96F-4709-BA47-F23337BDD29B
// Physical Sector GUID: CDA348C7-445D-4471-9CC9-E9885251C556
```

**Virtual Disk ID** - 磁盘 GUID:

```rust
pub struct VirtualDiskId {
    pub guid: Guid,                // 唯一标识虚拟磁盘
}
// GUID: BECA12AB-B2E6-4523-93EF-C309E000C746
```

**Parent Locator** - 父磁盘定位器（差异磁盘）:

```rust
pub struct ParentLocator {
    pub entries: Vec<ParentLocatorEntry>,
}

pub struct ParentLocatorEntry {
    pub key: String,               // 键名（如 "relative_path"）
    pub value: String,             // 值（如 "..\\parent.vhdx"）
}
// GUID: 2E0B1E58-8CC1-4E2B-96C8-5CDB96B2429C
```

### 4.4 Metadata 布局示例

```
Metadata Region (1MB):
├─ Metadata Table (64KB)
│  ├─ Signature: "metadata"
│  ├─ Entry Count: 5
│  └─ Entries[5]:
│     ├─ [0] FileParameters @ offset=65536, length=8
│     ├─ [1] VirtualDiskSize @ offset=65544, length=8
│     ├─ [2] LogicalSectorSize @ offset=65552, length=4
│     ├─ [3] PhysicalSectorSize @ offset=65556, length=4
│     └─ [4] VirtualDiskId @ offset=65560, length=16
│
└─ Data Area (64KB - 1MB)
   ├─ @65536: FileParameters (8 bytes)
   ├─ @65544: VirtualDiskSize (8 bytes)
   ├─ @65552: LogicalSectorSize (4 bytes)
   ├─ @65556: PhysicalSectorSize (4 bytes)
   └─ @65560: VirtualDiskId (16 bytes)
```

---

## 5. 模块交互示例

### 5.1 打开 VHDX 文件流程

```rust
// VhdxFile::open() 内部流程

1. 读取 File Type Identifier (0-64KB)
   └─ 验证 "vhdxfile" 签名

2. 读取并选择 Header
   ├─ 读取 Header 1 @ 64KB
   ├─ 读取 Header 2 @ 128KB
   └─ 选择 sequence_number 较大的有效 Header

3. 读取 Region Table
   ├─ 读取 Region Table 1 @ 192KB
   ├─ 读取 Region Table 2 @ 256KB
   └─ 验证并解析 BAT/Metadata 位置

4. 重放日志（如有需要）
   └─ 如果 header.log_guid 非空，执行 Log Replay

5. 读取 Metadata
   ├─ 定位到 Metadata Region
   ├─ 读取 Metadata Table
   └─ 解析 FileParameters、VirtualDiskSize 等

6. 读取 BAT
   ├─ 定位到 BAT Region
   └─ 解析所有 BAT Entry

7. 初始化 Block I/O
   └─ 根据 DiskType 创建对应的 BlockIo 实现
```

### 5.2 读取数据流程

```rust
// VhdxFile::read(offset, buf) 内部流程

1. 检查 offset 是否越界

2. 计算 Block Index
   └─ block_idx = offset / block_size

3. 查询 BAT
   ├─ bat_idx = chunk_idx * (chunk_ratio + 1) + block_in_chunk
   ├─ entry = bat.entries[bat_idx]
   └─ 根据 entry.state 决定操作

4. 根据状态处理
   ├─ FullyPresent: 计算文件偏移并读取
   ├─ PartiallyPresent: 检查 Sector Bitmap，可能需要读取父磁盘
   └─ NotPresent/Zero: 返回零

5. 对于差异磁盘
   └─ 如果当前磁盘无数据，递归查询父磁盘
```

### 5.3 写入数据流程

```rust
// VhdxFile::write(offset, buf) 内部流程

1. 检查是否为只读模式

2. 对于动态磁盘
   ├─ 检查目标块是否已分配
   ├─ 如未分配，分配新块
   └─ 更新 BAT Entry

3. 对于差异磁盘
   ├─ 检查 Sector Bitmap
   ├─ 如未写入，分配空间并更新
   └─ 标记已修改的扇区

4. 写入数据
   ├─ 更新 Header.data_write_guid
   ├─ 写入 payload 数据
   └─ 更新 Header（通过 Log 保证原子性）
```

---

## 6. 关键常量汇总

| 常量 | 值 | 说明 |
|------|------|------|
| FILE_TYPE_OFFSET | 0 | 文件类型标识符偏移 |
| HEADER_OFFSET_1 | 64KB | Header 1 偏移 |
| HEADER_OFFSET_2 | 128KB | Header 2 偏移 |
| REGION_TABLE_OFFSET_1 | 192KB | Region Table 1 偏移 |
| REGION_TABLE_OFFSET_2 | 256KB | Region Table 2 偏移 |
| HEADER_SIZE | 4KB | Header 大小 |
| REGION_TABLE_SIZE | 64KB | Region Table 大小 |
| FILE_TYPE_SIZE | 64KB | 文件类型标识符大小 |
| DEFAULT_BLOCK_SIZE | 32MB | 默认块大小 |
| DEFAULT_LOGICAL_SECTOR | 512 | 默认逻辑扇区大小 |
| DEFAULT_PHYSICAL_SECTOR | 4096 | 默认物理扇区大小 |
| CHUNK_SIZE_FACTOR | 2^23 | Chunk 大小计算因子 |

---

## 7. 参考文档

- [MS-VHDX 规范](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-vhdx/)
- [01-architecture-overview.md](./01-architecture-overview.md) - 架构概述
- [03-block-io.md](./03-block-io.md) - Block I/O 实现
