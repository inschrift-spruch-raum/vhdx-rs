# MS-VHDX 重点解读（面向实现）

> 基于 `docs/Standard/MS-VHDX.md` 提炼。本文重点不是逐段翻译，而是帮助实现者快速抓住 **必须遵守** 的格式约束、打开流程和易错点。

---

## 1. 先记住：哪些内容是“规范性要求”

原文明确指出：**1.7 与 2 章是规范性（normative）**，其余多为说明性内容。  
实现上可理解为：

- 章节 2（Structures）里的 `MUST/MUST NOT` 是硬约束；
- 图示、示例可帮助理解布局，但不能替代字段约束与状态机约束。

---

## 2. VHDX 的核心设计目标（为什么这样设计）

VHDX 相比早期格式，核心是：

- **大容量**：虚拟盘最大到 64 TB；
- **现代扇区支持**：逻辑/物理扇区可到 4 KB；
- **可调块大小**：1 MB ~ 256 MB，2 的幂；
- **崩溃恢复能力**：通过日志（Log）保障元数据更新的一致性；
- **差分链支持**：通过 BAT + sector bitmap + parent locator。

这意味着实现重点是：**一致性优先于性能；恢复优先于“侥幸可读”**。

---

## 3. 全局硬约束（实现时最容易漏）

1. **字节序**：除特别声明外，均为小端。  
2. **CRC**：默认使用 CRC-32C（Castagnoli，多项式 0x1EDC6F41）。  
3. **对齐**：文件中主要对象按 **1 MB** 对齐；日志项按至少 **4 KB** 对齐。  
4. **不重叠**：BAT、Metadata、Log、Payload、Sector Bitmap 等必须互不重叠。  
5. **未知必需对象处理**：
   - Region 或 Metadata 标记 `IsRequired=True` 且实现不认识 → **必须拒绝打开**。

---

## 4. 打开文件的最小正确流程（建议作为解析主流程）

### Step A：验证 Header Section（前 1MB）

- File Type Identifier 签名必须为 `"vhdxfile"`；
- 两个 Header（64KB 与 128KB 偏移）分别校验：
  - Signature=`"head"`
  - CRC 正确
- 选“当前 Header”规则：
  - 仅一个有效就用它；
  - 两个都有效取 `SequenceNumber` 更大者；
  - 都无效则文件损坏。

### Step B：读取 Region Table

- 校验 `"regi"` + CRC；
- 每个 entry 检查：1MB 对齐、长度 1MB 倍数、不重叠；
- 必须能识别并定位两个 required region：
  - BAT region
  - Metadata region。

### Step C：决定是否执行 Log Replay

- 若 Header 的 `LogGuid==0`，视为无有效日志；
- 否则必须扫描日志，找到**最新且完整有效的 active sequence**；
- 不能回放日志则**不得打开文件**（尤其只读场景需内存回放语义）。

### Step D：读取 Metadata，建立虚拟盘参数

至少要拿到 required 的 known items：

- File Parameters（BlockSize、HasParent 等）
- Virtual Disk Size
- Virtual Disk ID
- Logical Sector Size
- Physical Sector Size
- Parent Locator（差分盘时）

### Step E：加载 BAT，并建立虚拟偏移 → 文件偏移映射

- 按 payload/sector-bitmap 交织规则解释 BAT；
- 用 block state 决定读路径（本文件/父链/零/未定义语义）。

---

## 5. Header 三个 GUID 的语义区分（高频误用点）

1. **FileWriteGuid**：表示“文件内容身份”，每次可写打开后首次修改前应更新。  
2. **DataWriteGuid**：表示“用户可见数据身份”，凡是影响虚拟盘读结果的变化都应更新。  
3. **LogGuid**：限定当前日志有效域；log entry 的 LogGuid 不匹配则该项无效。

差分链校验关键依赖 `DataWriteGuid`（通过 parent locator 的 linkage 字段匹配父盘）。

---

## 6. Log 机制：VHDX 一致性的核心

规范要求：**除 Header 外的元数据更新，都必须走日志**。

典型顺序：

1. 把元数据更新写入 Log；
2. Flush Log；
3. 把更新应用到最终位置；
4. Flush 最终位置。

若中途掉电，下次打开通过 replay 恢复。实现上要重点验证：

- Entry Header (`"loge"`) CRC；
- Descriptor 与 Data Sector 的 `SequenceNumber` 一致性；
- Entry `LogGuid` 与 Header `LogGuid` 一致；
- `EntryLength`、`Tail`、4KB 对齐约束。

一句话：**日志不是可选优化，而是元数据写入协议的一部分**。

---

## 7. BAT：读路径决策的状态机

BAT entry = 64 位，核心字段：

- `State`（3 bit）
- `FileOffsetMB`（44 bit，单位 MB）

### 7.1 Payload 常用状态语义

- `FULLY_PRESENT(6)`：数据在本文件，直接按 `FileOffsetMB` 读；
- `PARTIALLY_PRESENT(7)`：仅差分盘有效；需结合 sector bitmap 判定每个 sector 是否在本文件；
- `ZERO(2)`：读全零；
- `NOT_PRESENT(0)` / `UNDEFINED(1)` / `UNMAPPED(3)`：行为有弹性定义，需按规范保证语义边界。

### 7.2 Sector Bitmap 状态

- 固定盘/动态盘：bitmap entry 必须 `SB_BLOCK_NOT_PRESENT(0)`；
- 差分盘：若关联 payload 有 `PARTIALLY_PRESENT`，bitmap 必须 `SB_BLOCK_PRESENT(6)`。

---

## 8. 动态盘 vs 差分盘：实现差异要点

### 动态盘（Dynamic）

- 所有可见数据都在本文件语义中，不需要借助父链；
- BAT 中仍保留 sector bitmap 的“位置位次”（为类型转换兼容），但通常不分配实际 bitmap 块。

### 差分盘（Differencing）

- 读路径可能落到父盘；
- `PARTIALLY_PRESENT + sector bitmap` 是核心；
- `Parent Locator` + `parent_linkage(DataWriteGuid)` 决定父子是否匹配；
- 路径查找顺序：`relative_path -> volume_path -> absolute_win32_path`。

#### parent_linkage / parent_linkage2 实现备注

- 常规稳定态下，只需要 `parent_linkage` 参与匹配，值对应父盘当前 `DataWriteGuid`。
- `parent_linkage2` 主要用于 merge 过渡期的安全切换，不是常规读路径下的主匹配字段。
- merge 过渡期之外，`parent_linkage2` 不存在。
- merge 相关更新要按顺序执行：先把新 GUID 写入子盘的 parent identifier，再更新父盘 `DataWriteGuid`。这样可避免出现“父盘 GUID 已变更，但子盘尚未指向新值”的短暂断链窗口。
- 按上面语义理解后，`parent_linkage2` 的描述与常规 `parent_linkage` 检查并不冲突，前者用于过渡安全，后者用于常态匹配。

---

## 9. Metadata 区：表与项的边界规则

Metadata region = 64KB 表 + 可变长 item 区。实现需检查：

- Table 签名 `"metadata"`；
- EntryCount 合法（<=2047）；
- 每个 item 的 `Offset/Length` 落在 metadata region 内、互不重叠；
- `IsRequired=True` 且未知项 → 拒绝打开；
- `Length==0` 时 `Offset` 也必须为 0（表示“存在但为空”）。

---

## 10. 建议的“严格校验清单”（可直接转测试用例）

1. 文件头签名、双 Header CRC 与 current header 选择逻辑；
2. Region table CRC、entry 对齐/长度/重叠检测；
3. Log replay：active sequence 识别、序列连续性、LogGuid 匹配、文件截断检测（`FlushedFileOffset`）；
4. Metadata required 项齐全性与字段范围；
5. BAT 总项数计算与边界访问；
6. Payload/Sector-bitmap 状态组合合法性（按盘类型）；
7. 差分链父盘定位与 `DataWriteGuid` 链接验证。

---

## 11. 实现建议（工程实践角度）

- 把“解析成功”与“语义可用”分层：先结构校验，再构建读写语义；
- 对 `MUST` 失败统一定义错误码，便于定位损坏原因；
- 对日志回放做幂等设计：同一 active sequence 重放不应导致二次破坏；
- 差分链读取时缓存 parent 解析结果，避免每次读都重复路径/ID 校验；
- 对 UNDEFINED/UNMAPPED 等弹性语义，产品侧要明确安全策略（避免历史数据泄露风险）。

---

## 12. 一句话总结

**VHDX 的本质是：用“冗余头 + 日志回放 + BAT 状态机 + 元数据约束”组合出一个可恢复、可扩展、可差分链化的虚拟块设备文件格式。**  
实现成败的关键不在“能读到数据”，而在“掉电后仍能严格按规范恢复到一致状态”。
