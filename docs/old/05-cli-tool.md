# CLI 工具使用说明

本文档详细介绍 vhdx-tool 命令行工具的使用方法，包括安装、命令说明和使用示例。

---

## 1. 安装

### 1.1 从源码编译

```bash
# 克隆仓库
git clone <repository-url>
cd vhdx-rs

# 编译发布版本
cargo build --release

# 可执行文件位置
./target/release/vhdx-tool
```

### 1.2 添加到 PATH

**Linux/macOS**:
```bash
# 复制到系统目录
sudo cp target/release/vhdx-tool /usr/local/bin/

# 或创建符号链接
sudo ln -s $(pwd)/target/release/vhdx-tool /usr/local/bin/vhdx-tool
```

**Windows**:
```powershell
# 添加到环境变量 PATH
# 或复制到已知路径
copy target\release\vhdx-tool.exe C:\Windows\System32\
```

### 1.3 验证安装

```bash
vhdx-tool --help
```

输出示例：
```
VHDX (Virtual Hard Disk v2) command line tool

Usage: vhdx-tool <COMMAND>

Commands:
  info     Display information about a VHDX file
  create   Create a new VHDX file
  read     Read data from VHDX
  write    Write data to VHDX
  check    Check VHDX file integrity
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

---

## 2. 命令概览

| 命令 | 功能 | 常用程度 |
|------|------|----------|
| `info` | 显示 VHDX 文件信息 | ★★★★★ |
| `create` | 创建新的 VHDX 文件 | ★★★★★ |
| `read` | 从 VHDX 读取数据 | ★★★☆☆ |
| `write` | 写入数据到 VHDX | ★★★☆☆ |
| `check` | 检查 VHDX 完整性 | ★★★★☆ |

---

## 3. info 命令

显示 VHDX 文件的详细信息。

### 3.1 语法

```bash
vhdx-tool info <PATH>
```

### 3.2 参数

| 参数 | 说明 | 必需 |
|------|------|------|
| `PATH` | VHDX 文件路径 | 是 |

### 3.3 示例

**显示基本信息**:
```bash
vhdx-tool info disk.vhdx
```

输出示例：
```
VHDX File: disk.vhdx
============================
Virtual Disk Size: 10737418240 bytes (10.00 GB)
Block Size: 33554432 bytes (32.00 MB)
Logical Sector Size: 512 bytes
Physical Sector Size: 4096 bytes
Disk Type: Dynamic
Virtual Disk ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

**带创建者信息**:
```bash
vhdx-tool info created_by_windows.vhdx
```

输出示例：
```
VHDX File: created_by_windows.vhdx
============================
Virtual Disk Size: 21474836480 bytes (20.00 GB)
Block Size: 33554432 bytes (32.00 MB)
Logical Sector Size: 512 bytes
Physical Sector Size: 4096 bytes
Disk Type: Fixed
Virtual Disk ID: b2c3d4e5-f6a7-8901-bcde-f23456789012
Creator: Windows 10.0.19044.0
```

**差异磁盘信息**:
```bash
vhdx-tool info snapshot.vhdx
```

输出示例：
```
VHDX File: snapshot.vhdx
============================
Virtual Disk Size: 10737418240 bytes (10.00 GB)
Block Size: 33554432 bytes (32.00 MB)
Logical Sector Size: 512 bytes
Physical Sector Size: 4096 bytes
Disk Type: Differencing
Virtual Disk ID: c3d4e5f6-a7b8-9012-cdef-345678901234
Has Parent: Yes
```

---

## 4. create 命令

创建新的 VHDX 文件。

### 4.1 语法

```bash
vhdx-tool create <PATH> [OPTIONS]
```

### 4.2 参数

| 参数 | 短选项 | 说明 | 默认值 |
|------|--------|------|--------|
| `PATH` | - | 新 VHDX 文件路径 | - |
| `--size` | `-s` | 虚拟磁盘大小 | 必需 |
| `--type` | `-t` | 磁盘类型 (fixed/dynamic/differencing) | dynamic |
| `--block-size` | `-b` | 块大小 | 32MB |
| `--logical-sector` | - | 逻辑扇区大小 (512/4096) | 512 |
| `--physical-sector` | - | 物理扇区大小 (512/4096) | 4096 |
| `--parent` | `-p` | 父磁盘路径（差异磁盘） | - |

### 4.3 大小格式

支持以下格式：

| 格式 | 说明 | 示例 |
|------|------|------|
| 纯数字 | 字节 | `--size 10737418240` |
| K/KB | 千字节 | `--size 10485760K` |
| M/MB | 兆字节 | `--size 10240M` |
| G/GB | 吉字节 | `--size 10G` |
| T/TB | 太字节 | `--size 1T` |

### 4.4 示例

**创建 10GB 动态磁盘**:
```bash
vhdx-tool create disk.vhdx --size 10G
# 或
vhdx-tool create disk.vhdx -s 10GB
```

**创建 100GB 固定磁盘**:
```bash
vhdx-tool create disk.vhdx --size 100G --type fixed
# 或
vhdx-tool create disk.vhdx -s 100G -t fixed
```

**自定义块大小**:
```bash
vhdx-tool create disk.vhdx --size 50G --block-size 1M
vhdx-tool create disk.vhdx --size 50G -b 64M
```

**自定义扇区大小**:
```bash
# 4K 扇区磁盘
vhdx-tool create disk.vhdx --size 10G --logical-sector 4096 --physical-sector 4096
```

**创建差异磁盘**:
```bash
# 基于 parent.vhdx 创建差异磁盘
vhdx-tool create snapshot.vhdx --size 10G --type differencing --parent parent.vhdx
# 或
vhdx-tool create snapshot.vhdx -s 10G -t differencing -p parent.vhdx
```

**完整示例**:
```bash
vhdx-tool create mydisk.vhdx \
  --size 50G \
  --type dynamic \
  --block-size 32M \
  --logical-sector 512 \
  --physical-sector 4096
```

### 4.5 输出示例

```
Successfully created VHDX file: disk.vhdx
  Size: 10737418240 bytes (10.00 GB)
  Type: Dynamic
  Block size: 33554432 bytes (32.00 MB)
  Logical sector: 512 bytes
  Physical sector: 4096 bytes
```

---

## 5. read 命令

从 VHDX 文件读取数据。

### 5.1 语法

```bash
vhdx-tool read <PATH> [OPTIONS]
```

### 5.2 参数

| 参数 | 短选项 | 说明 | 必需 |
|------|--------|------|------|
| `PATH` | - | VHDX 文件路径 | 是 |
| `--offset` | `-o` | 读取起始偏移（字节） | 是 |
| `--length` | `-l` | 读取长度（字节） | 是 |
| `--output` | `-O` | 输出文件路径（默认 stdout） | 否 |

### 5.3 示例

**读取到标准输出**:
```bash
# 读取前 1024 字节到屏幕
vhdx-tool read disk.vhdx --offset 0 --length 1024

# 十六进制查看
vhdx-tool read disk.vhdx -o 0 -l 512 | xxd
```

**读取到文件**:
```bash
# 读取 MBR（前 512 字节）
vhdx-tool read disk.vhdx --offset 0 --length 512 --output mbr.bin

# 读取分区表
vhdx-tool read disk.vhdx -o 446 -l 64 -O partition_table.bin
```

**读取引导扇区**:
```bash
# 假设分区从偏移 1MB 开始
vhdx-tool read disk.vhdx --offset 1048576 --length 512 --output bootsector.bin
```

**提取整个分区**:
```bash
# 提取 1GB 分区（从 1MB 偏移开始）
vhdx-tool read disk.vhdx --offset 1048576 --length 1073741824 --output partition.raw
```

---

## 6. write 命令

写入数据到 VHDX 文件。

### 6.1 语法

```bash
vhdx-tool write <PATH> [OPTIONS]
```

### 6.2 参数

| 参数 | 短选项 | 说明 | 必需 |
|------|--------|------|------|
| `PATH` | - | VHDX 文件路径 | 是 |
| `--offset` | `-o` | 写入偏移（字节） | 是 |
| `--input` | `-i` | 输入文件路径（默认 stdin） | 否 |

### 6.3 示例

**从文件写入**:
```bash
# 写入 MBR
vhdx-tool write disk.vhdx --offset 0 --input mbr.bin

# 写入引导扇区
vhdx-tool write disk.vhdx -o 1048576 -i bootsector.bin
```

**从标准输入写入**:
```bash
# 使用 echo 写入字符串
echo -n "Hello, VHDX!" | vhdx-tool write disk.vhdx --offset 0

# 使用 dd 复制数据
dd if=/dev/zero bs=1M count=10 | vhdx-tool write disk.vhdx --offset 0
```

**修改特定字节**:
```bash
# 使用 printf 写入二进制数据
printf '\x55\xAA' | vhdx-tool write disk.vhdx --offset 510
```

---

## 7. check 命令

检查 VHDX 文件的完整性和有效性。

### 7.1 语法

```bash
vhdx-tool check <PATH>
```

### 7.2 参数

| 参数 | 说明 | 必需 |
|------|------|------|
| `PATH` | VHDX 文件路径 | 是 |

### 7.3 示例

**检查有效文件**:
```bash
vhdx-tool check disk.vhdx
```

输出示例：
```
Checking VHDX file: disk.vhdx
✓ File opened successfully
✓ Headers validated
✓ Region table validated
✓ Metadata parsed
✓ BAT loaded

File is valid!
```

**检查差异磁盘**:
```bash
vhdx-tool check snapshot.vhdx
```

输出示例：
```
Checking VHDX file: snapshot.vhdx
✓ File opened successfully
✓ Headers validated
✓ Region table validated
✓ Metadata parsed
✓ BAT loaded
✓ Parent disk accessible

File is valid!
```

**检查损坏的文件**:
```bash
vhdx-tool check corrupted.vhdx
```

输出示例：
```
Checking VHDX file: corrupted.vhdx
✗ File check failed: Invalid checksum
```

### 7.4 检查项目

| 检查项 | 说明 |
|--------|------|
| File Type Identifier | "vhdxfile" 签名验证 |
| Headers | 双头签名、校验和、版本 |
| Region Table | 签名、校验和、必需区域存在性 |
| Metadata | 所有必需元数据项解析 |
| BAT | BAT 条目数量和状态 |
| Parent Disk | 差异磁盘的父磁盘可访问性 |

---

## 8. 使用场景示例

### 8.1 创建测试磁盘

```bash
# 创建 1GB 动态磁盘用于测试
vhdx-tool create test.vhdx --size 1G --type dynamic

# 写入测试数据
dd if=/dev/urandom bs=1M count=100 | vhdx-tool write test.vhdx --offset 0

# 验证
vhdx-tool check test.vhdx
vhdx-tool info test.vhdx
```

### 8.2 数据恢复

```bash
# 检查损坏的磁盘
vhdx-tool check damaged.vhdx

# 尝试读取可恢复的数据
vhdx-tool read damaged.vhdx --offset 0 --length 1048576 --output recovered.bin
```

### 8.3 创建快照链

```bash
# 1. 创建基础磁盘
vhdx-tool create base.vhdx --size 100G --type fixed

# 2. 创建第一个快照
vhdx-tool create snapshot1.vhdx --size 100G --type differencing --parent base.vhdx

# 3. 创建第二个快照（基于第一个）
vhdx-tool create snapshot2.vhdx --size 100G --type differencing --parent snapshot1.vhdx

# 4. 检查链
vhdx-tool check snapshot2.vhdx
```

### 8.4 批量处理

```bash
# 批量检查多个文件
for file in *.vhdx; do
    echo "Checking: $file"
    vhdx-tool check "$file" || echo "FAILED: $file"
done

# 批量获取信息
for file in *.vhdx; do
    vhdx-tool info "$file" | grep "Disk Type"
done
```

---

## 9. 错误处理

### 9.1 常见错误

| 错误信息 | 原因 | 解决方案 |
|----------|------|----------|
| `PermissionDenied` | 无权限访问文件 | 检查文件权限，使用 sudo（Linux）或以管理员运行（Windows） |
| `InvalidSignature` | 不是有效的 VHDX 文件 | 确认文件未损坏，是有效的 VHDX 格式 |
| `NoValidHeader` | 两个 Header 都无效 | 文件可能严重损坏，尝试数据恢复 |
| `ParentNotFound` | 差异磁盘的父磁盘不存在 | 检查父磁盘路径，使用绝对路径 |
| `InvalidOffset` | 偏移超出虚拟磁盘大小 | 检查 --offset 参数，确保在有效范围内 |
| `FileTooSmall` | 文件太小，无法包含有效结构 | 文件可能未正确创建或已截断 |

### 9.2 退出码

| 退出码 | 含义 |
|--------|------|
| 0 | 成功 |
| 1 | 一般错误（文件损坏、参数错误等） |
| 2 | 参数解析错误（clap 返回） |

---

## 10. 参考文档

- [04-file-operations.md](./04-file-operations.md) - 文件操作 API
- [06-api-guide.md](./06-api-guide.md) - 编程 API 使用指南
