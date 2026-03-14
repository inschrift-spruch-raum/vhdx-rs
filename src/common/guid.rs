//! GUID handling for VHDX
//!
//! VHDX uses 128-bit GUIDs in various structures.
//! All GUIDs are stored in little-endian format as per VHDX specification.

use std::fmt;
use uuid::Uuid;

/// A 128-bit GUID as used in VHDX files.
///
/// This is a newtype wrapper around `uuid::Uuid` that ensures
/// correct little-endian byte order for VHDX file compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid(pub Uuid);

impl Guid {
    /// Create a new random GUID (version 4).
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a nil (all-zero) GUID.
    pub fn nil() -> Self {
        Self(Uuid::nil())
    }

    /// Create a GUID from a little-endian byte array.
    ///
    /// This is the format used in VHDX files.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes_le(bytes))
    }

    /// Convert the GUID to a little-endian byte array.
    ///
    /// This is the format used in VHDX files.
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0.to_bytes_le()
    }

    /// Check if this is a nil (all-zero) GUID.
    pub fn is_nil(&self) -> bool {
        self.0.is_nil()
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for Guid {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<Guid> for Uuid {
    fn from(guid: Guid) -> Self {
        guid.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_nil() {
        let nil_guid = Guid::nil();
        assert!(nil_guid.is_nil());
        assert_eq!(nil_guid.to_bytes(), [0u8; 16]);
    }

    #[test]
    fn test_guid_roundtrip() {
        // Test with a known GUID byte array
        let bytes: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD,
            0x4A, 0x08,
        ];
        let guid = Guid::from_bytes(bytes);
        let roundtrip = guid.to_bytes();
        assert_eq!(bytes, roundtrip);
    }

    #[test]
    fn test_guid_display() {
        // Create GUID from known bytes and verify display format
        let bytes: [u8; 16] = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD,
            0x4A, 0x08,
        ];
        let guid = Guid::from_bytes(bytes);
        let s = format!("{}", guid);
        assert_eq!(s, "2dc27766-f623-4200-9d64-115e9bfd4a08");
    }

    #[test]
    fn test_guid_from_uuid() {
        let uuid = Uuid::new_v4();
        let guid: Guid = uuid.into();
        let uuid_back: Uuid = guid.into();
        assert_eq!(uuid, uuid_back);
    }
}
