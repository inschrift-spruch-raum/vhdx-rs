use std::fmt;

/// A GUID stored as raw 16 bytes (RFC 4122 / mixed-endian layout as on disk).
///
/// Internally uses `uuid::Uuid` for display and parsing, but stores `[u8; 16]`
/// for zero-copy compatibility with disk structures.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    bytes: [u8; 16],
}

impl Guid {
    /// Create a `Guid` from raw little-endian bytes as stored on disk.
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
    use super::Guid;

    pub const FILE_PARAMETERS: Guid = Guid::from_bytes([
        0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
        0xE7, 0x6B,
    ]);

    pub const VIRTUAL_DISK_SIZE: Guid = Guid::from_bytes([
        0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B,
        0xF4, 0xB8,
    ]);

    pub const VIRTUAL_DISK_ID: Guid = Guid::from_bytes([
        0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00,
        0xC7, 0x46,
    ]);

    pub const LOGICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA,
        0xAB, 0x5F,
    ]);

    pub const PHYSICAL_SECTOR_SIZE: Guid = Guid::from_bytes([
        0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51,
        0xC5, 0x56,
    ]);

    pub const PARENT_LOCATOR: Guid = Guid::from_bytes([
        0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34,
        0xAB, 0x0C,
    ]);

    /// VHDX Parent Locator Type GUID.
    pub const LOCATOR_TYPE_VHDX: Guid = Guid::from_bytes([
        0xB7, 0xEF, 0x4A, 0xB0, 0x9E, 0xD1, 0x81, 0x4A, 0xB7, 0x89, 0x25, 0xB8, 0xE9, 0x44,
        0x59, 0x13,
    ]);
}

/// Re-export for convenience.
pub use StandardItems::*;

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
