# Phase 2: VHDX 元数据层 - Metadata Region & BAT

**目标**: 实现 Metadata Region 解析和 BAT（块分配表）管理。

**依赖**: Phase 1 (Header 解析)

**参考**: MS-VHDX.md Section 2.6 (Metadata), Section 2.5 (BAT)

---

## 2.1 Metadata Region 解析 (Section 2.6)

**位置**: 由 Region Table 指定，1MB 对齐
**结构**: 64KB 元数据表头 + 多个元数据项

### 2.1.1 Metadata Table Header

**数据结构**:
```
Signature (8 bytes):    0x636174616465746D ("metadata")
Reserved (2 bytes):     0
EntryCount (2 bytes):   元数据项数量
Reserved (4 bytes):     0
Reserved (4012 bytes):  0
```

**实现任务**:
- [ ] 定义 MetadataTableHeader 结构体
- [ ] 实现 Signature 验证（"metadata"）
- [ ] 解析 EntryCount

### 2.1.2 Metadata Table Entry

**数据结构**:
```
ItemId (16 bytes):      GUID 标识
Offset (4 bytes):       相对 Metadata Region 起点的偏移（≥ 64KB）
Length (4 bytes):       数据长度
Flags (4 bytes):        bit 0 = IsUser, bit 1 = IsVirtualDisk
Reserved (4 bytes):     0
```

### 2.1.3 系统元数据项 (Required)

| 元数据项 | GUID | 说明 |
|----------|------|------|
| File Parameters | CAA16737-FA36-4D43-B3B6-33F0AA44E76B | 块大小、磁盘类型 |
| Virtual Disk Size | 2FA54224-CD1B-4876-B211-5DBED83BF4B8 | 虚拟磁盘大小（字节） |
| Virtual Disk ID | BECA12AB-B2E6-4523-93EF-C309E000C746 | 虚拟磁盘唯一标识 |
| Logical Sector Size | 8141BF1D-A96F-4709-BA47-F233A8FAAB5F | 逻辑扇区大小（512 或 4096） |
| Physical Sector Size | CDA348C7-445D-4471-9CC9-E9885251C556 | 物理扇区大小（512 或 4096） |
| Parent Locator | A8D35F2D-B30B-454D-ABF7-D3D84834AB0C | 差分磁盘的父定位器（条件必需） |

**实现任务**:
- [ ] 定义 MetadataTableEntry 结构体
- [ ] 实现元数据表解析
- [ ] 实现各系统元数据项的解析

### 2.1.4 File Parameters (Section 2.6.2.1)

**数据结构**:
```
BlockSize (4 bytes):     块大小（1MB - 256MB，必须是 1MB 倍数）
HasParent (4 bytes):     0 = 无父磁盘，1 = 差分磁盘
Reserved (4 bytes):      0
Reserved (4 bytes):      0
```

**实现任务**:
- [ ] 解析 BlockSize
- [ ] 解析 HasParent 标志
- [ ] 验证 BlockSize 范围和对齐

### 2.1.5 Virtual Disk Size (Section 2.6.2.2)

**数据结构**:
```
VirtualDiskSize (8 bytes):  虚拟磁盘大小（字节，> 0）
```

### 2.1.6 Logical/Physical Sector Size (Section 2.6.2.4, 2.6.2.5)

**有效值**: 512 或 4096 字节

### 2.1.7 Parent Locator (Section 2.6.2.6) - 差分磁盘

**仅当 HasParent = 1 时必需**

**数据结构**:
```
Reserved (4 bytes):              0
KeyCount (4 bytes):              键值对数量
Reserved (8 bytes):              0
Entries:                         可变数量的 Parent Locator Entry
```

**Parent Locator Entry**:
```
KeyOffset (4 bytes):     键字符串相对 Parent Locator 起点的偏移
ValueOffset (4 bytes):   值字符串相对 Parent Locator 起点的偏移
KeyLength (2 bytes):     键长度（UTF-16 LE 字符数）
ValueLength (2 bytes):   值长度（UTF-16 LE 字符数）
```

**常见键**:
- `"parent_linkage"`: 父磁盘的 Virtual Disk ID
- `"parent_linkage2"`: 父磁盘的 DataWriteGuid
- `"relative_path"`: 父磁盘的相对路径
- `"absolute_win32_path"`: 父磁盘的绝对路径（Windows）

**实现任务**:
- [ ] 定义 ParentLocator 和 ParentLocatorEntry 结构体
- [ ] 解析键值对
- [ ] 支持 UTF-16 LE 字符串读取

### Metadata Region 验收标准

- [ ] 能正确解析 Metadata Table
- [ ] 能读取所有系统元数据项
- [ ] 对 Required 但未找到的元数据项返回错误
- [ ] 能解析 Parent Locator（差分磁盘）

---

## 2.2 BAT (Block Allocation Table) (Section 2.5)

**位置**: 由 Region Table 指定，1MB 对齐
**结构**: 64位条目的数组

### 2.2.1 BAT Entry 结构

**数据结构**:
```
State (3 bits):          块状态
Reserved (17 bits):      0
FileOffsetMB (44 bits):  文件偏移（MB 为单位）
```

### 2.2.2 Payload Block 状态 (Section 2.5.1.1)

| 状态 | 值 | Fixed | Dynamic | Differencing | 说明 |
|------|-----|-------|---------|--------------|------|
| NOT_PRESENT | 0 | ✓ | ✓ | ✓ | 块未分配 |
| UNDEFINED | 1 | ✓ | ✓ | ✓ | 块内容未定义 |
| ZERO | 2 | ✓ | ✓ | ✓ | 块内容为零 |
| UNMAPPED | 3 | ✓ | ✓ | ✓ | 收到 UNMAP 命令 |
| FULLY_PRESENT | 6 | ✓ | ✓ | ✓ | 块完全存在于文件 |
| PARTIALLY_PRESENT | 7 | - | - | ✓ | 仅差分磁盘，需查 Sector Bitmap |

### 2.2.3 Sector Bitmap Block 状态 (Section 2.5.1.2)

| 状态 | 值 | 说明 |
|------|-----|------|
| NOT_PRESENT | 0 | 块未分配 |
| PRESENT | 6 | 块存在于文件 |

### 2.2.4 Chunk 和 Chunk Ratio

**关键概念**:
- **Chunk Size**: 一个 Sector Bitmap Block 能描述的虚拟磁盘大小
  - `ChunkSize = 2^23 * LogicalSectorSize`
  - 512B 扇区: ChunkSize = 4GB
  - 4096B 扇区: ChunkSize = 32GB

- **Chunk Ratio**: 每个 Sector Bitmap Block 对应的 Payload Block 数
  - `ChunkRatio = ChunkSize / BlockSize`

**BAT 布局（交错存储）**:
```
[PB0][PB1]...[PB(n-1)][SB][PBn]...[PB(2n-1)][SB]...
```
- PB = Payload Block Entry
- SB = Sector Bitmap Block Entry
- n = ChunkRatio

### 2.2.5 BAT 索引计算

**Payload Block 数量**:
```
NumberOfPayloadBlocks = ceil(VirtualDiskSize / BlockSize)
```

**Sector Bitmap Block 数量**:
```
NumberOfSectorBitmapBlocks = ceil(NumberOfPayloadBlocks / ChunkRatio)
```

**BAT 条目总数**:
- 固定/动态: `TotalEntries = NumberOfPayloadBlocks + NumberOfSectorBitmapBlocks`
- 差分: 需要特殊计算（参见 MS-VHDX 2.5）

**计算 BAT 索引**:
```rust
chunk_index = block_idx / ChunkRatio
block_in_chunk = block_idx % ChunkRatio
bat_index = chunk_index * (ChunkRatio + 1) + block_in_chunk
```

**Sector Bitmap 索引**:
```rust
sb_index = chunk_index * (ChunkRatio + 1) + ChunkRatio
```

### 实现任务

- [ ] 定义 BatEntry 结构体
- [ ] 实现 ChunkRatio 计算
- [ ] 实现 Payload Block BAT 索引计算
- [ ] 实现 Sector Bitmap Block BAT 索引计算
- [ ] 实现虚拟偏移到文件偏移的转换
- [ ] 支持不同块状态的处理

### 虚拟偏移到文件偏移转换逻辑

```rust
fn translate(&self, virtual_offset: u64) -> Option<u64> {
    let block_idx = virtual_offset / self.block_size;
    let offset_in_block = virtual_offset % self.block_size;
    
    let bat_entry = self.get_bat_entry(block_idx)?;
    
    match bat_entry.state {
        FULLY_PRESENT => {
            let file_offset = bat_entry.file_offset_mb * 1024 * 1024;
            Some(file_offset + offset_in_block)
        }
        ZERO | NOT_PRESENT | UNMAPPED => None,
        PARTIALLY_PRESENT => {
            // 需要查 Sector Bitmap（差分磁盘）
            None
        }
        _ => None,
    }
}
```

### BAT 验收标准

- [ ] 能正确计算 ChunkRatio
- [ ] 能正确计算 BAT 索引（交错布局）
- [ ] 能实现虚拟偏移到文件偏移的转换
- [ ] 能处理各种块状态（FULLY_PRESENT, ZERO, NOT_PRESENT）
- [ ] 支持固定、动态、差分三种磁盘类型

---

## Phase 2 完成标准

- [ ] Metadata Region 完整解析
- [ ] 所有系统元数据项读取
- [ ] BAT 表加载和索引计算
- [ ] 虚拟偏移到文件偏移的转换
- [ ] 单元测试覆盖各种块大小和扇区大小组合

## 下一步

完成 Phase 2 后，进入 **Phase 3: 日志系统**，实现 Log Entry 解析和 Log Replay 机制。
