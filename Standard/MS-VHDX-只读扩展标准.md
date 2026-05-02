# MS-VHDX 只读扩展标准

> 基线：MS-VHDX v20240423  
> 作用域：API 行为层（只读打开与日志回放策略），不修改 on-disk 格式。  

---

## 1. 范围

本文定义“只读扩展”的完整行为约束：

- `OpenOptions::log_replay(policy)` 策略语义；
- 含 pending log 时 `finish()` 的行为；
- 结构面与数据面在不同策略下的一致性边界；
- 只读场景下的内存回放约束。

---

## 2. 术语

- **可回放日志**：`LogGuid != 0` 且存在有效 active sequence。  
- **结构面**：Header / Region / BAT / Metadata / Log 结构读取。  
- **数据面**：虚拟扇区（payload）读取结果的一致性语义。  
- **只读打开**：未启用写权限。

本文 `MUST/SHOULD/MAY` 采用 RFC2119 语义；如与 MS-VHDX §1.7 + §2 冲突，以 MS-VHDX 为准。

---

## 3. 默认策略约束

- 若调用方未显式设置 `log_replay(...)`，实现 **MUST** 采用 `LogReplayPolicy::Require`。

---

## 4. LogReplayPolicy 规范

### 4.1 `Require`

1. 若检测到可回放日志，`finish()` **MUST** 返回 `LogReplayRequired`。  
2. 在该策略下，实现 **MUST NOT** 隐式执行日志回放。  
3. 未满足回放前置条件时，**MUST NOT** 进入数据面读取路径。

### 4.2 `Auto`

1. 打开阶段实现 **MUST** 自动执行日志回放流程。  
2. 回放失败时，`finish()` **MUST** 失败。  
3. 在只读打开场景中，回放 **MUST** 采用内存语义（不得写回底层文件）。

### 4.3 `InMemoryOnReadOnly`

1. 该策略 **仅允许** 用于只读打开。  
2. 若在可写打开中触发该策略处理 pending log，实现 **MUST** 返回参数错误并拒绝打开。  
3. 实现 **MUST** 在内存中构建“回放后视图”，且 **MUST NOT** 写回底层文件。  
4. 结构面读取与数据面读取 **MUST** 基于该回放后视图。

### 4.4 `ReadOnlyNoReplay`

1. 该策略 **仅允许** 用于只读打开。  
2. 实现 **MAY** 跳过日志回放并允许 `finish()` 成功。  
3. 实现 **MUST** 明确声明：该模式仅保证结构面读取，**不保证** 数据面一致性。  
4. 若调用方进入数据面读取，结果 **MAY** 与“完成回放后的状态”不一致。

---

## 5. 与读取路径的耦合约束

1. `Require`：未完成回放语义前，数据面读取 **MUST** 被阻断。  
2. `Auto` / `InMemoryOnReadOnly`：数据面读取 **MUST** 基于“已回放语义”。  
3. `ReadOnlyNoReplay`：结构面可读；数据面可读但一致性不受保证，调用方需显式承担该风险。

---

## 6. 错误语义（Log 相关）

1. 在 `Require` 下检测到可回放日志时，**MUST** 返回可区分错误 `LogReplayRequired`。  
2. 日志回放失败（`Auto` / `InMemoryOnReadOnly`）时，**MUST** 返回可区分的日志错误（如回放失败、条目损坏、active sequence 非法）。  
3. 策略与打开模式冲突（如可写 + `InMemoryOnReadOnly` 或可写 + `ReadOnlyNoReplay`）时，**MUST** 返回参数/模式错误并拒绝打开。

---

## 7. 最小合规清单（Log）

- [ ] 默认策略为 `Require`。  
- [ ] `Require` 下遇到可回放日志返回 `LogReplayRequired`，且不隐式回放。  
- [ ] `Auto` 在打开阶段自动回放，失败即打开失败。  
- [ ] `InMemoryOnReadOnly` 仅限只读、仅内存回放、不落盘。  
- [ ] `ReadOnlyNoReplay` 明确标注“仅结构面保证”。  
- [ ] 不同策略下数据面读取前置条件与一致性边界可被测试验证。
