# Phase 1: VHDX 基础解析层 - Header Section

**目标**: 实现 VHDX 文件的 Header Section 解析，能够正确识别和验证 VHDX 文件格式。

**参考**: MS-VHDX.md Section 2.2

## 1.1 File Type Identifier (Section 2.2.1)

**位置**: 文件偏移 0，大小 64KB

### 数据结构
```
Signature (8 bytes):  0x7668647866696C65 ("vhdxfile")
Creator (512 bytes):  UTF-16 LE 字符串，可空
Reserved:             填充至 64KB
```

### 实现任务
- [ ] 定义 FileTypeIdentifier 结构体
- [ ] 实现 Signature 验证（"vhdxfile"）
- [ ] 实现 Creator 字段解析（UTF-16 LE）
- [ ] **约束**: 创建后不得覆盖前 64KB

### 验收标准
- [ ] 能正确识别有效的 VHDX 文件
- [ ] 对无效 Signature 返回错误
- [ ] 能读取 Creator 字符串

---

## 1.2 Headers (Section 2.2.2)

**位置**: Header 1 在 64KB，Header 2 在 128KB
**大小**: 各 4KB

### 数据结构
```
Signature (4 bytes):       0x68656164 ("head")
Checksum (4 bytes):        CRC-32C 校验
SequenceNumber (8 bytes):  用于判断当前有效头
FileWriteGuid (16 bytes):  文件内容唯一标识
DataWriteGuid (16 bytes):  用户可见数据标识
LogGuid (16 bytes):        日志有效性标识
LogVersion (2 bytes):      必须为 0
Version (2 bytes):         必须为 1 (VHDX v2)
LogLength (4 bytes):       日志大小（1MB 倍数）
LogOffset (8 bytes):       日志偏移（1MB 对齐）
Reserved (4016 bytes):     必须为 0
```

### 实现任务
- [ ] 定义 VhdxHeader 结构体
- [ ] 实现 CRC-32C 校验计算（Castagnoli 多项式 0x1EDC6F41）
- [ ] 实现 Header 验证（Signature + Checksum）
- [ ] 实现当前 Header 选择逻辑（比较 SequenceNumber）
- [ ] 定义 GUID 相关结构体

### Header 更新逻辑 (Section 2.2.2.1)

**电源故障安全更新流程**:
1. 识别当前头和非当前头
2. 生成新头，SequenceNumber = 当前头 + 1
3. 计算新头 CRC-32C 校验和
4. 写入非当前头位置
5. Flush 确保写入稳定存储
6. （可选）重复更新当前头，确保双头一致

### 实现任务
- [ ] 实现 Header 更新方法
- [ ] 实现双头轮换机制
- [ ] 生成新的 FileWriteGuid（首次修改时）
- [ ] 生成新的 DataWriteGuid（用户数据变更时）

### 验收标准
- [ ] 能正确读取并验证两个 Header
- [ ] 能根据 SequenceNumber 选择当前有效头
- [ ] 能正确计算 CRC-32C 校验
- [ ] 能实现安全的 Header 更新（双头轮换）

---

## 1.3 Region Table (Section 2.2.3)

**位置**: Region Table 1 在 192KB，Region Table 2 在 256KB
**大小**: 各 64KB

### 数据结构

**Region Table Header**:
```
Signature (4 bytes):   0x72656769 ("regi")
Checksum (4 bytes):    CRC-32C
EntryCount (4 bytes):  条目数（≤ 2047）
Reserved (4 bytes):    0
```

**Region Table Entry**:
```
Guid (16 bytes):       区域 GUID
FileOffset (8 bytes):  区域偏移（1MB 倍数，≥ 1MB）
Length (4 bytes):      区域长度（1MB 倍数）
Required (4 bytes):    1 = 必须识别，否则拒绝加载
```

### 已知区域 GUID

| 区域 | GUID | Required |
|------|------|----------|
| BAT | 2DC27766-F623-4200-9D64-115E9BFD4A08 | True |
| Metadata | 8B7CA206-4790-4B9A-B8FE-575F050F886E | True |

### 实现任务
- [ ] 定义 RegionTableHeader 和 RegionTableEntry 结构体
- [ ] 实现 Region Table 验证（Signature + Checksum）
- [ ] 实现 Entry 解析（Guid、FileOffset、Length、Required）
- [ ] 实现 BAT 和 Metadata Region 位置定位
- [ ] 验证区域不重叠且 1MB 对齐

### 验收标准
- [ ] 能正确解析 Region Table
- [ ] 能定位 BAT 和 Metadata Region
- [ ] 对 Required=1 但不认识的区域返回错误
- [ ] 验证区域不重叠

---

## Phase 1 完成标准

- [ ] File Type Identifier 识别实现
- [ ] 双 Header 读取、验证、选择机制
- [ ] Header 安全更新（双头轮换）
- [ ] Region Table 解析
- [ ] 能通过 Windows 生成的 VHDX 文件读取测试
- [ ] 单元测试覆盖率 > 80%

## 下一步

完成 Phase 1 后，进入 **Phase 2: 元数据层**，解析 Metadata Region 和 BAT。
