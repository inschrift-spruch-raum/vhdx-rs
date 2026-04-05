# 库接口文档导航

这一组页面聚焦 `vhdx-rs` crate 的当前公开库接口，不重复 CLI 文档，也不替代顶层入口 [`../../API.md`](../../API.md)。

## 导航

- 返回 [`../../API.md`](../../API.md)，查看 API 与 CLI 总索引
- 回到 [`../../index.md`](../../index.md)，查看文档总览
- 继续进入 [`file.md`](file.md) 或 [`sections.md`](sections.md)

## 先看哪里

建议按下面顺序阅读：

1. [`file.md`](file.md)，先看 `File` 这个主入口，了解打开、创建、按虚拟磁盘偏移读写，以及文件级查询方法。
2. [`sections.md`](sections.md)，再看 `Sections`、Header、BAT、Metadata、Log，还有 `IO`、`Sector`、`PayloadBlock` 这类更细粒度的导航接口。

## 范围怎么拆

- `file.md` 记录大多数库用户会先接触到的稳定入口，尤其是 `File::open(...)`、`File::create(...)`、`read`、`write`、`flush()`。
- `sections.md` 记录偏底层的结构浏览入口，适合你需要检查 VHDX section 布局、BAT 条目、元数据项，或按 sector 观察内容时使用。

## 当前实现面

如果你只想先记住一个结论，那就是优先从 `File` 开始。当前实现里，固定盘的创建、读取、写入、刷新是更完整的主路径；`Sections` 和 `IO` 更适合检查底层结构与导航，其中部分写路径仍未补齐。

crate root 当前会从 `src/lib.rs` 公开导出 `File`、`Sections`、`IO`、`Sector`、`PayloadBlock`，以及多种 section 相关类型。`OpenOptions` 与 `CreateOptions` 虽然是公开 builder 类型，但它们定义在 `src/file.rs`，通过 `File::open` 和 `File::create` 返回，不是 crate root re-export；更细的接口边界请直接看 [`file.md`](file.md) 和 [`sections.md`](sections.md)。

如果你现在要从最常用入口开始，先看 [`file.md`](file.md)。如果你更关心 Header、BAT、Metadata、Log 和 sector 级接口，接着看 [`sections.md`](sections.md)。
