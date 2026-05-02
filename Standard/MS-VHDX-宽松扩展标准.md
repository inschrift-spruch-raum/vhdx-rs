# MS-VHDX 宽松扩展标准（Strict 策略分离）

> 基线：MS-VHDX v20240423  
> 作用域：API 行为层中的 `strict(strict: bool)` 语义约束。  
> 目的：将“宽松模式”从只读扩展中独立，形成可单独实现与测试的标准。

---

## 1. 范围

本文定义 `strict` 策略的实现约束，覆盖以下内容：

- unknown item 的分类与处理；
- `strict=true` 与 `strict=false` 的行为边界；
- required unknown 的强制失败规则；
- 错误可观测性与最小合规检查项。

---

## 2. 术语

- **unknown item**：实现无法识别的 Region 或 Metadata 项。  
- **required unknown**：unknown 且带 required 语义（必须理解）。  
- **optional unknown**：unknown 且不带 required 语义（可选理解）。

---

## 3. strict 策略规范

### 3.1 `strict = true`（严格模式，默认）

1. 对 `required unknown`，实现 **MUST** 失败并拒绝打开。  
2. 错误输出 **SHOULD** 可定位到具体 section / item。  
3. 实现 **MUST NOT** 以“猜测语义”方式继续处理未知 required 项。

### 3.2 `strict = false`（宽松模式）

1. 对 `optional unknown`，实现 **MAY** 忽略。  
2. 忽略时，实现 **MUST NOT** 破坏其原始解释路径（不得重写语义、不得污染其他已知项解析）。  
3. 对 `required unknown`，实现 **MUST** 仍然失败并拒绝打开。  
4. `strict=false` **MUST NOT** 被解释为“跳过 required 检查”。

---

## 4. 一致性边界

1. `strict` 仅决定“未知项容忍度”，不改变 MS-VHDX 对 required 项的硬约束。  
2. 本扩展 **MUST NOT** 与 MS-VHDX §1.7 + §2 的 required 语义冲突。  
3. 如存在冲突，以 MS-VHDX 原规范为准。

---

## 5. 错误语义建议

1. 对 `required unknown` 的失败，**SHOULD** 返回可区分错误（例如 required unknown / unknown required region / unknown required metadata）。  
2. 对 `optional unknown` 被忽略的情况，**SHOULD** 提供可观测记录（日志或诊断输出），便于排障与审计。

---

## 6. 最小合规清单

- [ ] 默认 `strict=true`。  
- [ ] `strict=true` 时 required unknown 一律失败。  
- [ ] `strict=false` 时 optional unknown 可忽略。  
- [ ] `strict=false` 时 required unknown 仍一律失败。  
- [ ] unknown 处理不影响已知项解析正确性。
