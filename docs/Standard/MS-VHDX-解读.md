# MS-VHDX 标准解读（v20240423）

> 对 `docs/Standard/MS-VHDX.md` 的工程化阅读笔记。目标是：把“规范里不好读、容易误解、但实现必须正确”的部分讲清楚。

---

## 1. 先读结论：实现 VHDX 时最重要的 10 件事

1. **只把 1.7 和第 2 章当作规范性约束（Normative）**；其余章节主要是说明性文本。
2. **全文件多字节字段一律小端（little-endian）**，CRC 一律默认 **CRC-32C（Castagnoli）**。
3. **1MB 对齐是硬约束**：除 Header Section 内部结构外，区域/日志/BAT/元数据/块布局都受 1MB 对齐限制。
4. **Header 有两份，必须选“当前头”**：签名+校验合法，且 `SequenceNumber` 更大者为 current。
5. **Header 不能走日志更新**，但 **Region Table/Metadata 等元数据更新必须走日志**。
6. **打开可写文件时，必须先更新 Header 再改其他位置**（尤其 `FileWriteGuid`）。
7. **`LogGuid == 0` 时禁止回放日志**；非 0 时必须用 `LogGuid` 过滤有效日志条目。
8. **差分盘完整性依赖 `DataWriteGuid` 与 parent locator linkage 校验**。
9. **BAT 状态语义不是“是否有物理块”这么简单**，`NOT_PRESENT / UNDEFINED / ZERO / UNMAPPED` 的读语义不同。
10. **遇到 unknown 且 required 的 Region/Metadata 必须拒绝打开**，不能“尽量兼容”。

---

## 2. 规范边界与术语：哪些是“必须做”

### 2.1 Normative 与 Informative

- 标准明确写了：**1.7 与第 2 章是 normative**。
- 所以实现时，`MUST / MUST NOT / SHOULD` 的判断优先来自这些章节。

### 2.2 MUST / SHOULD 的工程含义

- **MUST**：不满足就是不合规实现。
- **SHOULD**：原则上应满足；不满足需有清晰理由（兼容性、平台限制等）。
- **MAY**：可选行为。

---

## 3. 文件布局解读：静态图背后的动态规则

## 3.1 Header Section（固定 1MB）

Header Section 内含 5 个 64KB 片段：

- File Type Identifier（0KB）
- Header1（64KB）
- Header2（128KB）
- Region Table1（192KB）
- Region Table2（256KB）

**关键点**

- 第一个 64KB（含 `vhdxfile` 签名）创建后不能覆盖写。
- Header 实体仅 4KB，但放在 64KB 对齐槽位中；其余为保留空间。

### 3.2 1MB 对齐到底约束什么

规范含义是：Header Section 之后的对象（Region、Log、Payload/SectorBitmap）允许自由排列，但必须：

- 非重叠
- 1MB 对齐

这意味着：实现不能把“布局顺序”写死为某个固定顺序，只能依赖 Region Table / Header / BAT 解析定位。

---

## 4. Header 机制：为什么是双头，怎么安全更新

### 4.1 current header 选择规则

一个 Header 要“有效”需满足：

- Signature = `"head"`
- CRC-32C 校验正确（校验时 Checksum 字段置 0）

“当前头（current）”判定：

- 仅一份有效 → 该份就是 current
- 两份都有效 → `SequenceNumber` 更大者是 current
- 都无效 → 文件损坏，必须失败

### 4.2 三个 GUID 字段的职责（易混）

- `FileWriteGuid`：标识**文件内容层面**变化（包含元数据/log replay 等）。
- `DataWriteGuid`：标识**用户可见虚拟磁盘数据**变化（影响读结果就要变）。
- `LogGuid`：标识当前日志代；日志条目需与其匹配。

### 4.3 常见误解纠正

- 误解：只有写 payload 才改 `FileWriteGuid`。  
  更正：**任何文件修改前**（可写打开后首次修改）都应更新。

- 误解：移动块位置（物理重排）要改 `DataWriteGuid`。  
  更正：若不改变虚拟读结果，可不改。

### 4.4 Header 更新推荐流程（规范给出的安全思路）

1. 找 current / noncurrent。
2. 内存构建新 Header：`SequenceNumber = current + 1`。
3. 填写字段（首次更新会话时生成新 `FileWriteGuid`）。
4. 计算校验。
5. 覆写 noncurrent 并 flush。

规范还建议再做一次，把两份都同步成新值，提升抗单点损坏能力。

---

## 5. Region Table：扩展兼容性的关键

### 5.1 为什么有两份 Region Table

和 Header 类似，属于冗余设计。差别在于：Region Table 更新应通过日志保护一致性。

### 5.2 IsRequired 的真正含义

对 unknown region：

- `Required = 1`：必须拒绝打开
- `Required = 0`：可忽略，但必须不破坏其内容

这条规则是“向前兼容”的核心。

---

## 6. 日志（Log）：最容易做错的恢复路径

### 6.1 日志覆盖范围

- **必须走日志**：除 Header 外的元数据更新（例如 Region Table、Metadata 等）。
- **禁止走日志**：Payload block 更新。

### 6.2 刷盘顺序是协议语义的一部分

元数据更新的四步顺序不可乱：

1. 写日志
2. flush 日志
3. 应用到最终位置
4. flush 最终位置

没有 flush 语义，就无法保证掉电恢复行为符合规范。

### 6.3 LogEntry 的防撕裂设计（理解 Data Descriptor）

数据扇区里只放中间 4084 字节；前 8 字节和后 4 字节放进 descriptor：

- `LeadingBytes`（8B）
- `TrailingBytes`（4B）

回放时要拼回完整 4KB。这个设计用于提升撕裂写检测能力（配合 sequence split 与 CRC）。

### 6.4 active sequence 选择

不是“取序号最大单条”这么简单，而是要找：

- 有效条目组成
- 连续 `SequenceNumber`
- 头条目 `Tail` 指向序列内
- `LogGuid` 匹配当前 Header

满足这些条件的“完整有效序列”里，选 head sequence number 最大者。

### 6.5 replay 的两个文件尺寸字段（很关键）

- `FlushedFileOffset`：保证“当时至少稳定到这个大小”；若实际文件小于它，视为截断损坏。
- `LastFileOffset`：回放后文件至少扩到此值，确保全部结构可容纳。

---

## 7. BAT 与块状态：语义重于结构

### 7.1 BAT 是“虚拟块 -> 文件位置/状态”映射

BAT entry 64 位：

- 3 位状态
- 17 位保留（0）
- 44 位 `FileOffsetMB`

`FileOffsetMB` 是 **MB 单位偏移**，不是字节偏移。

### 7.2 ChunkRatio 与交错布局

- 一个 chunk 对应若干 payload blocks + 1 个 sector bitmap block entry（逻辑交错）。
- 动态/固定盘虽然通常不分配 sector bitmap block，但 BAT 里保留这些 entry 位置以维持布局稳定。

### 7.3 最易误解的 4 个 payload 状态

- `NOT_PRESENT`：在 dynamic/fixed 中读可返回任意/零/历史值；在 differencing 中应看父盘。
- `UNDEFINED`：内容未定义，可返回任意或特定历史内容。
- `ZERO`：必须返回全零。
- `UNMAPPED`：可返回零或旧内容（有数据泄漏风险，脚注明确提醒）。

工程建议：若追求跨实现稳定性，尽量把读语义收敛到 `ZERO` 或 `FULLY_PRESENT` 这类定义明确状态。

### 7.4 `PARTIALLY_PRESENT` 只允许差分盘

- fixed/dynamic 看到该状态应判不合法。
- differencing 下该状态要求配套 sector bitmap 已分配且有效。

---

## 8. Metadata Region：表和项的约束关系

### 8.1 两层结构

- 64KB Metadata Table（header + entries）
- 若干 metadata items（可变长、无序、可不对齐）

### 8.2 entry 的三个标志位要分清

- `IsUser`：用户元数据还是系统元数据。
- `IsVirtualDisk`：是否在 fork/merge 时按虚拟磁盘语义复制/替换。
- `IsRequired`：unknown 时是否必须失败。

### 8.3 required known items（实现最低集）

至少要正确识别并处理：

- File Parameters
- Virtual Disk Size
- Virtual Disk ID
- Logical Sector Size
- Physical Sector Size
- Parent Locator

---

## 9. Parent Locator：差分链可靠性的核心

### 9.1 LocatorType 不是摆设

实现必须验证自己理解该 `LocatorType`，不能盲读键值。

### 9.2 parent_linkage / parent_linkage2 说明

文本写法略易混：

- `parent_linkage` 必须存在；
- `parent_linkage2` 用于某些链路维护场景（脚注 12 解释了双字段动机）。

工程上应理解为：打开父链时，父盘 Header 的 `DataWriteGuid` 需匹配 linkage 字段之一。

### 9.3 路径解析顺序是协议要求

查找父盘顺序：

1. `relative_path`
2. `volume_path`
3. `absolute_win32_path`

链路成功打开后，需要回写陈旧路径项（stale entries）。

---

## 10. 实现者“模糊点清单”与建议决策

## 10.1 模糊点：NOT_PRESENT/UNDEFINED/UNMAPPED 允许返回“任意数据”

这在安全上有泄漏风险（脚注 11）。

**建议**：默认实现返回零，或仅在受控模式下返回历史数据。

### 10.2 模糊点：SHOULD 级行为如何落地

如“双次 header 同步更新”是 SHOULD，不是 MUST。

**建议**：默认执行 SHOULD；只在明确性能/介质限制时降级，并记录策略。

### 10.3 模糊点：日志对齐“至少 4KB，可更大”

不同宿主介质可能有更大物理扇区。

**建议**：

- 解析端必须兼容 4KB 基线；
- 写入端可按宿主特性扩展对齐，但不要要求读取端具备同等对齐假设。

### 10.4 模糊点：unknown 项是保留还是拒绝

- unknown + required：拒绝。
- unknown + optional：保留并透传，不能破坏。

---

## 11. 一份可执行的“打开文件”流程（建议）

1. 读 File Type Identifier，校验 `vhdxfile`。
2. 读双 Header，选 current；若无 current 失败。
3. 校验 Version / LogVersion 条件。
4. 读/校验 Region Table，解析 BAT 与 Metadata Region。
5. 若日志非空（`LogGuid != 0`），执行 active sequence 搜索与回放。
6. 解析 Metadata 必需项（BlockSize, DiskSize, SectorSize, ParentLocator...）。
7. 建立 BAT 映射与（差分盘）父链关系。
8. 进入正常 I/O 路径（按 BAT 状态与 sector bitmap 解析读写语义）。

---

## 12. 对本仓库实现的直接价值

这份解读可直接作为以下模块的行为检查表：

- `src/sections/header.rs`：双头选择、GUID 更新语义
- `src/sections/log.rs`：active sequence、descriptor/data sector 拼装与 replay
- `src/sections/bat.rs`：状态机与 offset 约束
- `src/sections/metadata.rs`：required metadata 与 Parent Locator 解析
- `src/file.rs`：open/create 生命周期中的 header/log 协议顺序

---

## 13. 附：建议重点回读的规范位置

- 2.2.2 + 2.2.2.1（Header 与更新时序）
- 2.3.1 ~ 2.3.3（日志条目/序列/回放）
- 2.5.1.1（Payload BAT 状态语义）
- 2.6.1.2（Metadata 表项约束）
- 2.6.2.6.3（VHDX Parent Locator）
- Footnotes 2/5/11/12（差分链与日志回放关键补充）

---

如果后续你希望，我可以基于这份解读再补一版：

- **“实现检查清单版”**（逐条可打勾的 MUST/SHOULD checklist）
- **“测试用例设计版”**（每个模糊点对应 1~3 个回归测试）
