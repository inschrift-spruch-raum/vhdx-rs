# Learnings (analysis-mode)

- 当前工作区中最关键实现入口集中在 `src/file.rs`、`src/io_module.rs`、`src/validation.rs` 与 `src/sections/*`。
- `docs/plan/API.md` 的核心 API 面（`File/OpenOptions/CreateOptions/IO/validation/section`）在实现中基本都有对应导出，`src/lib.rs` 已提供 `section::Entry` 兼容别名。
- 发现环境中 `grep/glob` 依赖 `rg` 不可用，本轮主要用 `Read` 与 `LSP` 做证据收集。

## Gap summary (plan-first)

- `SpecValidator::validate_file()` 未包含 `validate_parent_locator()` / `validate_parent_chain()`，差分盘全量校验覆盖不足。
- `validate_region_table()` 与打开路径未调用 `RegionTableHeader::verify_checksum()`，region table CRC 校验能力未接入。
- `IO::sector()` 以整扇区截断范围，和 `File::read()` 的字节级边界语义存在潜在不一致（尾部非整扇区场景）。
