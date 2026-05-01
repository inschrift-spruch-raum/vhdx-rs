# MS-VHDX 只读扩展标准（v20260430）

> 本文定义在 **MS-VHDX v20240423** 基础上的只读实现扩展约束。目标是：在不改变底层 VHDX 规范语义的前提下，给出可执行、可验证、可回归测试的只读 API 行为标准。

---

## 1. 范围与定位

1. 本扩展仅覆盖 **只读打开、结构校验、日志回放策略分流、差分链只读校验**。
2. 本扩展不修改 MS-VHDX 的文件格式定义，不引入新的 on-disk 布局。
3. 本扩展用于约束 API 行为层（open/validate/read-path），不是替代规范正文。

---

## 2. 术语与一致性要求

### 2.1 术语

- **结构面（control/metadata plane）**：Header、Region Table、BAT、Metadata、Log 条目结构。
- **数据面（payload plane）**：虚拟磁盘扇区读结果与可见数据一致性。
- **只读打开**：未启用写权限（例如未调用 `OpenOptions::write()`）。

### 2.2 一致性要求

- 本文中的 `MUST / SHOULD / MAY` 继承 RFC2119 语义。
- 若与 MS-VHDX §1.7 + §2 冲突，以 MS-VHDX 规范为准。

---

## 3. 只读打开扩展（OpenOptions）

### 3.1 `strict(strict: bool)`

- `strict = true`（默认）时：
  - unknown 且 required 的 Region/Metadata item **MUST** 失败。
  - 返回错误应可定位到具体 section/item。
- `strict = false` 时：
  - unknown 且 optional 的项 **MAY** 忽略，但 **MUST NOT** 破坏其原始内容解释路径。
  - unknown 且 required 的项仍 **MUST** 失败（不得放宽）。

### 3.2 `log_replay(policy: LogReplayPolicy)`

默认策略约束：若调用方未显式设置 `log_replay(...)`，实现 **MUST** 采用 `Require`。

支持以下策略：

1. `Require`
   - 若检测到可回放日志（`LogGuid != 0` 且存在有效 active sequence），`finish()` **MUST** 返回 `LogReplayRequired`。
   - 不得在该策略下隐式回放。

2. `Auto`
   - 打开阶段 **MUST** 自动执行日志回放流程。
   - 回放失败 **MUST** 使打开失败。
   - 只读打开时采用内存回放语义，不写回底层文件。

3. `InMemoryOnReadOnly`
   - 仅在只读场景允许。
   - 若以可写方式打开且触发该策略处理 pending log，**MUST** 返回参数错误并拒绝打开。
   - **MUST** 以内存态重建回放视图，不写回底层文件。
   - 结构读取与数据读取均应基于回放后视图。

4. `ReadOnlyNoReplay`
   - 仅在只读场景允许。
   - **MAY** 跳过日志回放并允许打开成功。
   - **MUST** 明确声明：仅保证结构面读取，不保证数据面一致性。

---

## 4. 只读校验扩展（validation 模块）

### 4.1 校验器职责

`SpecValidator` 作为只读校验器，**MUST** 支持以下分项校验：

- `validate_header`
- `validate_region_table`
- `validate_bat`
- `validate_metadata`
- `validate_required_metadata_items`
- `validate_log`
- `validate_parent_locator`
- `validate_parent_chain`
- `validate_file`（总入口，编排上述校验）

### 4.2 错误输出要求

- 校验失败 **SHOULD** 提供结构化错误信息：section、错误码、消息、规范引用。
- 对 required unknown、CRC 失败、active sequence 非法、parent linkage 不匹配等场景，**MUST** 可区分。

---

## 5. 只读数据面约束

### 5.1 IO 入口约束

- 虚拟磁盘数据访问 **MUST** 通过 IO/Sector 路径。
- File 层 **MUST NOT** 提供与 IO 等价的 payload 读写捷径接口。

### 5.2 与日志策略的耦合

- `Require`：未回放即失败，不进入数据面读取。
- `Auto` / `InMemoryOnReadOnly`：进入数据面读取前应基于“已回放语义”。
- `ReadOnlyNoReplay`：
  - 可进入结构面读取；
  - 数据面读取结果 **MAY** 不具备崩溃后一致性保证，调用方需显式接受。

---

## 6. 差分链只读扩展

### 6.1 Parent Locator

- `validate_parent_locator` **MUST** 检查：
  - `parent_linkage` 存在；
  - `parent_linkage2` 按规范语义处理；
  - `relative_path` / `volume_path` / `absolute_win32_path` 至少存在一个。

### 6.2 Parent Chain

- `validate_parent_chain` **MUST** 校验父盘 `DataWriteGuid` 与 linkage 字段一致性。
- 返回结果 **SHOULD** 包含：child、parent、linkage_matched。

---

## 7. 兼容性与实现建议（非规范 MUST）

1. 建议默认 `strict=true`，降低 silent corruption 风险。
2. 建议默认 `log_replay=Require`；在诊断场景可按需显式切换为 `InMemoryOnReadOnly`，其次 `ReadOnlyNoReplay`。
3. 若暴露 CLI，建议在输出中显式标注当前日志策略与一致性级别。

---

## 8. 最小合规清单（可打勾）

- [ ] 只读打开支持 `strict` 与 `log_replay` 策略。
- [ ] `ReadOnlyNoReplay` 明确标注“仅结构面保证”。
- [ ] `InMemoryOnReadOnly` 不落盘。
- [ ] `validate_file` 可覆盖 Header/Region/BAT/Metadata/Log/Parent。
- [ ] required unknown 一律失败。
- [ ] 差分链 linkage 校验可独立执行并返回结构化结果。

---

## 9. 版本信息

- 基线规范：MS-VHDX v20240423
- 扩展文档版本：v20260430
- 适用范围：`docs/plan/API.md` 中定义的只读 API 扩展
