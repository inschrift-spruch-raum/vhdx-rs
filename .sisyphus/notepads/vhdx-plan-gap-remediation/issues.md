
## 2026-04-29 task-1 issues
- 环境限制：`lsp_symbols` 依赖 rust-analyzer，不可用（工具链缺少 `rust-analyzer.exe`）。
- 环境限制：`grep`/`glob` 依赖 `rg`，当前 PATH 缺失，无法使用内容检索快捷工具。
- 风险控制：已采用 `read` 逐文件锚定 + evidence 文本留痕，确保每条结论有 path+symbol 可追溯依据。

## 2026-04-29 task-2 issues
- `IO::write_sectors` 未来若需公开化，签名需改为 `&mut self` 或引入写锁，当前 `&self` 签名在并发写入时不安全。这是 TD-IO-BATCH-OPS 的前置设计约束。
- `IO::read_sectors` 与 `write_sectors` 同为 `pub(crate)` dead code，如果后续清理可一并处理。
- 环境限制同 T1：无 `rg`/`rust-analyzer`，验证采用 `Select-String` + `read` 组合。

## 2026-04-29 task-3 issues
- 环境仍无 g，未使用 grep 工具完成全文检索，改用 read 锚点 + evidence 文本留痕。
- 本任务为文档对齐，不涉及源码，故不执行 cargo build/test，验证聚焦文档一致性与越界防护文件。

## 2026-04-29 task-3 issues
- 环境仍无 rg，未使用 grep 工具完成全文检索，改用 read 锚点 + evidence 文本留痕。
- 本任务为文档对齐，不涉及源码，故不执行 cargo build/test，验证聚焦文档一致性与越界防护文件。

## 2026-04-29 task-4 issues
- 环境限制同前：无 `rg`/`rust-analyzer`，使用 `Select-String` + `Read` 组合验证路径有效性。
- 不存在独立的"差异报告"文件（T7 最终报告尚未生成），因此无文件需要做措辞修正。证据文件作为替代载体。
- 18 条源码路径全部通过行级内容核对，路径有效率 100%。

## 2026-04-29 task-5 issues
- 环境限制同前：无 `rg`/`rust-analyzer`，使用 `Read` + 行级锚定验证。
- BL-DIFF-BITMAP 中 `allocate_payload_block` 使用 `FullyPresent` 而非 `PartiallyPresent` 是已确认的功能缺陷，但本任务仅记录不修复。
- BL-LOG-WRITE 的日志构造 API 设计尚未确定：需决定是在 `Log` 模块新增写入方法还是引入独立的 `LogWriter`。
- 本任务为文档/记录型，不执行 `cargo build/test`，验证聚焦证据文件字段完整性与交集检查。

## 2026-04-29 task-6 issues
- rustdoc 13 个 warning 全部为预先存在的 intra-doc link 问题（`src/lib.rs:29,49-52` 引用子模块类型无法解析 + `src/error.rs:20` 的 `Error` 歧义），未在本次任务中修复。
- 环境限制同前：无 `rg`/`rust-analyzer`，但本任务仅运行 cargo 命令，不受影响。

## 2026-04-29 task-7 issues
- 初版冲突检查将中文‘未判定’计入 unresolved 造成假阳性；已改为仅检测条目级 TBD/UNDECIDED/UNKNOWN 标记并复跑通过。
