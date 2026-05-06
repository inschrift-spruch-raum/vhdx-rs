# MS-VHDX 校验扩展标准（Validation Issue 编码与 spec_ref 字典）

> 基线：MS-VHDX v20240423  
> 作用域：`SpecValidator` 校验错误的标准化编码与 `spec_ref` 格式。  
> 目的：为 `ValidationIssue` 提供统一的错误码命名规范与 MS-VHDX 章节引用字典，确保跨实现的可审计一致性。

---

## 1. 范围

本文定义如下内容：

- `spec_ref` 字符串的编码格式；
- `code` 错误码的命名规范；
- 错误码 → `spec_ref` 的完整映射字典；
- `ValidationIssue` 各字段的生成规则。

---

## 2. 术语

- **code**：错误码字符串，形如 `BAT_ENTRY_INVALID_STATE`。
- **spec_ref**：标准章节引用字符串，形如 `MS-VHDX/2.5.1.1`。
- **section**：校验阶段名称，形如 `bat`、`log`、`header`。

---

## 3. 编码规范

### 3.1 `spec_ref` 格式

```
MS-VHDX/{chapter}.{section}[.{subsection}[.{sub}]]
```

规则：

1. 前缀固定为 `MS-VHDX`（基线规范）。
2. 若错误定义在扩展标准中，前缀替换为标准文件名（不含 `.md`）：
   - `MS-VHDX-只读扩展标准` → `ROEXT`
   - `MS-VHDX-宽松扩展标准` → `RELAX`
   - `MS-VHDX-校验扩展标准` → `VALEXT`
3. 章节号与 MS-VHDX 原始规范的章节编号严格一致（§2.3.1.1 → `MS-VHDX/2.3.1.1`）。
4. 无合适细粒度章节时，使用最近父章节（如 `MS-VHDX/2.5`）。

### 3.2 `code` 命名规范

```
{SECTION}_{ISSUE}
```

规则：

1. `SECTION` 为大写缩写，对应校验阶段：
   - `HEADER` — validate_header
   - `REGION` — validate_region_table
   - `BAT` — validate_bat
   - `METADATA` — validate_metadata
   - `METADATA_REQUIRED` — validate_required_metadata_items
   - `LOG` — validate_log
   - `LOG_SEQUENCE` — 日志 sequence 层级错误
   - `PARENT_LOCATOR` — validate_parent_locator
    - `GENERAL` — 跨阶段的通用约束
2. `ISSUE` 为具体问题描述，以 `_` 分隔，全大写。
3. 同一校验阶段内的 code 必须唯一。

### 3.3 `section` 命名

全小写，对应校验方法去 `validate_` 前缀：

| section 值 | 对应方法 |
|---|---|
| `header` | `validate_header()` |
| `region_table` | `validate_region_table()` |
| `bat` | `validate_bat()` |
| `metadata` | `validate_metadata()` |
| `metadata_required` | `validate_required_metadata_items()` |
| `log` | `validate_log()` |
| `parent_locator` | `validate_parent_locator()` |
| `file` | `validate_file()`（顶层） |

---

## 4. 错误码 → spec_ref 字典

### 4.1 Header 校验

| code | spec_ref | 说明 |
|---|---|---|
| `HEADER_FILE_TYPE_ID_INVALID` | `MS-VHDX/2.2.1` | File Type Identifier 签名非 `vhdxfile` |
| `HEADER_SIGNATURE_INVALID` | `MS-VHDX/2.2.2` | Header Structure 签名非 `head` |
| `HEADER_CHECKSUM_MISMATCH` | `MS-VHDX/2.2.2` | Header CRC-32C 校验和不匹配 |
| `HEADER_VERSION_UNSUPPORTED` | `MS-VHDX/2.2.2` | Header Version != 1 |
| `HEADER_LOG_VERSION_UNSUPPORTED` | `MS-VHDX/2.2.2` | Header LogVersion != 0（规范要求 LogVersion MUST be 0；若 LogGuid 非零则不得继续处理） |
| `HEADER_LOG_GUID_MISMATCH` | `MS-VHDX/2.2.2` | 双 Header LogGuid 不一致 |
| `HEADER_SEQUENCE_NUMBER_INVALID` | `MS-VHDX/2.2.2` | 序列号异常（无法选择 active header） |
| `HEADER_LOG_LENGTH_NOT_ALIGNED` | `MS-VHDX/2.2.2` | LogLength 不是 1MB 的整数倍 |
| `HEADER_LOG_OFFSET_NOT_ALIGNED` | `MS-VHDX/2.2.2` | LogOffset 不是 1MB 的整数倍 |
| `HEADER_CORRUPTED` | `MS-VHDX/2.2.2` | 无法归类到具体错误码时的通用 Header 数据损坏 |

### 4.2 Region Table 校验

| code | spec_ref | 说明 |
|---|---|---|
| `REGION_CHECKSUM_MISMATCH` | `MS-VHDX/2.2.3.1` | Region Table Header CRC-32C 校验和不匹配 |
| `REGION_ENTRY_ALIGNMENT` | `MS-VHDX/2.2.3.2` | Region Table Entry FileOffset 未按 1MB 对齐 |
| `REGION_ENTRY_OFFSET_MINIMUM` | `MS-VHDX/2.2.3.2` | Region Table Entry FileOffset 必须 >= 1MB |
| `REGION_ENTRY_OVERLAP` | `MS-VHDX/2.1` | Region 区间重叠/越界 |
| `REGION_ENTRY_COUNT_EXCEEDS_MAXIMUM` | `MS-VHDX/2.2.3.1` | Region table entry count 超过 2047 |
| `REGION_OPTIONAL_UNKNOWN` | `RELAX` | optional unknown region GUID 在严格模式下被拒绝 |
| `REGION_REQUIRED_UNKNOWN` | `RELAX` | required unknown region 存在且 strict=true |
| `REGION_SIGNATURE_INVALID` | `MS-VHDX/2.2.3.1` | Region Table Header 签名非 `regi` |

### 4.3 BAT 校验

| code | spec_ref | 说明 |
|---|---|---|
| `BAT_ENTRY_COUNT_INSUFFICIENT` | `MS-VHDX/2.5` | BAT entry 数量不足以覆盖虚拟磁盘范围 |
| `BAT_ENTRY_FILE_OFFSET_UNALIGNED` | `MS-VHDX/2.5` | Payload 块偏移未按块大小对齐 |
| `BAT_ENTRY_INVALID_STATE` | `MS-VHDX/2.5.1.1` | BAT entry 状态值未定义 |
| `BAT_ENTRY_STATE_MISMATCH` | `MS-VHDX/2.5.1.1` | 状态与磁盘类型不匹配（如非差分盘出现 `PartiallyPresent`，或非差分盘 SectorBitmap 状态为 `Present`） |
| `BAT_FILE_OFFSET_DUPLICATE` | `MS-VHDX/2.5` | BAT 中多个 entry 指向同一 file_offset_mb |
| `BAT_SECTOR_BITMAP_INVALID_STATE` | `MS-VHDX/2.5.1.2` | Sector Bitmap entry 状态值未定义 |

### 4.4 Metadata 校验

| code | spec_ref | 说明 |
|---|---|---|
| `METADATA_ENTRY_INVALID` | `MS-VHDX/2.6.1.2` | Table Entry 格式异常（offset/length 越界） |
| `METADATA_ENTRY_OFFSET_MINIMUM` | `MS-VHDX/2.6.1.2` | metadata entry offset 必须 >= 64KB |
| `METADATA_GUID_UNKNOWN` | `MS-VHDX/2.6.2` | 未知 Metadata Item GUID |
| `METADATA_ITEMS_OVERLAP` | `MS-VHDX/2.6.2` | metadata items 区间相互重叠 |
| `METADATA_ITEM_CORRUPTED` | `MS-VHDX/2.6.2` | Metadata Item 数据损坏 |
| `METADATA_OPTIONAL_UNKNOWN` | `RELAX` | optional unknown metadata item 在严格模式下被拒绝（非严格模式下 MUST 忽略，不产生此 code） |
| `METADATA_REQUIRED_MISSING` | `RELAX` | required metadata item 缺失 |
| `METADATA_REQUIRED_UNKNOWN` | `RELAX` | required unknown metadata item 存在 |
| `METADATA_RESERVED_FLAGS_SET` | `MS-VHDX/2.6.1.2` | metadata entry 的保留标志位被设置 |
| `METADATA_TABLE_SIGNATURE_INVALID` | `MS-VHDX/2.6.1.1` | Metadata Table Header 签名非 `metadata` |
| `METADATA_ENTRY_RESERVED_NONZERO` | `MS-VHDX/2.6.1.2` | Metadata entry reserved 字段不为 0 |
| `METADATA_FILE_PARAMETERS_RESERVED_FLAGS` | `MS-VHDX/2.6.2.1` | FileParameters 保留标志位（bits 2-31）被设置 |

### 4.5 Log 校验

| code | spec_ref | 说明 |
|---|---|---|
| `LOG_SIGNATURE_INVALID` | `MS-VHDX/2.3.1.1` | Entry Header 签名非 `loge` |
| `LOG_ENTRY_CHECKSUM_MISMATCH` | `MS-VHDX/2.3.1.1` | 日志条目 CRC-32C 错误 |
| `LOG_ENTRY_LENGTH_INVALID` | `MS-VHDX/2.3.1.1` | EntryLength 非 4KB 倍数或越界 |
| `LOG_ENTRY_TAIL_INVALID` | `MS-VHDX/2.3.1.1` | Tail 非 4KB 倍数或越界 |
| `LOG_DESCRIPTOR_SIGNATURE_INVALID` | `MS-VHDX/2.3.1` | Descriptor 签名非 `desc`（Data Desc, §2.3.1.3）或 `zero`（Zero Desc, §2.3.1.2） |
| `LOG_DESCRIPTOR_COUNT_MISMATCH` | `MS-VHDX/2.3.1` | DescriptorCount 与实际数量不符 |
| `LOG_SEQUENCE_GAP` | `MS-VHDX/2.3.2` | sequence 内相邻 entry 的 SequenceNumber 不连续 |
| `LOG_SEQUENCE_GUID_MISMATCH` | `MS-VHDX/2.3.2` | Entry LogGuid 与 Header LogGuid 不一致 |
| `LOG_ACTIVE_SEQUENCE_EMPTY` | `MS-VHDX/2.3.3` | 候选活跃序列为空（日志损坏） |
| `LOG_DATA_SECTOR_INVALID` | `MS-VHDX/2.3.1.4` | Data Sector 签名/SequenceHigh 异常 |
| `LOG_REPLAY_REQUIRED` | `ROEXT` | 存在可回放日志。在打开路径上为阻断错误（LogReplayRequired），详见 MS-VHDX-只读扩展标准 §4.1；在校验路径中为状态提示（非阻断性），调用方应按上下文决定处理方式 |

### 4.6 Parent Locator 校验

| code | spec_ref | 说明 |
|---|---|---|
| `PARENT_LOCATOR_MISSING_LINKAGE` | `MS-VHDX/2.6.2.6.3` | parent_linkage key 不存在（规范要求 MUST be present） |
| `PARENT_LOCATOR_LINKAGE2_CONFLICT` | `MS-VHDX/2.6.2.6.3` | parent_linkage2 存在（规范要求 parent_linkage2 can't be present）。注：Microsoft 产品实现在 merge 过渡期会暂时写入该字段，本扩展标准将其判定为冲突。因此不支持校验 merge 过渡期的 VHDX 文件，参见 MS-VHDX.md重点解读.md §8 |
| `PARENT_LOCATOR_NO_VALID_PATH` | `MS-VHDX/2.6.2.6.3` | relative_path / volume_path / absolute_win32_path 均不可访问（按规范顺序解析：relative_path → volume_path → absolute_win32_path） |
| `PARENT_LOCATOR_GUID_MISMATCH` | `MS-VHDX/2.6.2.6` | 子盘 parent_linkage 记录的父盘 DataWriteGuid 与实际父盘不一致。注：该错误由 `validate_parent_locator()` 在父磁盘可访问时触发（合并了结构校验与运行时链校验），区别于单纯的 `PARENT_LOCATOR_NO_VALID_PATH`（路径不可达） |
| `PARENT_LOCATOR_FORMAT_ERROR` | `VALEXT` | Parent Locator 内部 KeyValueEntry 解析失败（UTF-16LE 解码或数据切片越界）。专用于 `KeyValueEntry::key()` 和 `value()` 运行时解析错误，区别于 `InvalidMetadata`（ Metadata Table / Entry 的格式错误） |

---

## 5. 生成规则

### 5.1 `ValidationIssue` 各字段填充规则

- `section`：直接使用 §3.3 的 section 值。
- `code`：使用 §4 字典中的 code。
- `message`：人类可读描述，**SHOULD** 包含关键上下文值（如期望值/实际值）。
- `spec_ref`：使用 §4 字典中的 spec_ref。

### 5.2 与 `Error` 枚举的关系

`ValidationIssue` 是 `validate_*` 系列方法返回的结构化校验问题。
`Error` 枚举是 API 的公开错误面。两者关系：

- `validate_*` 方法返回 `Result<Vec<ValidationIssue>>`，`Ok` 中包含所有发现的校验问题列表（空 vec 表示完全合规）。
- `spec_ref` 字段是 `ValidationIssue` 独有的，`Error` 枚举不携带标准引用。
- `Error::Err` 变体用于阻断性错误（如文件损坏、签名错误，无法继续解析），而 `ValidationIssue` 用于不阻断的可报告问题。

---

## 6. 一致性要求

1. 新增校验错误时，**MUST** 在 §4 字典中注册对应的 code 和 spec_ref。
2. spec_ref **MUST** 引用 MS-VHDX 规范中最近的有效章节；无法映射时使用 `VALEXT/{章节}`。
3. 同一 `Err` 条件不得对应多个 code（一对一映射）。
4. section 值与 `SpecValidator` 方法名 **MUST** 保持对应关系。

---

## 7. 最小合规清单

- [ ] spec_ref 格式为 `MS-VHDX/{章节号}` 或 `{扩展标准}/{章节号}`。
- [ ] 所有 `validate_*` 方法可校验的错误均在 §4 字典中注册。
- [ ] code 命名符合 `{SECTION}_{ISSUE}` 规范。
- [ ] 新增校验错误时同步更新字典。
