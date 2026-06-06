use std::fmt;

/// A CRC-32C (Castagnoli) checksum value.
///
/// Wraps a raw `u32` and displays as `0x`-prefixed hex (e.g. `0xe3069283`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Crc32c(u32);

impl Crc32c {
    /// Create a `Crc32c` from a raw `u32` checksum value.
    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Return the raw `u32` checksum value.
    #[must_use]
    pub const fn value(&self) -> u32 {
        self.0
    }
}

impl fmt::Debug for Crc32c {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Crc32c({:#010x})", self.0)
    }
}

impl fmt::Display for Crc32c {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010x}", self.0)
    }
}

/// A GUID stored as raw 16 bytes (RFC 4122 / mixed-endian layout as on disk).
///
/// Internally uses `uuid::Uuid` for display and parsing, but stores `[u8; 16]`
/// for zero-copy compatibility with disk structures.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    bytes: [u8; 16],
}

impl Guid {
    /// Create a `Guid` from raw mixed-endian bytes as stored on disk.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// Create a zero GUID (all bytes zero).
    #[must_use]
    pub const fn zero() -> Self {
        Self { bytes: [0u8; 16] }
    }

    /// Return the raw 16 bytes.
    #[must_use]
    pub const fn to_bytes(&self) -> [u8; 16] {
        self.bytes
    }

    /// Generate a new random (v4) GUID.
    #[must_use]
    pub fn new_v4() -> Self {
        let uuid = uuid::Uuid::new_v4();
        Self {
            bytes: *uuid.as_bytes(),
        }
    }

    /// Convert to the underlying `uuid::Uuid`.
    #[must_use]
    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.bytes)
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({})", self.to_uuid().hyphenated())
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uuid().hyphenated())
    }
}

/// Standard metadata item GUID constants per MS-VHDX specification.
#[allow(non_snake_case)]
pub mod StandardItems {
    use crate::constants;
    use crate::types::Guid;

    pub const FILE_PARAMETERS: Guid = constants::FILE_PARAMETERS_GUID;
    pub const VIRTUAL_DISK_SIZE: Guid = constants::VIRTUAL_DISK_SIZE_GUID;
    pub const VIRTUAL_DISK_ID: Guid = constants::VIRTUAL_DISK_ID_GUID;
    pub const LOGICAL_SECTOR_SIZE: Guid = constants::LOGICAL_SECTOR_SIZE_GUID;
    pub const PHYSICAL_SECTOR_SIZE: Guid = constants::PHYSICAL_SECTOR_SIZE_GUID;
    pub const PARENT_LOCATOR: Guid = constants::PARENT_LOCATOR_GUID;
    pub const LOCATOR_TYPE_VHDX: Guid = constants::LOCATOR_TYPE_VHDX_GUID;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_from_bytes_roundtrip() {
        let bytes = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let guid = Guid::from_bytes(bytes);
        assert_eq!(guid.to_bytes(), bytes);
    }

    #[test]
    fn guid_display() {
        let guid = StandardItems::FILE_PARAMETERS;
        // from_bytes uses raw bytes directly with uuid crate
        let displayed = format!("{guid}");
        assert_eq!(displayed, "3767a1ca-36fa-434d-b3b6-33f0aa44e76b");
    }

    #[test]
    fn guid_debug() {
        let guid = StandardItems::FILE_PARAMETERS;
        let debugged = format!("{guid:?}");
        assert!(debugged.starts_with("Guid("));
        assert!(debugged.ends_with(')'));
    }

    #[test]
    fn guid_new_v4_is_unique() {
        let a = Guid::new_v4();
        let b = Guid::new_v4();
        assert_ne!(a, b);
    }

    #[test]
    fn standard_items_are_distinct() {
        let guids = [
            StandardItems::FILE_PARAMETERS,
            StandardItems::VIRTUAL_DISK_SIZE,
            StandardItems::VIRTUAL_DISK_ID,
            StandardItems::LOGICAL_SECTOR_SIZE,
            StandardItems::PHYSICAL_SECTOR_SIZE,
            StandardItems::PARENT_LOCATOR,
            StandardItems::LOCATOR_TYPE_VHDX,
        ];
        for (i, a) in guids.iter().enumerate() {
            for (j, b) in guids.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "StandardItems at index {i} and {j} should differ");
                }
            }
        }
    }
}
