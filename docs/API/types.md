# Types

← [Back to API Documentation](../API.md)

## Overview
Core types used throughout the VHDX library.

## Guid

GUID type for VHDX file identifiers.

## Error

Error type for VHDX operations.

### Variants

| Variant | Description |
|---------|-------------|
| `Io(std::io::Error)` | IO operation error |
| `InvalidFile(String)` | Invalid file format |
| `CorruptedHeader(String)` | Header corruption detected |
| `InvalidChecksum { expected: u32, actual: u32 }` | Checksum mismatch |
| `UnsupportedVersion(u16)` | Unsupported VHDX version |
| `InvalidBlockState(u8)` | Invalid block state value |
| `ParentNotFound { path: PathBuf }` | Parent disk not found |
| `ParentMismatch { expected: Guid, actual: Guid }` | Parent GUID mismatch |
| `LogReplayRequired` | Log replay required for consistency |
| `InvalidParameter(String)` | Invalid parameter provided |
| `MetadataNotFound { guid: Guid }` | Metadata item not found |
| `ReadOnly` | Write operation on read-only file |

## Module Exports

```rust
// Core types
pub use error::{Error, Result};
pub use types::Guid;
```
