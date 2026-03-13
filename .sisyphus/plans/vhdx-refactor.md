# VHDX 模块化重构计划

## TL;DR

> 按 MS-VHDX 规范组件层次重构 linkfs 项目，将 12 个扁平模块重组为 7 个层次化目录模块，提升代码可维护性和规范对应性。
> 
> **Deliverables**:
> - 新的目录结构（7个目录模块）
> - 代码文件拆分（~20个模块文件）
> - 集成测试目录（tests/）
> - 保持原有功能完整性
> 
> **Estimated Effort**: Large (~2-3小时)
> **Parallel Execution**: YES - 6 waves
> **Critical Path**: Wave 1 (common) → Wave 2 (header/bat) → Wave 3 (metadata/log) → Wave 4 (payload/block_io) → Wave 5 (file) → Wave 6 (tests/integration)

---

## Context

### Original Request
用户希望按 VHDX 组成结构重构当前代码，将现有的面条式代码按规范组件层次拆分。

### Current State
- 12 个扁平模块文件（src/*.rs）
- 测试内联在每个文件中
- 总代码量约 3,300 行

### Target State
- 7 个层次化目录模块
- 约 20 个细分模块文件
- 独立的 tests/ 集成测试目录
- 代码结构与 MS-VHDX 规范一一对应

### Key Decisions
1. **目录命名**: 简洁命名（header/, bat/, log/...）不加 region 后缀
2. **Block I/O 组织**: 集中在 block_io/ 目录
3. **测试位置**: 新建 tests/ 目录做集成测试
4. **元数据拆分**: 每个元数据条目独立文件
5. **API 兼容**: 允许 breaking changes

---

## Work Objectives

### Core Objective
将现有的扁平模块结构重构为符合 MS-VHDX 规范的层次化模块结构，提升代码可维护性、可测试性和规范对应性。

### Concrete Deliverables
- 7 个新的目录模块（common/, header/, bat/, log/, metadata/, payload/, block_io/, file/）
- 每个目录下的 mod.rs 和子模块文件
- tests/ 目录及集成测试框架
- 更新后的 lib.rs 导出
- 保持 CLI 工具正常工作

### Definition of Done
- [ ] 新目录结构创建完成
- [ ] 所有代码文件拆分到新位置
- [ ] 所有模块能通过编译
- [ ] 所有测试通过
- [ ] CLI 工具功能正常

### Must Have
- 完整的 VHDX 格式支持（Fixed/Dynamic/Differencing）
- 崩溃一致性保证（Log 系统）
- 原有的所有功能

### Must NOT Have (Guardrails)
- 不修改 VHDX 格式逻辑，只做代码组织重构
- 不删除现有测试，只是移动或复制
- 不引入新依赖

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES（已有内联测试）
- **Automated tests**: YES (Tests-after)
- **Framework**: cargo test
- **New Integration Tests**: tests/ 目录

### QA Policy
每个重构任务完成后验证：
- 编译通过：`cargo check`
- 单元测试通过：`cargo test --lib`
- 集成测试通过：`cargo test --test integration`
- CLI 工具编译通过：`cargo build --release`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately - 基础模块，无依赖):
├── Task 1: 创建 common/ 目录模块 [quick]
└── Task 2: 创建 utils/ 目录模块 [quick]

Wave 2 (After Wave 1 - Header 相关):
├── Task 3: 创建 header/ 目录模块 [quick]
└── Task 4: 创建 bat/ 目录模块 [unspecified-high]

Wave 3 (After Wave 2 - Metadata/Log):
├── Task 5: 创建 metadata/ 目录模块 [unspecified-high]
└── Task 6: 创建 log/ 目录模块 [unspecified-high]

Wave 4 (After Wave 3 - Payload/Block I/O):
├── Task 7: 创建 payload/ 目录模块 [unspecified-high]
└── Task 8: 创建 block_io/ 目录模块 [unspecified-high]

Wave 5 (After Wave 4 - 顶层 API):
├── Task 9: 创建 file/ 目录模块 [unspecified-high]
└── Task 10: 重构 lib.rs [quick]

Wave 6 (After Wave 5 - 测试与集成):
├── Task 11: 创建 tests/ 目录及集成测试框架 [quick]
└── Task 12: 更新 main.rs 及 CLI 工具 [quick]

Wave FINAL (验证):
├── Task F1: 完整编译验证 [quick]
├── Task F2: 单元测试验证 [quick]
├── Task F3: CLI 工具验证 [quick]
└── Task F4: 代码质量检查 [quick]

Critical Path: Task 1 → Task 3 → Task 5 → Task 7 → Task 9 → Task 11 → F1-F4
Parallel Speedup: ~60% faster than sequential
Max Concurrent: 2 (Waves 1-5 alternate)
```

### Agent Dispatch Summary
- **Wave 1**: 2 tasks → `quick` (文件移动和简单重构)
- **Wave 2**: 2 tasks → `quick` + `unspecified-high` (需要理解 header/region 逻辑)
- **Wave 3**: 2 tasks → `unspecified-high` (metadata 和 log 逻辑较复杂)
- **Wave 4**: 2 tasks → `unspecified-high` (payload 和 block_io 逻辑较复杂)
- **Wave 5**: 2 tasks → `unspecified-high` + `quick` (file API 和 lib.rs)
- **Wave 6**: 2 tasks → `quick` (测试和 CLI 调整)
- **Wave FINAL**: 4 tasks → `quick` (验证)

---

## TODOs

- [x] 1. 创建 common/ 目录模块 (无依赖，可立即开始)

  **What to do**:
  1. 创建 `src/common/` 目录
  2. 创建 `src/common/mod.rs`:
     - 导出 guid, crc32c, disk_type
     - 从原 `src/guid.rs`, `src/crc32c.rs` 移动代码
     - 新增 `disk_type.rs`（从 `src/vhdx.rs` 提取 DiskType 枚举）
  3. 移动 `src/guid.rs` 到 `src/common/guid.rs` 并调整模块路径
  4. 移动 `src/crc32c.rs` 到 `src/common/crc32c.rs` 并调整模块路径
  5. 创建 `src/common/disk_type.rs`:
     - 定义 `DiskType` enum: Fixed, Dynamic, Differencing
     - 实现 Display trait
     - 从原 `vhdx.rs` 复制相关代码

  **Must NOT do**:
  - 不修改 GUID 或 CRC 的逻辑
  - 不删除原文件（Wave 10 统一删除）

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 主要是文件移动和简单的模块结构调整
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 2)
  - **Blocks**: Task 10 (lib.rs 重构依赖 common/)
  - **Blocked By**: None

  **References**:
  - Pattern: 原 `src/guid.rs`, `src/crc32c.rs`, `src/vhdx.rs` (DiskType 部分)

  **Acceptance Criteria**:
  - [ ] `cargo check` 通过（临时添加 common/ 到 lib.rs）
  - [ ] common/mod.rs 正确导出所有子模块
  - [ ] disk_type.rs 包含完整的 DiskType enum

  **QA Scenarios**:
  ```
  Scenario: 编译 common 模块
    Tool: Bash
    Preconditions: 已执行本任务
    Steps:
      1. cargo check --lib
    Expected Result: 编译成功，无错误
    Evidence: .sisyphus/evidence/task-1-compile.log
  ```

  **Commit**: YES
  - Message: `refactor: create common/ directory module`
  - Files: `src/common/**`

- [x] 2. 创建 utils/ 目录模块 (无依赖，可立即开始)

  **What to do**:
  1. 创建 `src/utils/` 目录
  2. 创建 `src/utils/mod.rs`:
     - 导出通用工具函数（如果有）
     - 可以从原代码中提取通用辅助函数
  3. 检查原代码中是否有跨模块的工具函数需要移动到这里

  **Must NOT do**:
  - 不创建空文件，如果无工具函数可暂不创建内容

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 1)
  - **Blocks**: None
  - **Blocked By**: None

  **References**:
  - Pattern: 查看原代码中是否有通用工具函数

  **Acceptance Criteria**:
  - [ ] utils/mod.rs 存在（可为空或包含工具函数）

  **Commit**: YES (groups with Task 1)

- [x] 3. 创建 header/ 目录模块 (依赖 Wave 1)

  **What to do**:
  1. 创建 `src/header/` 目录
  2. 创建 `src/header/mod.rs`:
     - 导出 file_type, header, region_table
  3. 创建 `src/header/file_type.rs`:
     - 从原 `src/header.rs` 提取 FileTypeIdentifier
     - 包含签名验证逻辑
  4. 创建 `src/header/header.rs`:
     - 从原 `src/header.rs` 提取 VhdxHeader
     - 包含双头机制、SequenceNumber、校验和验证
  5. 创建 `src/header/region_table.rs`:
     - 从原 `src/region.rs` 移动 RegionTable 相关代码
     - RegionTableHeader, RegionTableEntry

  **Must NOT do**:
  - 不修改 header 验证逻辑
  - 保留原有的测试（后续移动到 tests/）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2
  - **Blocks**: Task 10 (lib.rs)
  - **Blocked By**: Task 1 (common/ 提供依赖如 guid, crc32c)

  **References**:
  - Pattern: 原 `src/header.rs`, `src/region.rs`

  **Acceptance Criteria**:
  - [ ] header/mod.rs 正确导出
  - [ ] file_type.rs 包含 FileTypeIdentifier
  - [ ] header.rs 包含 VhdxHeader 和双头逻辑
  - [ ] region_table.rs 包含 RegionTable 结构

  **QA Scenarios**:
  ```
  Scenario: 验证 header 模块编译
    Tool: Bash
    Preconditions: Task 1 完成
    Steps:
      1. 临时在 lib.rs 添加 mod header
      2. cargo check --lib
    Expected Result: 编译成功
    Evidence: .sisyphus/evidence/task-3-compile.log
  ```

  **Commit**: YES
  - Message: `refactor: create header/ directory module`

- [x] 4. 创建 bat/ 目录模块 (依赖 Wave 1)

  **What to do**:
  1. 创建 `src/bat/` 目录
  2. 创建 `src/bat/mod.rs`:
     - 导出 entry, states, table
  3. 创建 `src/bat/entry.rs`:
     - 从原 `src/bat.rs` 提取 BatEntry (64位)
     - State (3 bits) + Reserved (17 bits) + FileOffsetMB (44 bits)
  4. 创建 `src/bat/states.rs`:
     - PayloadBlockState enum: NotPresent, Zero, FullyPresent, PartiallyPresent, etc.
     - SectorBitmapState enum: NotPresent, Present
  5. 创建 `src/bat/table.rs`:
     - Bat 结构体
     - Chunk ratio 计算逻辑
     - BAT 索引计算方法

  **Must NOT do**:
  - 不修改 BAT 条目格式
  - Sector Bitmap 逻辑移到 payload/ (Task 7)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 需要理解复杂的 BAT entry 位布局和 chunk 计算
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 3)
  - **Blocks**: Task 7 (payload/), Task 8 (block_io/), Task 9 (file/)
  - **Blocked By**: Task 1 (common/)

  **References**:
  - Pattern: 原 `src/bat.rs`
  - MS-VHDX: Section 2.5 BAT

  **Acceptance Criteria**:
  - [ ] BatEntry 定义正确（64位）
  - [ ] PayloadBlockState 所有状态定义
  - [ ] SectorBitmapState 定义
  - [ ] Chunk ratio 计算正确

  **QA Scenarios**:
  ```
  Scenario: BAT entry 位布局验证
    Tool: Bash (cargo test)
    Preconditions: Task 1 完成
    Steps:
      1. 移动原 bat.rs 测试代码
      2. cargo test test_bat_entry
    Expected Result: 测试通过
    Evidence: .sisyphus/evidence/task-4-bat-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create bat/ directory module`

- [x] 5. 创建 metadata/ 目录模块 (依赖 Wave 1-2)

  **What to do**:
  1. 创建 `src/metadata/` 目录
  2. 创建 `src/metadata/mod.rs`:
     - 导出所有元数据子模块
  3. 创建 `src/metadata/region.rs`:
     - MetadataRegion 容器结构
     - 从原 `src/metadata.rs` 提取
  4. 创建 `src/metadata/table.rs`:
     - MetadataTable, MetadataTableHeader, MetadataTableEntry
     - 从原 `src/metadata.rs` 提取
  5. 创建 `src/metadata/file_parameters.rs`:
     - FileParameters 结构
     - BlockSize, LeaveBlockAllocated, HasParent
  6. 创建 `src/metadata/disk_size.rs`:
     - VirtualDiskSize 结构
  7. 创建 `src/metadata/disk_id.rs`:
     - VirtualDiskId (GUID)
  8. 创建 `src/metadata/sector_size.rs`:
     - LogicalSectorSize, PhysicalSectorSize
  9. 创建 `src/metadata/parent_locator.rs`:
     - ParentLocator, ParentLocatorHeader, ParentLocatorEntry
     - 差异磁盘父定位器逻辑

  **Must NOT do**:
  - 不修改元数据解析逻辑
  - 每个文件只包含一个元数据条目类型

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 元数据逻辑较复杂，需要正确拆分
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3
  - **Blocks**: Task 9 (file/ 使用 metadata)
  - **Blocked By**: Task 1 (common/), Task 3 (header/region_table 定位 metadata)

  **References**:
  - Pattern: 原 `src/metadata.rs` (540行，需拆分)
  - MS-VHDX: Section 2.6 Metadata Region

  **Acceptance Criteria**:
  - [ ] 6个元数据条目各自独立文件
  - [ ] MetadataTable 正确解析
  - [ ] ParentLocator 完整实现

  **QA Scenarios**:
  ```
  Scenario: 元数据解析验证
    Tool: Bash (cargo test)
    Steps:
      1. 移动原 metadata.rs 测试
      2. cargo test test_file_parameters
      3. cargo test test_sector_size
    Expected Result: 所有测试通过
    Evidence: .sisyphus/evidence/task-5-metadata-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create metadata/ directory module with split items`

- [x] 6. 创建 log/ 目录模块 (依赖 Wave 1-2)

  **What to do**:
  1. 创建 `src/log/` 目录
  2. 创建 `src/log/mod.rs`:
     - 导出所有日志子模块
  3. 创建 `src/log/entry.rs`:
     - LogEntryHeader 结构
     - 从原 `src/log.rs` 提取
  4. 创建 `src/log/descriptor.rs`:
     - ZeroDescriptor (zero signature)
     - DataDescriptor (desc signature)
     - 从原 `src/log.rs` 提取
  5. 创建 `src/log/sector.rs`:
     - DataSector (data signature)
     - SequenceHigh/SequenceLow 验证
  6. 创建 `src/log/replayer.rs`:
     - LogReplayer 结构
     - Log replay 逻辑（崩溃恢复）
  7. 创建 `src/log/writer.rs`:
     - LogWriter 结构
     - 日志写入逻辑

  **Must NOT do**:
  - 不修改日志格式和 replay 逻辑
  - 这是崩溃一致性的核心，需特别小心

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 日志系统逻辑复杂，涉及崩溃恢复
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Task 5)
  - **Blocks**: Task 8 (block_io/ 使用 log), Task 9 (file/)
  - **Blocked By**: Task 1 (common/), Task 3 (header/ 定位 log)

  **References**:
  - Pattern: 原 `src/log.rs` (860行，需拆分)
  - MS-VHDX: Section 2.3 Log

  **Acceptance Criteria**:
  - [ ] LogEntryHeader 结构完整
  - [ ] ZeroDescriptor 和 DataDescriptor 分离
  - [ ] LogReplayer 和 LogWriter 分离
  - [ ] 保留所有日志测试

  **QA Scenarios**:
  ```
  Scenario: 日志重放验证
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_log_entry_header
      2. cargo test test_zero_descriptor
      3. cargo test test_data_descriptor
    Expected Result: 所有测试通过
    Evidence: .sisyphus/evidence/task-6-log-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create log/ directory module`

- [x] 7. 创建 payload/ 目录模块 (依赖 Wave 1-4)

  **What to do**:
  1. 创建 `src/payload/` 目录
  2. 创建 `src/payload/mod.rs`:
     - 导出 block, bitmap, chunk
  3. 创建 `src/payload/block.rs`:
     - Payload Block 操作
     - 从原 `src/block.rs` 提取通用逻辑
  4. 创建 `src/payload/bitmap.rs`:
     - Sector Bitmap Block 操作
     - 从原 `src/bat.rs` 提取 Sector Bitmap 相关代码
     - 位图读取和解析逻辑
  5. 创建 `src/payload/chunk.rs`:
     - Chunk 计算逻辑
     - Chunk ratio 公式: (2^23 * LogicalSectorSize) / BlockSize
     - 从原 `src/bat.rs` 提取

  **Must NOT do**:
  - 不修改 payload block 数据布局
  - Sector Bitmap 逻辑从 bat.rs 完整移出

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 需要正确分离 Sector Bitmap 和 Chunk 计算
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4
  - **Blocks**: Task 8 (block_io/)
  - **Blocked By**: Task 4 (bat/), Task 5 (metadata/ 提供 sector size)

  **References**:
  - Pattern: 原 `src/bat.rs` (Sector Bitmap), `src/block.rs`
  - MS-VHDX: Section 2.4 Blocks

  **Acceptance Criteria**:
  - [ ] Sector Bitmap 操作完整
  - [ ] Chunk 计算正确
  - [ ] Payload Block 基础操作

  **QA Scenarios**:
  ```
  Scenario: Chunk 计算验证
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_chunk_calculation
    Expected Result: 测试通过
    Evidence: .sisyphus/evidence/task-7-chunk-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create payload/ directory module`

- [x] 8. 创建 block_io/ 目录模块 (依赖 Wave 1-7)

  **What to do**:
  1. 创建 `src/block_io/` 目录
  2. 创建 `src/block_io/mod.rs`:
     - 导出 traits, fixed, dynamic, differencing, cache
  3. 创建 `src/block_io/traits.rs`:
     - BlockIo trait 定义
     - 从原 `src/block.rs` 提取
  4. 创建 `src/block_io/fixed.rs`:
     - FixedBlockIo 实现
     - 固定磁盘 I/O 逻辑
  5. 创建 `src/block_io/dynamic.rs`:
     - DynamicBlockIo 实现（新建）
     - 动态磁盘 I/O 逻辑（从原 block.rs 提取）
  6. 创建 `src/block_io/differencing.rs`:
     - DifferencingBlockIo 实现（新建）
     - 差异磁盘 I/O 逻辑（从原 block.rs 提取）
  7. 创建 `src/block_io/cache.rs`:
     - BlockCache 实现
     - 从原 `src/block.rs` 提取

  **Must NOT do**:
  - 不修改 I/O 逻辑，只做文件拆分
  - 保留原有的 trait 定义

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Block I/O 是核心功能，需要仔细拆分
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Task 7)
  - **Blocks**: Task 9 (file/)
  - **Blocked By**: Task 4 (bat/), Task 5 (metadata/), Task 6 (log/), Task 7 (payload/)

  **References**:
  - Pattern: 原 `src/block.rs` (429行)
  - MS-VHDX: Section 2.4 Blocks (I/O operations)

  **Acceptance Criteria**:
  - [ ] BlockIo trait 定义完整
  - [ ] FixedBlockIo, DynamicBlockIo, DifferencingBlockIo 分离
  - [ ] BlockCache 保留

  **QA Scenarios**:
  ```
  Scenario: Block I/O 验证
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_block_cache
    Expected Result: 测试通过
    Evidence: .sisyphus/evidence/task-8-blockio-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create block_io/ directory module`

- [x] 9. 创建 file/ 目录模块 (依赖 Wave 1-8)

  **What to do**:
  1. 创建 `src/file/` 目录
  2. 创建 `src/file/mod.rs`:
     - 导出 vhdx_file, builder
  3. 创建 `src/file/vhdx_file.rs`:
     - VhdxFile 结构体
     - 从原 `src/vhdx.rs` 提取（949行拆分到两个文件）
     - 包含 open(), read(), write(), virtual_disk_size() 等方法
  4. 创建 `src/file/builder.rs`:
     - VhdxBuilder 结构体
     - 从原 `src/vhdx.rs` 提取
     - 包含 new(), with_size(), with_type(), create() 等方法

  **Must NOT do**:
  - 不修改 public API（暂时保持兼容）
  - 只是将代码物理拆分到两个文件

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 需要仔细拆分 VhdxFile 和 VhdxBuilder
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Task 10 (lib.rs), Task 12 (main.rs)
  - **Blocked By**: Task 1-8（所有底层模块）

  **References**:
  - Pattern: 原 `src/vhdx.rs` (949行)

  **Acceptance Criteria**:
  - [ ] VhdxFile 完整功能
  - [ ] VhdxBuilder 完整功能
  - [ ] DiskType enum 已移动到 common/

  **QA Scenarios**:
  ```
  Scenario: VHDX 文件操作验证
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_vhdx_builder
      2. cargo test test_create_dynamic_vhdx
      3. cargo test test_create_fixed_vhdx
    Expected Result: 所有测试通过
    Evidence: .sisyphus/evidence/task-9-file-test.log
  ```

  **Commit**: YES
  - Message: `refactor: create file/ directory module`

- [x] 10. 重构 lib.rs (依赖 Wave 1-9)

  **What to do**:
  1. 重写 `src/lib.rs`:
     - 按新模块结构导出 public API
     - 示例：
       ```rust
       pub mod common;
       pub mod header;
       pub mod bat;
       pub mod log;
       pub mod metadata;
       pub mod payload;
       pub mod block_io;
       pub mod file;
       pub mod error;
       
       // Re-exports for convenience
       pub use file::{VhdxFile, VhdxBuilder};
       pub use common::DiskType;
       pub use error::VhdxError;
       ```
  2. 更新模块声明，移除旧模块
  3. 保持向后兼容的 re-exports（如果可能）

  **Must NOT do**:
  - 不删除旧文件（最后统一删除）
  - 保持 public API 稳定（可选）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5 (with Task 9)
  - **Blocks**: Task 12 (main.rs 依赖 lib exports)
  - **Blocked By**: Task 1-9

  **References**:
  - Pattern: 原 `src/lib.rs` (19行)

  **Acceptance Criteria**:
  - [ ] lib.rs 正确导出所有新模块
  - [ ] `cargo check --lib` 通过

  **QA Scenarios**:
  ```
  Scenario: 库编译验证
    Tool: Bash
    Steps:
      1. cargo check --lib
      2. cargo build --lib
    Expected Result: 编译成功
    Evidence: .sisyphus/evidence/task-10-lib-build.log
  ```

  **Commit**: YES
  - Message: `refactor: update lib.rs with new module structure`

- [ ] 11. 创建 tests/ 目录及集成测试框架 (依赖 Wave 1-10)

  **What to do**:
  1. 创建 `tests/` 目录结构：
     ```
     tests/
     ├── common/
     │   └── mod.rs
     ├── header/
     │   └── header_tests.rs
     ├── bat/
     │   └── bat_tests.rs
     ├── log/
     │   └── log_tests.rs
     ├── metadata/
     │   └── metadata_tests.rs
     ├── payload/
     │   └── payload_tests.rs
     ├── block_io/
     │   └── block_io_tests.rs
     ├── file/
     │   └── file_tests.rs
     └── integration/
         └── full_workflow.rs
     ```
  2. 创建 `tests/common/mod.rs`:
     - 共享的测试辅助函数
     - 测试数据生成工具
  3. 从原内联测试中移动关键测试到对应 tests/ 文件
  4. 创建 `tests/integration/full_workflow.rs`:
     - 完整的 VHDX 创建-写入-读取-验证流程

  **Must NOT do**:
  - 不删除原内联测试（可选保留）
  - 只是补充集成测试

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: None
  - **Blocked By**: Task 1-10

  **References**:
  - Pattern: 原各文件中的 `#[cfg(test)]` 模块

  **Acceptance Criteria**:
  - [ ] tests/ 目录结构完整
  - [ ] 至少一个集成测试通过
  - [ ] `cargo test --test integration` 能运行

  **QA Scenarios**:
  ```
  Scenario: 集成测试运行
    Tool: Bash
    Steps:
      1. cargo test --test full_workflow
    Expected Result: 测试通过
    Evidence: .sisyphus/evidence/task-11-integration-test.log
  ```

  **Commit**: YES
  - Message: `test: add tests/ directory with integration tests`

- [ ] 12. 更新 main.rs 及 CLI 工具 (依赖 Wave 1-11)

  **What to do**:
  1. 更新 `src/main.rs`:
     - 更新 use 语句，使用新的模块路径
     - 检查是否需要调整导入
     - 保持 CLI 命令不变
  2. 验证所有 CLI 子命令：
     - `info` - 显示 VHDX 信息
     - `create` - 创建新 VHDX
     - `read` - 读取数据
     - `write` - 写入数据
     - `check` - 检查完整性
  3. 测试 CLI 工具编译

  **Must NOT do**:
  - 不修改 CLI 命令接口（保持用户习惯）
  - 不修改功能逻辑

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6 (with Task 11)
  - **Blocks**: None
  - **Blocked By**: Task 10 (lib.rs)

  **References**:
  - Pattern: 原 `src/main.rs` (353行)

  **Acceptance Criteria**:
  - [ ] CLI 编译通过：`cargo build --release`
  - [ ] CLI 能运行：`./target/release/vhdx-tool --help`

  **QA Scenarios**:
  ```
  Scenario: CLI 工具验证
    Tool: Bash
    Steps:
      1. cargo build --release
      2. ./target/release/vhdx-tool --help
      3. ./target/release/vhdx-tool info test.vhdx (如果有测试文件)
    Expected Result: 编译成功，帮助信息正常
    Evidence: .sisyphus/evidence/task-12-cli-build.log
  ```

  **Commit**: YES
  - Message: `refactor: update main.rs for new module structure`

---

## Final Verification Wave (MANDATORY - after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [ ] F1. 完整编译验证
  执行完整编译检查，验证所有模块能正确编译
  - `cargo check --lib`
  - `cargo build --lib`
  - `cargo build --release` (包含 CLI)
  Output: `Build [PASS/FAIL] | VERDICT`

- [ ] F2. 单元测试验证
  运行所有单元测试
  - `cargo test --lib`
  - 统计通过/失败数量
  Output: `Unit Tests [N pass/N fail] | VERDICT`

- [ ] F3. 集成测试验证
  运行所有集成测试
  - `cargo test --test full_workflow`
  - 验证完整工作流
  Output: `Integration Tests [PASS/FAIL] | VERDICT`

- [ ] F4. CLI 工具验证
  验证 CLI 工具功能完整
  - `./target/release/vhdx-tool --help`
  - `./target/release/vhdx-tool info <test.vhdx>`
  Output: `CLI [PASS/FAIL] | VERDICT`

---

## Cleanup Task

- [ ] CLEANUP. 删除旧文件 (After ALL verification passed)

  **What to do**:
  1. 删除已迁移的旧文件：
     - `src/guid.rs` (已移动到 common/)
     - `src/crc32c.rs` (已移动到 common/)
     - `src/header.rs` (已移动到 header/)
     - `src/region.rs` (已移动到 header/)
     - `src/bat.rs` (已移动到 bat/)
     - `src/log.rs` (已移动到 log/)
     - `src/metadata.rs` (已移动到 metadata/)
     - `src/block.rs` (已移动到 block_io/)
     - `src/vhdx.rs` (已移动到 file/)
  2. 重新验证编译
  3. 确保没有残留的旧代码

  **Must NOT do**:
  - 在验证通过前不删除旧文件
  - 确保所有测试仍然通过

  **Recommended Agent Profile**:
  - **Category**: `quick`

  **Acceptance Criteria**:
  - [ ] 所有旧文件已删除
  - [ ] 编译仍然通过
  - [ ] 测试仍然通过

  **Commit**: YES
  - Message: `refactor: remove old files after migration`

---

## Commit Strategy

- **Wave 1**: `refactor: create common/ and utils/ directory modules`
- **Wave 2**: `refactor: create header/ and bat/ directory modules`
- **Wave 3**: `refactor: create metadata/ and log/ directory modules`
- **Wave 4**: `refactor: create payload/ and block_io/ directory modules`
- **Wave 5**: `refactor: create file/ directory module and update lib.rs`
- **Wave 6**: `refactor: add tests/ directory and update CLI`
- **Cleanup**: `refactor: remove old files after migration`

---

## Success Criteria

### Verification Commands
```bash
# 库编译
cargo check --lib

# 完整构建
cargo build --release

# 所有测试
cargo test

# CLI 工具
./target/release/vhdx-tool --help
```

### Final Checklist
- [ ] 新目录结构完整 (7个目录模块)
- [ ] 所有代码文件正确拆分 (~20个模块文件)
- [ ] tests/ 目录及集成测试框架创建
- [ ] lib.rs 正确导出所有模块
- [ ] CLI 工具正常工作
- [ ] 所有旧文件已清理
- [ ] 所有测试通过
- [ ] 功能完整保留（创建/读取/写入/检查 VHDX）

---

## Summary

此重构计划将现有的 12 个扁平模块重组为 7 个层次化目录模块：

| VHDX 规范组件 | 新模块路径 | 文件数 |
|---------------|------------|--------|
| 公共基础设施 | `common/` | 4 |
| Header Section | `header/` | 4 |
| BAT | `bat/` | 4 |
| Log | `log/` | 6 |
| Metadata | `metadata/` | 8 |
| Payload | `payload/` | 4 |
| Block I/O | `block_io/` | 5 |
| 顶层 API | `file/` | 3 |
| **总计** | | **~38** |

重构后代码结构与 MS-VHDX 规范文档一一对应，便于维护和功能扩展。
