# Phase 5: VHDX 差异磁盘 - Differencing Disk Support

**目标**: 实现差异磁盘（Differencing VHDX）支持，包括父磁盘定位和扇区位图管理。

**依赖**: Phase 4 (Block I/O)

**参考**: MS-VHDX.md Section 2.4 (Blocks), Section 2.6.2.6 (Parent Locator)

---

## 5.1 差异磁盘概述

**定义**: 存储相对于父虚拟磁盘的修改块的 VHDX 文件

**特点**:
- 只包含修改过的块（相对于父磁盘）
- 通过 Sector Bitmap 记录哪些扇区存在于当前文件
- 读取时优先从子磁盘获取，如果不存在则从父磁盘获取
- 可以有多个层次的父磁盘（父子链）

**关键字段**:
- FileParameters.HasParent = 1
- 必须包含 Parent Locator 元数据项

---

## 5.2 Parent Locator (Section 2.6.2.6)

**位置**: Metadata Region
**用途**: 定位父 VHDX 文件

### 5.2.1 数据结构回顾

```
Reserved (4 bytes):       0
KeyCount (4 bytes):       键值对数量
Reserved (8 bytes):       0
Entries:                  ParentLocatorEntry 数组
Key/Value Strings:        UTF-16 LE 字符串（紧跟 Entries）
```

**ParentLocatorEntry**:
```
KeyOffset (4 bytes):      键字符串偏移（相对 Parent Locator 起点）
ValueOffset (4 bytes):    值字符串偏移（相对 Parent Locator 起点）
KeyLength (2 bytes):      键长度（UTF-16 字符数）
ValueLength (2 bytes):    值长度（UTF-16 字符数）
```

### 5.2.2 标准键值对

| 键 | 用途 | 示例值 |
|----|------|--------|
| `parent_linkage` | 父磁盘 Virtual Disk ID | GUID 字符串 |
| `parent_linkage2` | 父磁盘 DataWriteGuid | GUID 字符串 |
| `relative_path` | 相对路径 | `..\\parent.vhdx` |
| `absolute_win32_path` | Windows 绝对路径 | `C:\\Disks\\parent.vhdx` |
| `absolute_uri` | URI 路径 | `file:///C:/Disks/parent.vhdx` |

### 实现任务

- [ ] 解析 Parent Locator 键值对
- [ ] 读取 UTF-16 LE 字符串
- [ ] 提取父磁盘路径
- [ ] 提取父磁盘 GUID 用于验证

---

## 5.3 父磁盘管理

### 5.3.1 父磁盘打开流程

```
1. 从 Parent Locator 获取父磁盘路径
2. 尝试按以下顺序查找：
   a. 相对路径（基于子磁盘位置）
   b. 绝对路径
   c. URI 路径
3. 打开父 VHDX 文件（只读）
4. 验证父磁盘的 Virtual Disk ID 与 parent_linkage 匹配
5. 如果验证失败，报错（父磁盘可能被替换）
```

### 5.3.2 父子链处理

**场景**: 差分磁盘可以有父磁盘，父磁盘也可以是差分磁盘

**处理**:
- 递归加载父磁盘
- 检测循环依赖（A 的父是 B，B 的父是 A）
- 限制链深度（防止无限递归）

### 实现任务

- [ ] 实现父磁盘路径解析
- [ ] 实现父磁盘打开和验证
- [ ] 实现父子链加载
- [ ] 实现循环依赖检测

---

## 5.4 Sector Bitmap (Section 2.4)

**用途**: 记录差分磁盘中哪些扇区存在于当前文件（相对于父磁盘被修改过）

**大小**: 固定 1MB

**位图布局**:
- 每个位对应一个逻辑扇区
- Bit 0 = 第一个虚拟扇区
- Bit N = 第 N 个虚拟扇区
- Bit = 1: 数据从当前文件读取
- Bit = 0: 数据从父磁盘读取

### 5.4.1 Chunk 和 Sector Bitmap

**关系**:
- 每个 Sector Bitmap Block 描述一个 Chunk 的扇区状态
- Chunk 大小 = 2^23 * LogicalSectorSize

**计算**:
```
ChunkSize = 2^23 * LogicalSectorSize
ChunkRatio = ChunkSize / BlockSize
SectorsPerChunk = ChunkSize / LogicalSectorSize = 2^23 = 8,388,608
```

**Sector Bitmap 容量**:
- 1MB = 8,388,608 bits
- 正好可以描述一个 Chunk 的所有扇区

### 5.4.2 Sector Bitmap 读取

**流程**:
```
1. 计算虚拟偏移所在的 Chunk
   chunk_idx = virtual_offset / ChunkSize
   sector_in_chunk = (virtual_offset % ChunkSize) / LogicalSectorSize

2. 从 BAT 获取 Sector Bitmap Block 位置
   sb_bat_index = chunk_idx * (ChunkRatio + 1) + ChunkRatio
   sb_entry = bat.get_entry(sb_bat_index)

3. 如果 sb_entry.state == PRESENT：
   a. 读取 Sector Bitmap Block（1MB）
   b. 检查对应位
   c. 如果位 = 1，从当前文件读取；否则从父磁盘读取

4. 如果 sb_entry.state == NOT_PRESENT：
   - 从父磁盘读取
```

### 实现任务

- [ ] 实现 Sector Bitmap 读取
- [ ] 实现位检查函数
- [ ] 实现 Chunk 索引计算

---

## 5.5 差分磁盘读取

### 5.5.1 读取流程

```
1. 计算 Block 索引和 Chunk 索引
2. 查询 BAT 获取 Payload Block 状态

3. 根据状态处理：
   - FULLY_PRESENT: 直接从当前文件读取
   
   - PARTIALLY_PRESENT:
     a. 查询 Sector Bitmap
     b. 如果位 = 1，从当前文件读取
     c. 如果位 = 0，从父磁盘递归读取
     
   - NOT_PRESENT/ZERO/UNMAPPED:
     - 从父磁盘递归读取

4. 如果父磁盘也是差分磁盘，递归查询
```

### 5.5.2 部分存在块处理

**场景**: 一个 Block 中部分扇区在当前文件，部分在父磁盘

**处理**:
- 对每个扇区分别查询 Sector Bitmap
- 从相应的来源读取
- 合并结果

### 实现任务

- [ ] 实现差分磁盘读取流程
- [ ] 实现父磁盘递归读取
- [ ] 实现部分存在块处理
- [ ] 实现扇区级读取决策

---

## 5.6 差分磁盘写入

### 5.6.1 写入流程

```
1. 计算 Block 索引和 Chunk 索引
2. 查询 BAT 获取当前状态

3. 如果状态为 NOT_PRESENT/ZERO/UNMAPPED：
   a. 分配新的 Payload Block
   b. 分配新的 Sector Bitmap Block（如果需要）
   c. 初始化 Sector Bitmap 为全 0
   d. 更新 BAT Entries（通过日志）

4. 如果状态为 PARTIALLY_PRESENT：
   a. 确保 Sector Bitmap Block 已分配

5. 写入数据到 Payload Block
6. 更新 Sector Bitmap 中对应位为 1
7. 更新 Sector Bitmap Block（通过日志）
```

### 5.6.2 写入时分配

**新块分配**:
- 分配 Payload Block 空间
- 分配 Sector Bitmap Block 空间（如果该 Chunk 的 Sector Bitmap 还不存在）
- 初始化 Sector Bitmap 为全 0
- 更新 BAT 两个 Entries

### 实现任务

- [ ] 实现差分磁盘写入
- [ ] 实现 Sector Bitmap 分配
- [ ] 实现 Sector Bitmap 更新
- [ ] 实现 BAT 批量更新（Payload + Sector Bitmap）

---

## 5.7 合并（Merge）- 可选高级功能

**定义**: 将差分磁盘的修改合并到父磁盘

**类型**:
- **Live Merge**: 运行时合并（复杂，需要 Hyper-V 支持）
- **Offline Merge**: 离线合并

**注意**: 合并是高级功能，初期实现可以跳过

---

## 5.8 测试场景

### 5.8.1 父磁盘定位测试
- [ ] 通过相对路径定位父磁盘
- [ ] 通过绝对路径定位父磁盘
- [ ] 父磁盘不存在时的错误处理
- [ ] 父磁盘 GUID 不匹配时的错误处理

### 5.8.2 Sector Bitmap 测试
- [ ] 读取已修改扇区（从子磁盘）
- [ ] 读取未修改扇区（从父磁盘）
- [ ] 读取部分修改的 Block
- [ ] 写入后更新 Sector Bitmap

### 5.8.3 父子链测试
- [ ] 3 层父子链（孙 -> 子 -> 父）
- [ ] 循环依赖检测
- [ ] 深层父子链性能

### 5.8.4 兼容性测试
- [ ] 与 Windows 创建的差分磁盘互操作
- [ ] 读取 Windows 修改过的差分磁盘
- [ ] Windows 读取我们创建的差分磁盘

---

## Phase 5 完成标准

- [ ] Parent Locator 解析
- [ ] 父磁盘定位和验证
- [ ] Sector Bitmap 管理
- [ ] 差分磁盘读取（父子链）
- [ ] 差分磁盘写入
- [ ] 通过所有差分磁盘测试
- [ ] 与 Windows 差分磁盘兼容

## 下一步

完成 Phase 5 后，进入 **Phase 6: 高级功能**，实现 Trim、快照等高级特性。
