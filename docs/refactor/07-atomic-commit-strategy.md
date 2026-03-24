# 原子提交策略与测试驱动开发计划

## 概述

本文档定义了执行重构的原子提交策略和测试驱动开发(TDD)方法。每次提交都是自包含的、可回滚的，并保持可工作状态。

## 测试驱动开发工作流

### 核心原则

```
Red -> Green -> Refactor -> Commit
```

1. **Red（红）**: 为期望行为编写失败的测试
2. **Green（绿）**: 最小化实现使测试通过
3. **Refactor（重构）**: 在保持测试通过的情况下清理代码
4. **Commit（提交）**: 原子提交，附带清晰的提交信息

### 工作流步骤

```bash
# 1. 从失败的测试开始
cargo test --test my_test  # 失败 (Red)

# 2. 实现最小变更
# ... 编辑代码 ...

# 3. 验证测试通过
cargo test --test my_test  # 通过 (Green)

# 4. 运行所有测试确保没有回归
cargo test  # 全部通过

# 5. 运行 clippy 和 fmt
cargo clippy -- -D warnings
cargo fmt

# 6. 提交，附带描述性信息
git add -A
git commit -m "refactor: 简短描述

变更的详细说明：
- 变更了什么
- 为什么变更
- 破坏性变更（如有）

测试：
- 为 X 添加了单元测试
- 所有现有测试通过"

# 7. 验证提交
git log -1 --stat
```

## 提交结构

### 提交信息格式

```
type(scope): 简短描述

详细说明：
- 变更 1
- 变更 2
- 变更 3

破坏性变更：
- API 变更 X（迁移指南：见 docs/refactor/XX.md）

测试：
- 为 Y 添加了测试
- 所有现有测试通过
- 性能：中性/退化/提升 X%

Refs: #issue-number
```

### 提交类型

- `refactor`: 代码重构（无功能变更）
- `feat`: 新功能
- `fix`: 错误修复
- `test`: 添加或更新测试
- `docs`: 文档变更
- `chore`: 维护任务
- `perf`: 性能改进

### 提交示例

#### 示例 1：常量重构

```
refactor(constants): 提取 VHDX 魔法数字到常量模块

- 创建 src/constants.rs，包含所有 VHDX 格式常量
- 将硬编码的 1024*1024 替换为 constants::MB
- 将头偏移量替换为 constants::layout::HEADER_OFFSET_1 等
- 添加编译时断言验证常量有效性

测试：
- 为常量计算添加了单元测试
- 所有现有测试通过（无行为变更）
- 通过 grep 验证：无魔法数字残留

Refs: docs/refactor/01-magic-numbers-and-constants.md
```

#### 示例 2：类型安全

```
refactor(types): 添加 VirtualOffset 和 FileOffset 新类型

- 创建 src/types.rs，包含新类型包装器
- VirtualOffset 在构造时验证扇区对齐
- FileOffset 在构造时验证 1MB 对齐
- 添加带有溢出检查的算术运算

破坏性变更：
- VhdxFile::read/write 现在接受 VirtualOffset 而非 u64
- 迁移：使用 VirtualOffset::new(offset)? 代替原始偏移量

测试：
- 为所有新类型添加了全面的单元测试
- 更新了集成测试以使用新类型
- 所有现有测试通过新 API

Refs: docs/refactor/02-type-safety-newtypes.md
```

## 重构执行计划

### 第一阶段：基础（低风险）

#### 提交 1：常量模块
```bash
# 提交前状态
git status  # 干净的工作区
cargo test  # 全部通过

# 创建常量模块
# ... 实现 src/constants.rs ...

# 验证
cargo test
cargo clippy -- -D warnings

# 提交
git add src/constants.rs src/lib.rs
git commit -m "refactor(constants): 提取 VHDX 魔法数字

将所有魔法数字提取到集中管理的常量模块。
- 添加大小常量（KB, MB, GB）
- 添加布局常量（头偏移量、区域位置）
- 添加扇区大小常量
- 添加块大小约束

测试：
- 为常量添加了单元测试
- 所有现有测试通过
- 验证无魔法数字残留

Refs: docs/refactor/01-magic-numbers-and-constants.md"
```

#### 提交 2：替换魔法数字 - 第 1 部分
```bash
# 在头模块中替换
# ... 编辑 src/header/*.rs ...

git add src/header/
git commit -m "refactor(header): 使用常量表示偏移量

将硬编码偏移量替换为 constants::layout 值。
- HEADER_1_OFFSET, HEADER_2_OFFSET
- REGION_TABLE_1_OFFSET, REGION_TABLE_2_OFFSET
- FILE_TYPE_SIZE

无行为变更。

Refs: docs/refactor/01-magic-numbers-and-constants.md"
```

#### 提交 3：替换魔法数字 - 第 2 部分
```bash
# 在构建器中替换
# ... 编辑 src/file/builder.rs ...

git add src/file/builder.rs
git commit -m "refactor(builder): 使用常量表示大小

将硬编码大小替换为常量模块。
- 扇区大小（512, 4096）
- 块大小约束（1MB-256MB）
- 对齐要求

无行为变更。

Refs: docs/refactor/01-magic-numbers-and-constants.md"
```

### 第二阶段：类型安全

#### 提交 4：创建新类型模块
```bash
# 创建 src/types.rs
# ... 实现新类型 ...

git add src/types.rs src/lib.rs
git commit -m "refactor(types): 为偏移量添加新类型包装器

为 VHDX 偏移量添加类型安全的包装器：
- VirtualOffset（扇区对齐的虚拟偏移量）
- FileOffset（1MB 对齐的文件偏移量）
- BlockSize（验证的 1MB-256MB）
- SectorSize（512 或 4096）

优点：
- 编译时防止偏移量混淆
- 运行时验证对齐
- 自说明代码

测试：
- 为所有新类型添加了全面的单元测试
- 边界情况的验证测试

Refs: docs/refactor/02-type-safety-newtypes.md"
```

#### 提交 5：更新 BlockIo 特性
```bash
# 更新特性签名
# ... 编辑 src/block_io/traits.rs ...

git add src/block_io/traits.rs
git commit -m "refactor(traits): 更新 BlockIo 以使用新类型

破坏性变更：
- BlockIo::read 签名: virtual_offset: VirtualOffset
- BlockIo::write 签名: virtual_offset: VirtualOffset

迁移：使用 VirtualOffset::new(offset)? 包装偏移量

Refs: docs/refactor/02-type-safety-newtypes.md"
```

#### 提交 6：更新 VhdxFile 公共 API
```bash
# 更新 VhdxFile read/write 方法
# ... 编辑 src/file/vhdx_file.rs ...

git add src/file/vhdx_file.rs
git commit -m "refactor(vhdx_file): 在公共 API 中使用新类型

更新 VhdxFile::read 和 VhdxFile::write 以使用 VirtualOffset。

破坏性变更：
- VhdxFile::read(offset: VirtualOffset, ...)
- VhdxFile::write(offset: VirtualOffset, ...)

所有调用点已更新。测试已更新。

Refs: docs/refactor/02-type-safety-newtypes.md"
```

### 第三阶段：函数分解

#### 提交 7：提取 VhdxFile::open 辅助函数
```bash
# 提取辅助方法
# ... 编辑 src/file/vhdx_file.rs ...

git add src/file/vhdx_file.rs
git commit -m "refactor(vhdx_file): 从 open() 提取辅助方法

将 VhdxFile::open() 分解为专注的辅助函数：
- open_file()
- read_file_type()
- read_and_validate_headers()
- replay_log_if_needed()
- read_region_tables()
- read_metadata_region()
- read_bat()
- detect_disk_type()
- load_parent_if_needed()
- initialize()
- initialize_log_writer()

VhdxFile::open() 从 149 行减少到 25 行。

无行为变更。

Refs: docs/refactor/03-complex-function-decomposition.md"
```

#### 提交 8：提取构建器辅助函数
```bash
# 提取构建器组件
# ... 编辑 src/file/builder.rs ...

git add src/file/builder.rs
git commit -m "refactor(builder): 提取专用构建器

将 VhdxBuilder::create() 拆分为专注的构建器：
- LayoutCalculator
- HeaderWriter
- RegionTableWriter
- BatWriter
- MetadataWriter
- FixedDiskAllocator

VhdxBuilder::create() 从 335 行减少到 45 行。

无行为变更。

Refs: docs/refactor/03-complex-function-decomposition.md"
```

### 第四阶段：BlockIo 去重

#### 提交 9：创建 BaseBlockIo
```bash
# 创建 src/block_io/base.rs
# ... 实现共享逻辑 ...

git add src/block_io/base.rs src/block_io/mod.rs
git commit -m "refactor(block_io): 创建 BaseBlockIo 共享逻辑

提取通用 BlockIo 功能：
- read_from_block()
- write_to_block()
- allocate_block()
- write_bat_entry()

减少动态/差异/固定磁盘之间的代码重复。

Refs: docs/refactor/04-block-io-deduplication.md"
```
