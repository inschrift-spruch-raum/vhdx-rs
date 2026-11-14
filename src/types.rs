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

/// A GUID stored as raw 16 bytes in the Windows mixed-endian on-disk layout.
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
            bytes: uuid.to_bytes_le(),
        }
    }

    /// Convert to the underlying `uuid::Uuid`.
    ///
    /// The on-disk bytes are a Windows GUID: the first three fields are
    /// little-endian, so `{CAA16737-FA36-4D43-B3B6-33F0AA44E76B}` is stored as
    /// `37 67 A1 CA 36 FA 43 4D B3 B6 33 F0 AA 44 E7 6B`.
    #[must_use]
    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes_le(self.bytes)
    }

    /// Parse a GUID string, with optional hyphens and surrounding braces.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid GUID string.
    pub fn parse_braced(input: &str) -> std::result::Result<Self, uuid::Error> {
        let trimmed = input.trim();
        let guid = trimmed
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
            .unwrap_or(trimmed);
        uuid::Uuid::parse_str(guid).map(|uuid| Self {
            bytes: uuid.to_bytes_le(),
        })
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
        // MS-VHDX 2.6.2.1: the File Parameters item is CAA16737-FA36-4D43-B3B6-33F0AA44E76B.
        let guid = StandardItems::FILE_PARAMETERS;
        let displayed = format!("{guid}");
        assert_eq!(displayed, "caa16737-fa36-4d43-b3b6-33f0aa44e76b");
    }

    #[test]
    fn guid_parse_braced_matches_on_disk_bytes() {
        let guid = Guid::parse_braced("{CAA16737-FA36-4D43-B3B6-33F0AA44E76B}").unwrap();
        assert_eq!(guid, StandardItems::FILE_PARAMETERS);
        assert_eq!(
            Guid::parse_braced("caa16737-fa36-4d43-b3b6-33f0aa44e76b").unwrap(),
            guid
        );
    }

    #[test]
    fn guid_text_matches_hyper_v_parent_linkage() {
        // A parent's DataWriteGuid as stored in its header, and the parent_linkage value
        // Hyper-V (New-VHD -Differencing) wrote for it into the child's parent locator.
        let on_disk = [
            0x30, 0x7A, 0x79, 0x8E, 0xA2, 0x8E, 0x4C, 0xE4, 0xA8, 0xF5, 0x96, 0x3C, 0xAC, 0xB2,
            0xAC, 0xD6,
        ];
        let linkage = "{8e797a30-8ea2-e44c-a8f5-963cacb2acd6}";
        let guid = Guid::from_bytes(on_disk);
        assert_eq!(Guid::parse_braced(linkage).unwrap(), guid);
        assert_eq!(format!("{{{guid}}}"), linkage);
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
    fn guid_new_v4_uses_windows_layout() {
        // Repeated samples prevent a random byte from masking misplaced version bits.
        for _ in 0..32 {
            let guid = Guid::new_v4();
            let bytes = guid.to_bytes();

            // Windows GUIDs store the version in the high nibble of byte 7.
            assert_eq!(bytes[7] & 0xf0, 0x40);
            assert_eq!(bytes[8] & 0xc0, 0x80);
            assert_eq!(guid.to_uuid().get_version(), Some(uuid::Version::Random));
            assert_eq!(guid.to_uuid().get_variant(), uuid::Variant::RFC4122);
        }
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
