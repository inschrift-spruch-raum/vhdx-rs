# VHDX 开发步骤文档索引

## 文档列表

| 文档 | 内容 | 依赖 |
|------|------|------|
| [01-overview.md](./01-overview.md) | 阶段总览和关键路径 | - |
| [02-phase1-header.md](./02-phase1-header.md) | Phase 1: Header Section 解析 | - |
| [03-phase2-metadata.md](./03-phase2-metadata.md) | Phase 2: Metadata + BAT | Phase 1 |
| [04-phase3-log.md](./04-phase3-log.md) | Phase 3: 日志系统 | Phase 1 |
| [05-phase4-blocks.md](./05-phase4-blocks.md) | Phase 4: 块管理 | Phase 2, 3 |
| [06-phase5-differencing.md](./06-phase5-differencing.md) | Phase 5: 差异磁盘 | Phase 4 |
| [07-phase6-advanced.md](./07-phase6-advanced.md) | Phase 6: 高级功能 | Phase 5 |

## 快速开始

### 阅读顺序
1. 先读 [01-overview.md](./01-overview.md) 了解整体架构
2. 按顺序阅读 Phase 文档（1 → 2 → 3 → 4 → 5 → 6）
3. 每个 Phase 文档包含详细的实现任务和验收标准

### 开发路径

**路径 A: 最小可行产品（MVP）**
- Phase 1 (Header) → Phase 2 (Metadata, 只读) → Phase 4 (块读取, 只读)
- 目标: 能读取固定和动态 VHDX 文件

**路径 B: 基础读写功能**
- 路径 A → Phase 3 (日志) → Phase 4 (块写入)
- 目标: 能读写固定和动态 VHDX 文件

**路径 C: 完整功能**
- 路径 B → Phase 5 (差异磁盘) → Phase 6 (高级功能)
- 目标: 完整的 VHDX 实现

## 参考规范

所有步骤文档基于 Microsoft 开放规范：
- **文档**: [MS-VHDX]: Virtual Hard Disk v2 (VHDX) File Format
- **版本**: v20240423 (Version 8.0)
- **位置**: `misc/MS-VHDX.md`

## 关键约束速查

| 约束 | 说明 |
|------|------|
| 对齐 | 所有结构必须 1MB 对齐 |
| 字节序 | 小端序（Little-Endian） |
| 校验 | CRC-32C (Castagnoli 多项式) |
| 安全 | 双 Header 轮换、日志重放 |
