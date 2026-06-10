# vhdx-rs

`vhdx-rs` is a Rust library for working with VHDX (Virtual Hard Disk v2) files, with a companion command-line tool, `vhdx-tool`.

The project focuses on the on-disk structures defined by the MS-VHDX specification: headers, region tables, the block allocation table, metadata, log entries, differencing disk metadata, and virtual-sector I/O.

## Features

- Open and create dynamic, fixed, and differencing VHDX files.
- Inspect VHDX sections through zero-copy views where possible.
- Read headers, region tables, BAT entries, metadata items, and log entries.
- Validate files against structural and MS-VHDX consistency rules.
- Configure log replay behavior when opening files.
- Resolve and validate differencing disk parent locator data.
- Read and write virtual sectors through the `IO` and `Sector` APIs.
- Enable optional GPT integration with the `gpt` feature.

## Install

Add the library crate:

```bash
cargo add vhdx-rs
```

Install the CLI tool:

```bash
cargo install vhdx-tool
```

To enable GPT support in a library project:

```toml
vhdx-rs = { version = "0.1", features = ["gpt"] }
```

## Library Usage

```rust
use vhdx::{LogReplayPolicy, Medium};

fn main() -> vhdx::Result<()> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .open("disk.vhdx")?;
    let mut file = Medium::open(inner)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()?;

    let metadata = file.sections()?.metadata()?;
    let size = metadata.items().virtual_disk_size()?;
    println!("virtual size: {size} bytes");

    let issues = file.validator()?.validate_file()?;
    for issue in issues {
        println!("[{}] {}: {}", issue.section(), issue.code(), issue.message());
    }

    Ok(())
}
```

Create a new dynamic disk:

```rust
use vhdx::Medium;

fn main() -> vhdx::Result<()> {
    let inner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("new.vhdx")?;

    Medium::create(inner)
        .size(10 * 1024 * 1024 * 1024)
        .finish()?;

    Ok(())
}
```

## CLI Usage

Show general file information:

```bash
vhdx-tool info disk.vhdx
vhdx-tool info disk.vhdx --format json
```

Create a VHDX file:

```bash
vhdx-tool create disk.vhdx --size 10GB
vhdx-tool create fixed.vhdx --size 10GB --type fixed
vhdx-tool create child.vhdx --size 10GB --type differencing --parent parent.vhdx
```

Run validation and inspect internal structures:

```bash
vhdx-tool check disk.vhdx
vhdx-tool check disk.vhdx --log-replay
vhdx-tool sections disk.vhdx header
vhdx-tool sections disk.vhdx metadata
vhdx-tool diff disk.vhdx parent
vhdx-tool diff disk.vhdx chain
```

## Development

Build and test the workspace with Cargo:

```bash
cargo check --workspace --all-features
cargo test --workspace --all-features
```

The root crate publishes the library as `vhdx-rs` while exposing the Rust module name `vhdx`. The workspace also contains the `vhdx-tool` binary crate.

## License

This project is licensed under the MIT license.
