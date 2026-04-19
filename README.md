# vhdx-rs

VHDX (Virtual Hard Disk v2) 文件格式的 Rust 实现。

纯 Rust 编写的 VHDX 虚拟磁盘库，支持 Fixed、Dynamic、Differencing 三种磁盘类型，包含完整的日志回放和 CLI 工具。

## 功能特性

- **三种 VHDX 磁盘类型**
  - Fixed（固定大小）
  - Dynamic（动态分配）
  - Differencing（差异磁盘）

- **完整的文件操作**
  - 打开、创建、读写虚拟磁盘
  - 自动日志回放，崩溃一致性保证
  - 双头安全机制与 CRC-32C 校验

- **核心组件**
  - Header 解析与验证
  - Region Table 解析
  - BAT（块分配表）管理
  - Metadata Region 读写
  - 块级 IO 操作

- **CLI 命令行工具** `vhdx-tool`
- **跨平台** Windows / Linux

## 构建

```bash
# 构建库
cargo build

# 构建 CLI 工具
cargo build -p vhdx-tool

# 运行测试
cargo test --workspace
```

## CLI 使用

```bash
# 显示 VHDX 文件信息
vhdx-tool info disk.vhdx

# 创建虚拟磁盘
vhdx-tool create new.vhdx --size 10GB
vhdx-tool create fixed.vhdx --size 20GB --type fixed
vhdx-tool create compat.vhdx --size 20GB --disk-type fixed
vhdx-tool create child.vhdx --size 10GB --type differencing --parent base.vhdx

# 覆盖已存在文件
vhdx-tool create existed.vhdx --size 10GB --force

# 检查文件完整性
vhdx-tool check disk.vhdx

# 修复文件（重放日志）
vhdx-tool repair disk.vhdx

# 查看区域详情
vhdx-tool sections disk.vhdx header
vhdx-tool sections disk.vhdx bat
vhdx-tool sections disk.vhdx metadata
vhdx-tool sections disk.vhdx log
```

### create 参数契约

- `--type <DISK_TYPE>` 是主参数，`-d` 是其短选项。
- `--disk-type <DISK_TYPE>` 是兼容别名。当 `--type` 与 `--disk-type` 同时给出时，`--type` 优先，`--disk-type` 会被忽略。
- `DISK_TYPE` 可选值与 `--help` 一致：`dynamic`、`fixed`、`differencing`。
- `--force` 仅表示“允许覆盖已存在的目标文件”。它不代表修复、忽略校验、或跳过父盘检查等额外能力。

## 库 API 使用

```rust
use vhdx_rs::File;

// 打开 VHDX 文件
let file = File::open("disk.vhdx").finish()?;

// 创建新文件（10GB Fixed 类型）
let file = File::create("new.vhdx")
    .size(10 * 1024 * 1024 * 1024)
    .fixed(true)
    .finish()?;

// 创建 Dynamic 类型
let file = File::create("dynamic.vhdx")
    .size(10 * 1024 * 1024 * 1024)
    .finish()?;

// 读写数据
let mut buf = vec![0u8; 512];
file.read(0, &mut buf)?;
file.write(0, &buf)?;
```

## 项目结构

```
vhdx-rs/
├── src/
│   ├── lib.rs          # 库入口，导出公共 API
│   ├── file.rs         # VHDX 文件操作核心（打开、创建、读写）
│   ├── io_module.rs    # 扇区/块级 IO 操作
│   ├── sections.rs     # 各区域聚合容器
│   ├── sections/
│   │   ├── header.rs   # Header 解析与验证
│   │   ├── bat.rs      # 块分配表（BAT）
│   │   ├── log.rs      # 日志解析与回放
│   │   └── metadata.rs # 元数据解析
│   ├── types.rs        # 基础类型（GUID）
│   ├── error.rs        # 错误定义
│   └── common/
│       ├── constants.rs # 格式常量
│       └── mod.rs      # 通用工具
├── vhdx-cli/           # CLI 工具
│   └── src/
│       ├── main.rs     # 入口
│       ├── cli.rs      # 命令行定义
│       └── commands/   # 各子命令实现
├── tests/              # 集成测试
└── misc/               # 规范文档与参考
```

## 参考规范

- [Microsoft VHDX 规范 (MS-VHDX)](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-vhdx/)

## 许可证

待定
