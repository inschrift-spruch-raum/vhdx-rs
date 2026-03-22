# VHDX Rust 库 API 设计分析

## 基于 MS-VHDX v20240423 规范的导出设计

---

## 模块文件

本文档按功能模块组织为以下文件：

| 模块 | 描述 | 链接 |
|------|------|------|
| File API | 核心文件操作，打开/创建选项 | [file.md](./API/file.md) |
| Sections | Header、BAT、Metadata、Log | [section.md](./API/section.md) |
| IO | 扇区级读写操作 | [io.md](./API/io.md) |
| Types | Guid、Error 类型 | [types.md](./API/types.md) |
| CLI | 命令行工具 | [cli.md](./API/cli.md) |
| Examples | 使用示例 | [examples.md](./API/examples.md) |

在子模块文档中查看详细的 API 树和类型定义。

---

## 快速导航

### 核心概念

- **File**: VHDX 文件句柄，通过 `File::open()` 或 `File::create()` 获取
- **Sections**: 物理文件结构映射（Header、BAT、Metadata、Log）
- **IO**: 扇区级读写操作，通过 `file.io()` 获取
- **Sector**: 扇区级定位，自动处理 BAT 寻址

### 使用流程

1. 打开或创建 VHDX 文件
2. 通过 `sections()` 访问文件结构
3. 通过 `io()` 进行扇区级读写
4. 使用 CLI 工具进行文件操作

---

## 文档版本

- **规范**: MS-VHDX v20240423
- **版本**: 3.0
- **更新日期**: 2026
