# Phase 3: VHDX 日志系统 - Log & Log Replay

**目标**: 实现 VHDX 日志系统的读写和重放机制，确保电源故障时的数据一致性。

**依赖**: Phase 1 (Header 解析)

**参考**: MS-VHDX.md Section 2.3

**重要性**: ⚠️ **这是 VHDX 实现中最关键的部分之一，必须在允许写入前正确实现。**

---

## 3.1 Log 概述

**位置**: 由 Header 的 LogOffset 和 LogLength 指定，1MB 对齐
**结构**: 环形缓冲区，由多个 Log Entry 组成
**用途**: 保证元数据更新的原子性和电源故障恢复

**关键规则**:
- 所有元数据更新（除 Header）必须通过日志
- Payload Block 更新不通过日志
- 日志必须在使用前重放（如果非空）

---

## 3.2 Log Entry 结构 (Section 2.3.1)

**对齐**: 4KB 对齐
**组成**: Entry Header + Descriptors + Data Sectors

### 3.2.1 Entry Header (Section 2.3.1.1)

**数据结构**:
```
Signature (4 bytes):        0x65676F6C ("loge")
Checksum (4 bytes):         CRC-32C（整个 Entry）
EntryLength (4 bytes):      Entry 总长度（4KB 倍数）
Tail (4 bytes):             指向同一序列的起始 Entry 的偏移
SequenceNumber (8 bytes):   递增的序列号（> 0）
DescriptorCount (4 bytes):  描述符数量
Reserved (4 bytes):         0
LogGuid (16 bytes):         必须与 Header.LogGuid 匹配
FlushedFileOffset (8 bytes): 确保稳定的文件大小
LastFileOffset (8 bytes):   所有结构的最大文件偏移
```

### 3.2.2 Zero Descriptor (Section 2.3.1.2)

**数据结构**:
```
ZeroSignature (4 bytes):  0x6F72657A ("zero")
Reserved (4 bytes):       0
ZeroLength (8 bytes):     要清零的长度（4KB 倍数）
FileOffset (8 bytes):     文件偏移（4KB 倍数）
SequenceNumber (8 bytes): 必须匹配 Entry Header
```

### 3.2.3 Data Descriptor (Section 2.3.1.3)

**数据结构**:
```
DataSignature (4 bytes):   0x63736564 ("desc")
TrailingBytes (4 bytes):   数据扇区的后 4 字节
LeadingBytes (8 bytes):    数据扇区的前 8 字节
FileOffset (8 bytes):      文件偏移（4KB 倍数）
SequenceNumber (8 bytes):  必须匹配 Entry Header
```

### 3.2.4 Data Sector (Section 2.3.1.4)

**数据结构**:
```
DataSignature (4 bytes):   0x61746164 ("data")
SequenceHigh (4 bytes):    SequenceNumber 的高 4 字节
Data (4084 bytes):         实际数据（字节 8-4091）
SequenceLow (4 bytes):     SequenceNumber 的低 4 字节
```

**注意**: 数据扇区的前 8 字节和后 4 字节存储在 Data Descriptor 中

---

## 3.3 实现任务

### 3.3.1 Log Entry 解析

- [ ] 定义 LogEntryHeader 结构体
- [ ] 定义 ZeroDescriptor 结构体
- [ ] 定义 DataDescriptor 结构体
- [ ] 定义 DataSector 结构体
- [ ] 实现 Entry 验证（Signature + Checksum）
- [ ] 实现 Descriptor 解析

### 3.3.2 写入日志

**元数据更新流程**:
1. 将元数据更新打包为 Data Descriptor 或 Zero Descriptor
2. 写入 Log Entry 到环形缓冲区
3. Flush 日志确保写入稳定存储
4. 应用更新到最终位置
5. Flush 最终位置

- [ ] 实现 Log Entry 组装
- [ ] 实现环形缓冲区写入
- [ ] 实现日志 Flush
- [ ] 处理日志环绕（wrap around）

### 3.3.3 Log Sequence (Section 2.3.2)

**定义**: 一系列按 SequenceNumber 递增的 Entry，从 Tail 指向的 Entry 到最新的 Entry（Head）

**有效性验证**:
- 每个 Entry 的 SequenceNumber = 前一个 Entry + 1
- 每个 Entry 的 LogGuid 匹配 Header.LogGuid
- Entry 通过 CRC-32C 验证

- [ ] 实现 Sequence 验证
- [ ] 实现 SequenceNumber 连续性检查

---

## 3.4 Log Replay (Section 2.3.3) ⚠️ 关键

**触发条件**: 打开 VHDX 文件时，如果 Header.LogGuid != 0，必须重放日志

**算法步骤**（来自 MS-VHDX 规范）:

```
1. 初始化候选活动序列为空，序列号 = 0
   设置 current_tail = 0, old_tail = 0

2. 设置当前序列为空，head = current_tail，序列号 = 0

3. 验证从当前序列 head 开始的 Log Entry：
   - 验证 Header 所有字段和 Checksum
   - 如果是有效 Entry，且（序列为空 或 SequenceNumber = 当前序列号 + 1）
   - 则将 Entry 加入当前序列，重复步骤 3

4. 如果当前序列的 head Entry 的 Tail 指向序列内的某个 Entry，
   则当前序列有效（形成闭环）

5. 如果当前序列有效且序列号 > 候选活动序列的序列号，
   则更新候选活动序列 = 当前序列

6. 如果当前序列为空或无效：
   - current_tail += 4KB（环绕到 0 如果达到日志大小）
   否则：
   - current_tail = 当前序列的 head
   
   如果 current_tail < old_tail，说明扫描完成，停止
   否则：old_tail = current_tail，返回步骤 2

7. 如果候选活动序列为空，文件损坏，返回错误

8. 如果文件大小 < 候选活动序列 head 的 FlushedFileOffset，
   文件被截断，返回错误

9. 重放候选活动序列：
   - 从 Tail Entry 开始，按顺序重放每个 Entry 的每个 Descriptor
   - Data Descriptor: 写入 Data Sector 到 FileOffset
   - Zero Descriptor: 将 FileOffset 开始的 ZeroLength 字节清零

10. 扩展文件大小到 LastFileOffset
```

### 实现任务

- [ ] 实现 Log Replay 算法
- [ ] 实现 Entry 有效性验证
- [ ] 实现 Sequence 有效性验证
- [ ] 实现 Data Descriptor 重放
- [ ] 实现 Zero Descriptor 重放
- [ ] 实现文件大小扩展

---

## 3.5 关键约束

### 3.5.1 写入前必须更新 Header

**规则**: 每次打开 VHDX 文件准备写入时：
1. 生成新的 LogGuid（非零）
2. 更新 Header
3. 然后才能写入 Log Entry

### 3.5.2 LogGuid 匹配

**规则**: 
- 写入 Log Entry 时，Entry.LogGuid 必须等于 Header.LogGuid
- 重放时，如果 Entry.LogGuid != Header.LogGuid，该 Entry 无效

### 3.5.3 文件大小检查

**规则**: 重放前必须验证文件大小 >= HeadEntry.FlushedFileOffset

---

## 3.6 测试场景

### 3.6.1 正常日志重放
- [ ] 创建包含未重放日志的 VHDX 文件
- [ ] 打开文件时自动重放日志
- [ ] 验证重放后的数据一致性

### 3.6.2 电源故障模拟
- [ ] 模拟写入日志后崩溃（未应用到最终位置）
- [ ] 重新打开文件，验证日志被重放
- [ ] 验证数据正确性

### 3.6.3 日志环绕
- [ ] 测试日志写满后的环绕处理
- [ ] 验证环绕后仍能找到完整序列

---

## Phase 3 完成标准

- [ ] Log Entry 解析和验证
- [ ] 日志写入（环形缓冲区）
- [ ] Log Replay 完整实现
- [ ] 通过电源故障模拟测试
- [ ] 通过日志环绕测试
- [ ] 与 Windows 生成的 VHDX 日志兼容

## 下一步

完成 Phase 3 后，进入 **Phase 4: 块管理**，实现 Payload Block 的读写和动态分配。
