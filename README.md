# vhdx-rs

`vhdx-rs` 是一个用于处理 VHDX（Virtual Hard Disk v2）文件的 Rust workspace。仓库包含 `vhdx-rs` 库 crate，用于打开、检查、创建、读取和有限写入 VHDX 文件；也包含位于 `vhdx-cli/` 的 `vhdx-tool` CLI，用于快速检查和基础维护流程。

本页是 quick-start 入口。更完整的 API 与 CLI 参考说明已经迁移到 `docs/` 下。

## 文档入口

- [`docs/index.md`](docs/index.md)：文档总入口
- [`docs/API.md`](docs/API.md)：API 与 CLI 总索引
- [`docs/structure.md`](docs/structure.md)：仓库结构参考
- [`docs/env.md`](docs/env.md)：开发环境与构建上下文

## 工作区结构

```text
.
├── Cargo.toml          # workspace + library package
├── src/                # vhdx-rs library source
├── tests/              # library integration tests
├── docs/               # current documentation set
└── vhdx-cli/           # vhdx-tool CLI crate
```

## 当前状态

当前可用：

- 以只读方式打开已有 VHDX 文件
- 创建固定盘和动态盘 VHDX 文件
- 读取虚拟磁盘数据
- 写入并刷新固定盘 payload 数据
- 检查 header、metadata、BAT 大小，以及高层差分盘元数据
- 运行 CLI `repair` 流程，以可写方式打开文件，并在需要时触发日志回放

当前限制：

- 动态盘写入仍未完整实现
- `IO::write_sectors` 仍未完整实现；如果需要当前更稳定的读写路径，请优先使用 `File::read` 和 `File::write`
- 差分盘支持仍是部分实现，包括不完整的父链处理
- CLI 对 BAT、日志和 repair 的输出仍偏摘要，而不是完整诊断报告

## 构建与测试

```bash
cargo build
cargo test
cargo test -p vhdx-tool
```

## 快速示例

### 库接口

```rust,no_run
use vhdx_rs::File;

fn main() -> Result<(), vhdx_rs::Error> {
    let file = File::open("disk.vhdx").finish()?;
    println!("virtual size: {}", file.virtual_disk_size());
    println!("block size: {}", file.block_size());
    Ok(())
}
```

### CLI

```bash
cargo run -p vhdx-tool -- create demo.vhdx --size 64MiB --disk-type fixed
cargo run -p vhdx-tool -- info demo.vhdx
```

如果你需要查看命令细节、库入口和当前实现边界，请继续阅读 [`docs/API.md`](docs/API.md)。如果你想先看完整文档地图和阅读顺序，请从 [`docs/index.md`](docs/index.md) 开始；如果你更关心仓库布局或本地开发准备，再继续阅读 [`docs/structure.md`](docs/structure.md) 与 [`docs/env.md`](docs/env.md)。
