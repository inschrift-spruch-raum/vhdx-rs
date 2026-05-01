# 计划一致性验收报告（Task 17）

日期：2026-05-01  
范围：`.sisyphus/plans/api-alignment-full.md` 中 Task 1 到 Task 16 的结果汇总与一致性验收

## 1. 验收结论摘要

- 主要差异项已闭环，状态为“已修复”或“按决策保留且已文档对齐”。
- strict 语义、BAT 参数化、API 形态与文档一致性、UB 安全边界、质量门禁均有证据链。
- 当前无阻塞性交付风险，可作为 Task 18/19 的直接输入。

## 2. 主要差异项映射（状态, 理由, 证据）

### 2.1 strict 模式语义（Standard §3.1）

- 状态：**已修复**。
- 最终语义：
  - `strict=true` 拒绝 required unknown 与 optional unknown。
  - `strict=false` 仅放宽 optional unknown，required unknown 仍拒绝。
- 理由：按标准要求执行“仅放宽 optional unknown”，且保持错误类型契约稳定。
- 证据：
  - `.sisyphus/evidence/task-3-strict-optional.txt`
  - `.sisyphus/evidence/task-3-strict-required-error.txt`
  - `.sisyphus/evidence/task-10-strict-matrix.txt`
  - `.sisyphus/evidence/task-10-strict-matrix-error.txt`

### 2.2 BAT chunk ratio 真实参数化

- 状态：**已修复**。
- 最终语义：`Bat::new` 不再使用默认常量硬编码，改为使用真实 `logical_sector_size + block_size` 进行 chunk ratio 计算，并完成调用链透传。
- 理由：该项是数据正确性核心差异，修复后与读写路径参数计算一致。
- 证据：
  - `.sisyphus/evidence/task-2-bat-chunk-ratio.txt`
  - `.sisyphus/evidence/task-7-bat-signature.txt`
  - `.sisyphus/evidence/task-11-bat-nondefault.txt`
  - `.sisyphus/evidence/task-11-bat-nondefault-error.txt`

### 2.3 API 形态与文档契约一致性（SpecValidator / HeaderStructure::create / 导出路径）

- 状态：**已对齐（文档校准为主）**。
- 最终语义：
  - `SpecValidator` 生命周期语义在文档中显式化。
  - `HeaderStructure::create` 文档明确为序列化字节产物语义。
  - 示例导入路径与当前 crate 路径一致。
- 理由：实现已稳定且通过回归，按“实现正确优先，文档对齐”策略避免不必要 API 扰动。
- 证据：
  - `.sisyphus/evidence/task-6-api-shape.txt`
  - `.sisyphus/evidence/task-6-api-shape-build.txt`
  - `.sisyphus/evidence/task-12-validator-parity.txt`
  - `.sisyphus/evidence/task-14-header-create-doc.txt`
  - `.sisyphus/evidence/task-15-doc-examples.txt`

### 2.4 Error 映射关系（计划集合 vs 实现超集）

- 状态：**已对齐（保留实现超集）**。
- 最终语义：文档明确“计划核心变体 + 实现扩展变体”的兼容关系，不回退实现细粒度诊断能力。
- 理由：扩展变体被真实使用，删除会带来兼容与诊断风险。
- 证据：
  - `.sisyphus/evidence/task-8-error-map.txt`
  - `.sisyphus/evidence/task-8-error-map-error.txt`

### 2.5 UB 结论可重复性

- 状态：**已固化**。
- 最终结论：未发现 Rust 语言层 UB 证据，唯一 unsafe 入口已由边界回归测试锁定不变量。
- 理由：通过针对 malformed/log-entry 边界的自动化用例，验证 unsafe 前置条件未被破坏。
- 证据：
  - `.sisyphus/evidence/task-5-ub-safety.txt`
  - `.sisyphus/evidence/task-5-ub-safety-error.txt`

### 2.6 日志策略边界（Require/Auto/InMemoryOnReadOnly/ReadOnlyNoReplay）

- 状态：**已固化**。
- 最终语义：四策略在只读/可写组合下行为可复现，且与计划要求一致。
- 理由：策略矩阵属于关键行为边界，已补齐回归并纳入总体验证。
- 证据：
  - `.sisyphus/evidence/task-4-policy-require-and-auto.txt`
  - `.sisyphus/evidence/task-4-policy-readonly-structure.txt`
  - `.sisyphus/evidence/task-4-policy-writable-error.txt`

### 2.7 质量门禁

- 状态：**已通过**。
- 最终结论：`cargo test --workspace`、`cargo clippy --workspace`、`cargo fmt --check` 最终全绿。
- 理由：Task 16 已记录一次格式失败后修复并复跑通过，属于格式化差异非逻辑问题。
- 证据：
  - `.sisyphus/evidence/task-16-quality-gates.txt`
  - `.sisyphus/evidence/task-16-quality-gates-error.txt`

## 3. 覆盖闭环与证据完整性

- 测试映射矩阵已建立，关键差异项均可回链到自动化测试集合。
- 本报告所有关键结论均引用 `.sisyphus/evidence/` 下具体文件，无“仅口头结论”。
- 证据矩阵主索引：
  - `.sisyphus/evidence/task-9-test-matrix.txt`
  - `.sisyphus/evidence/task-9-test-matrix-error.txt`

## 4. 残余偏差与保留项

- 保留项 A：API 形态差异采取“文档改而非代码改”。
  - 判定：**可接受保留**。
  - 依据：实现稳定且验证充分，文档已完成契约对齐。
  - 证据：Task 6/12/14/15 对应证据文件。

- 保留项 B：Error 枚举保持实现超集，不回退到计划最小集合。
  - 判定：**可接受保留**。
  - 依据：扩展变体为真实诊断语义，兼容性通过“core + extension”解释对齐。
  - 证据：Task 8 对应证据文件。

## 5. 环境与工具约束

- `lsp_diagnostics` 在本环境不可用（缺少 rust-analyzer/相关依赖），因此本次验收以 Rust 原生命令门禁与直接文件证据审阅为准。
- 上述约束已在前序任务中记录，并未影响 `cargo test/clippy/fmt` 门禁闭环。
- 证据：
  - `.sisyphus/notepads/api-alignment-full/issues.md`
  - `.sisyphus/evidence/task-15-doc-examples.txt`
  - `.sisyphus/evidence/task-16-quality-gates.txt`

## 6. 最终验收意见

- 对照 Task 17 目标，strict 语义、BAT 参数化、API 形态/文档一致性、UB 结论、质量门禁与残余偏差说明均已覆盖。
- 结论：**Task 17 验收通过**，可进入 Task 18/19 的质量与作用域复核阶段。
