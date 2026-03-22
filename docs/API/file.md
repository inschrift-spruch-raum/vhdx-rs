# File API

[Back to API Documentation](../API.md)

## Overview

Core file operations including opening, creating, and accessing VHDX files.

## API Tree

```
File                                    # 核心 API
├── open(path) -> File::OpenOptions     # 链式打开
├── create(path) -> File::CreateOptions # 链式创建
├── sections(&self) -> &Sections        # 获取所有sections
├── io(&self) -> IO                    # 获取IO模块
└── inner(&self) -> &std::fs::File

└── OpenOptions                         # 关联类型：打开选项
    ├── write(self) -> Self             # 启用写权限（RW）
    └── finish(self) -> Result<File>    # 完成打开

└── CreateOptions                          # 关联类型：创建选项
    ├── size(self, u64) -> Self            # 必需：虚拟磁盘大小
    ├── fixed(self, bool) -> Self          # 可选：固定磁盘
    ├── has_parent(self, bool) -> Self     # 可选：差分磁盘
    ├── block_size(self, u32) -> Self      # 可选：块大小
    └── finish(self) -> Result<File>       # 完成创建
```

## Detailed Design

### 1. File - Core API

Core VHDX file handle providing access to sections and IO operations.

```rust
pub struct File;

impl File {
    /// 打开现有 VHDX 文件（只读默认）
    /// 返回 OpenOptions 用于链式配置
    pub fn open(path: impl AsRef<Path>) -> File::OpenOptions;

    /// 创建新 VHDX 文件
    /// 返回 CreateOptions 用于链式配置
    pub fn create(path: impl AsRef<Path>) -> File::CreateOptions;

    /// 获取所有Section的容器（懒加载）
    pub fn sections(&self) -> &Sections;

    /// 获取IO模块（用于扇区级读写）
    /// 懒加载：内部Sector缓存按需从文件读取
    pub fn io(&self) -> IO;

    /// 获取底层文件句柄（std::fs::File）
    /// 用户可通过此句柄直接进行底层 IO 操作
    pub fn inner(&self) -> &std::fs::File;
}
```

### 2. File::OpenOptions

Builder pattern for configuring file open operations.

```rust
impl File {
    pub struct OpenOptions;
}

impl File::OpenOptions {
    /// 启用写权限（默认为只读）
    pub fn write(self) -> Self;

    /// 完成打开操作
    pub fn finish(self) -> Result<File>;
}
```

### 3. File::CreateOptions

Builder pattern for configuring file creation operations.

```rust
impl File {
    pub struct CreateOptions;
}

impl File::CreateOptions {
    /// 设置虚拟磁盘大小（必需）
    pub fn size(self, virtual_size: u64) -> Self;

    /// 设置是否为固定磁盘（可选，默认 Dynamic）
    pub fn fixed(self, fixed: bool) -> Self;

    /// 设置是否为差分磁盘（可选，默认 false）
    pub fn has_parent(self, has_parent: bool) -> Self;

    /// 设置块大小（可选，默认 32MB）
    pub fn block_size(self, size: u32) -> Self;

    /// 完成创建操作
    pub fn finish(self) -> Result<File>;
}
```

## Usage Flow

1. **Open existing file**: `File::open(path)?.finish()?`
2. **Open with write**: `File::open(path)?.write().finish()?`
3. **Create new file**: `File::create(path)?.size(N).finish()?`
4. **Access sections**: `file.sections()`
5. **Access IO**: `file.io()`
