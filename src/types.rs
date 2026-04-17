//! VHDX 基础类型定义
//!
//! 本模块定义了 VHDX 文件格式中使用的基础数据类型。
//! 核心类型 [`Guid`] 用于标识区域表条目（MS-VHDX §2.2.3.2）
//! 和元数据表条目（MS-VHDX §2.6.1.2）中的 128 位唯一标识符。

use std::fmt;

/// 128 位全局唯一标识符（GUID）
///
/// 在 VHDX 文件中，GUID 用于标识区域表条目（MS-VHDX §2.2.3.2）
/// 和元数据表条目（MS-VHDX §2.6.1.2）。
///
/// 内部以 16 字节 little-endian 字节数组存储。
/// 格式化输出时使用混合字节序（前 3 组字节翻转），符合 Microsoft GUID 的标准显示格式。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    /// 16 字节的 GUID 原始数据（little-endian 存储）
    data: [u8; 16],
}

impl Guid {
    /// 从 16 字节数组创建 GUID
    ///
    /// 字节序为 VHDX 文件中的原始存储顺序（little-endian）。
    #[must_use]
    pub const fn from_bytes(data: [u8; 16]) -> Self {
        Self { data }
    }

    /// 返回 GUID 的 16 字节原始数据引用
    ///
    /// 返回的字节序与 VHDX 文件中的存储顺序一致。
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.data
    }

    /// 创建全零（空）GUID
    ///
    /// 空 GUID 通常表示未设置或无效的标识符。
    #[must_use]
    pub const fn nil() -> Self {
        Self { data: [0; 16] }
    }

    /// 检查是否为全零（空）GUID
    ///
    /// # 返回值
    /// - `true` — GUID 的所有 16 个字节均为 0
    /// - `false` — GUID 包含非零字节
    #[must_use]
    pub fn is_nil(&self) -> bool {
        self.data == [0; 16]
    }
}

/// 实现 [`fmt::Debug`] trait，使用混合字节序格式化 GUID
///
/// 输出格式为 `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX`（大写十六进制），
/// 其中前 3 组字节（Data1、Data2、Data3）进行字节翻转，
/// 符合 Microsoft GUID 的标准显示格式。
impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.data[3],
            self.data[2],
            self.data[1],
            self.data[0],
            self.data[5],
            self.data[4],
            self.data[7],
            self.data[6],
            self.data[8],
            self.data[9],
            self.data[10],
            self.data[11],
            self.data[12],
            self.data[13],
            self.data[14],
            self.data[15]
        )
    }
}

/// 实现 [`fmt::Display`] trait，格式与 [`Debug`](fmt::Debug) 相同
impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// 支持从 16 字节数组直接转换为 GUID
impl From<[u8; 16]> for Guid {
    fn from(data: [u8; 16]) -> Self {
        Self::from_bytes(data)
    }
}

/// 支持从 [`uuid::Uuid`] 转换为 GUID
///
/// 直接复制 uuid 的 16 字节数据。
impl From<uuid::Uuid> for Guid {
    fn from(uuid: uuid::Uuid) -> Self {
        Self::from_bytes(uuid.as_bytes().to_owned())
    }
}

/// 支持从 GUID 转换为 [`uuid::Uuid`]
///
/// 直接使用 GUID 的 16 字节数据构造 uuid。
impl From<Guid> for uuid::Uuid {
    fn from(guid: Guid) -> Self {
        Self::from_bytes(guid.data)
    }
}

/// 默认值为全零（空）GUID
impl Default for Guid {
    fn default() -> Self {
        Self::nil()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_nil() {
        let guid = Guid::nil();
        assert!(guid.is_nil());
        assert_eq!(guid.as_bytes(), &[0; 16]);
    }

    #[test]
    fn test_guid_from_bytes() {
        let bytes = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let guid = Guid::from_bytes(bytes);
        assert_eq!(guid.as_bytes(), &bytes);
    }

    #[test]
    fn test_guid_debug_format() {
        let bytes = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let guid = Guid::from_bytes(bytes);
        let debug_str = format!("{guid:?}");
        assert!(debug_str.contains('-'));
    }
}
