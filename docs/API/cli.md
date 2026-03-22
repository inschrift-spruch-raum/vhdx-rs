# CLI Tool

<- [Back to API Documentation](../API.md)

## Overview

Command-line interface for VHDX file operations.

## CLI Tool Tree

```
vhdx-tool::
├── info [file]                             # 查看VHDX文件信息
│   └── --format <json|text>                # 输出格式 (默认: text)
│
├── create <path>                           # 创建VHDX文件
│   ├── --size <size>                       # 虚拟磁盘大小 (必需)
│   ├── --type <dynamic|fixed|differencing> # 磁盘类型 (默认: dynamic)
│   ├── --block-size <size>                 # 块大小 (默认: 32MB)
│   ├── --parent <path>                     # 父磁盘路径 (差分磁盘必需)
│   └── --force                             # 覆盖已存在文件
│
├── check [file]                            # 检查文件完整性
│   ├── --repair                            # 尝试修复
│   └── --log-replay                        # 重放日志
│
├── sections [file]                         # 查看内部Sections
│   ├── header                              # 查看Header Section
│   ├── bat                                 # 查看BAT Entries
│   ├── metadata                            # 查看Metadata
│   └── log                                 # 查看Log Entries
│
└── diff [file]                             # 差分磁盘操作
    ├── parent                              # 显示父磁盘路径
    └── chain                               # 显示磁盘链
```

## Commands

### info [file]

View VHDX file information.

**Options:**

- `--format <json|text>` - Output format (default: text)

### create <path>

Create a new VHDX file.

**Options:**

- `--size <size>` - Virtual disk size (required)
- `--type <dynamic|fixed|differencing>` - Disk type (default: dynamic)
- `--block-size <size>` - Block size (default: 32MB)
- `--parent <path>` - Parent disk path (required for differencing disks)
- `--force` - Overwrite existing file

### check [file]

Check file integrity.

**Options:**

- `--repair` - Attempt to repair
- `--log-replay` - Replay log

### sections [file]

View internal sections.

**Subcommands:**

- `header` - View Header Section
- `bat` - View BAT Entries
- `metadata` - View Metadata
- `log` - View Log Entries

### diff [file]

Differencing disk operations.

**Subcommands:**

- `parent` - Show parent disk path
- `chain` - Show disk chain
