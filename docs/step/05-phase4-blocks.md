# Phase 4: VHDX 块管理 - Block I/O & Dynamic Allocation

**目标**: 实现 Payload Block 的读取和写入，支持动态磁盘分配。

**依赖**: Phase 2 (Metadata + BAT), Phase 3 (Log Replay)

**参考**: MS-VHDX.md Section 2.4 (Blocks)

---

## 4.1 Block 概述

**两种 Block 类型**:
1. **Payload Block**: 存储虚拟磁盘数据，大小 = BlockSize（来自 File Parameters）
2. **Sector Bitmap Block**: 仅差分磁盘使用，大小固定 1MB

**虚拟块索引**:
- Payload Block 0 = 虚拟磁盘前 BlockSize 字节
- Payload Block N = 虚拟磁盘第 N 个 BlockSize 字节

---

## 4.2 块读取实现

### 4.2.1 读取流程

```
1. 计算虚拟偏移对应的 Block 索引
   block_idx = virtual_offset / BlockSize
   offset_in_block = virtual_offset % BlockSize

2. 查询 BAT 获取块状态
   bat_entry = bat.get_entry(block_idx)

3. 根据状态处理：
   - FULLY_PRESENT: 从 FileOffsetMB 读取
   - ZERO: 返回零填充数据
   - NOT_PRESENT: 固定磁盘 = 错误，动态磁盘 = 返回零
   - PARTIALLY_PRESENT: 查 Sector Bitmap，决定从当前文件或父磁盘读取

4. 读取数据并返回
```

### 4.2.2 跨块读取

如果读取范围跨越多个 Block:
- 对每个涉及的 Block 分别处理
- 合并结果返回

### 实现任务

- [ ] 实现单块读取函数
- [ ] 实现跨块读取函数
- [ ] 处理 ZERO 状态（返回零填充）
- [ ] 处理 NOT_PRESENT 状态（动态磁盘）

---

## 4.3 块写入实现

### 4.3.1 固定磁盘 (Fixed VHDX)

**特点**: 所有块在创建时已分配，状态始终为 FULLY_PRESENT

**写入流程**:
```
1. 计算 Block 索引
2. 从 BAT 获取 FileOffsetMB
3. 直接写入文件偏移
4. （可选）Flush 确保写入稳定
```

### 4.3.2 动态磁盘 (Dynamic VHDX)

**特点**: 块按需分配，文件大小动态增长

**写入流程**:
```
1. 计算 Block 索引
2. 查询 BAT 获取当前状态
3. 如果状态为 NOT_PRESENT/ZERO/UNMAPPED：
   a. 分配新的文件空间（1MB 对齐）
   b. 更新 BAT Entry 为 FULLY_PRESENT
   c. 通过日志记录 BAT 更新（元数据更新）
4. 从 BAT 获取 FileOffsetMB
5. 写入数据到文件偏移
```

### 4.3.3 块分配器

**职责**:
- 管理文件内空闲空间
- 分配新块时 1MB 对齐
- 动态扩展文件大小

**分配策略**:
```rust
struct BlockAllocator {
    file_size: u64,
    next_free_offset: u64,  // 下一个空闲偏移（1MB 对齐）
}

impl BlockAllocator {
    fn allocate_block(&mut self, block_size: u64) -> u64 {
        let offset = self.next_free_offset;
        self.next_free_offset += align_up(block_size, 1 << 20);
        
        // 如果需要，扩展文件大小
        if self.next_free_offset > self.file_size {
            self.extend_file(self.next_free_offset);
        }
        
        offset
    }
}
```

### 实现任务

- [ ] 实现块分配器
- [ ] 实现动态块分配
- [ ] 实现文件动态扩展
- [ ] 实现固定磁盘写入
- [ ] 实现动态磁盘写入（按需分配）

---

## 4.4 BAT 更新与日志

**关键规则**: BAT 是元数据，更新必须通过日志

### 4.4.1 更新 BAT Entry 流程

```
1. 准备新的 BAT Entry 值
   - State = FULLY_PRESENT
   - FileOffsetMB = 分配的偏移 / 1MB

2. 创建 Data Descriptor
   - FileOffset = BAT Entry 在文件中的位置
   - Data = 新的 Entry 值（8 字节）

3. 写入 Log Entry
   - 包含 Data Descriptor

4. Flush 日志

5. 应用更新到 BAT Entry 的最终位置

6. Flush BAT 区域
```

### 实现任务

- [ ] 实现 BAT Entry 更新（通过日志）
- [ ] 确保 BAT 更新原子性

---

## 4.5 缓存层（可选优化）

### 4.5.1 块缓存

**目的**: 减少磁盘 I/O，提高性能

**策略**:
- LRU (Least Recently Used) 缓存
- 缓存热点 Block
- 写回（Write-back）或直写（Write-through）

### 4.5.2 预读

**策略**:
- 顺序读取时预加载下一个 Block
- 提高顺序读性能

### 实现任务（可选）

- [ ] 实现 LRU 块缓存
- [ ] 实现写回/直写策略
- [ ] 实现预读优化

---

## 4.6 块大小和扇区大小处理

### 4.6.1 支持的配置

| Block Size | Logical Sector Size | Physical Sector Size |
|------------|---------------------|----------------------|
| 1MB - 256MB | 512B | 512B |
| 1MB - 256MB | 512B | 4096B |
| 1MB - 256MB | 4096B | 4096B |

### 4.6.2 对齐要求

- 所有文件结构必须 1MB 对齐
- 块写入不需要扇区对齐（VHDX 内部处理）

### 实现任务

- [ ] 测试不同 Block Size 组合
- [ ] 测试不同扇区大小组合
- [ ] 验证 1MB 对齐

---

## 4.7 测试场景

### 4.7.1 读取测试
- [ ] 读取固定磁盘任意偏移
- [ ] 读取动态磁盘已分配块
- [ ] 读取动态磁盘未分配块（应返回零）
- [ ] 跨块读取

### 4.7.2 写入测试
- [ ] 写入固定磁盘
- [ ] 写入动态磁盘新块（触发分配）
- [ ] 写入动态磁盘已有块
- [ ] 跨块写入
- [ ] 大文件写入（> 4GB）

### 4.7.3 电源故障测试
- [ ] 写入过程中断电模拟
- [ ] 验证日志重放后数据一致性

---

## Phase 4 完成标准

- [ ] 块读取（固定 + 动态）
- [ ] 块写入（固定 + 动态）
- [ ] 动态块分配
- [ ] BAT 更新（通过日志）
- [ ] 通过所有读取/写入测试
- [ ] 通过电源故障测试

## 下一步

完成 Phase 4 后，进入 **Phase 5: 差异磁盘**，实现 Parent Locator 和 Sector Bitmap 支持。
