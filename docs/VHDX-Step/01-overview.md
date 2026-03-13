# VHDX 实现阶段总览

基于 [MS-VHDX] Virtual Hard Disk v2 (VHDX) File Format 规范的阶段性开发计划。

## 文档结构参考

MS-VHDX.md 包含以下核心技术章节：
- **Section 2.1**: Layout - 文件布局和 1MB 对齐要求
- **Section 2.2**: Header Section - 文件头、双头机制、Region Table
- **Section 2.3**: Log - 日志系统和重放机制
- **Section 2.4**: Blocks - Payload Blocks 和 Sector Bitmap Blocks
- **Section 2.5**: BAT - 块分配表管理
- **Section 2.6**: Metadata Region - 元数据区域

## 实现阶段概览

| 阶段 | 名称 | 核心内容 | 依赖 |
|------|------|----------|------|
| Phase 1 | 基础解析层 | File Type、Header、Region Table | 无 |
| Phase 2 | 元数据层 | Metadata Region、BAT 解析 | Phase 1 |
| Phase 3 | 日志系统 | Log Entry、Log Replay | Phase 1 |
| Phase 4 | 块管理 | 块读取、动态分配 | Phase 2, 3 |
| Phase 5 | 差异磁盘 | Parent Locator、Sector Bitmap | Phase 4 |
| Phase 6 | 高级功能 | Trim、快照、压缩 | Phase 5 |

## 关键路径

```
Phase 1 (Header)
    ↓
Phase 2 (Metadata + BAT)
    ↓
Phase 3 (Log Replay) ← 必须完成才能安全写入
    ↓
Phase 4 (块读写)
    ↓
Phase 5 (差异磁盘)
    ↓
Phase 6 (高级功能)
```

## 验收标准

每个阶段完成前必须满足：
1. 代码通过单元测试
2. 符合 MS-VHDX 规范要求
3. 与 Windows 生成的 VHDX 文件兼容
4. 文档和注释完整
