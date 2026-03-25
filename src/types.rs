//! Type definitions for VHDX

use std::fmt;

/// GUID (Globally Unique Identifier) type
///
/// VHDX uses GUIDs extensively for identifying regions, metadata items, etc.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    data: [u8; 16],
}

impl Guid {
    /// Create a new GUID from raw bytes
    pub const fn from_bytes(data: [u8; 16]) -> Self {
        Self { data }
    }

    /// Get the raw bytes of the GUID
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.data
    }

    /// Create a nil GUID (all zeros)
    pub const fn nil() -> Self {
        Self { data: [0; 16] }
    }

    /// Check if this is a nil GUID
    pub fn is_nil(&self) -> bool {
        self.data == [0; 16]
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format as standard GUID string: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
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

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl From<[u8; 16]> for Guid {
    fn from(data: [u8; 16]) -> Self {
        Self::from_bytes(data)
    }
}

impl From<uuid::Uuid> for Guid {
    fn from(uuid: uuid::Uuid) -> Self {
        Self::from_bytes(uuid.as_bytes().to_owned())
    }
}

impl From<Guid> for uuid::Uuid {
    fn from(guid: Guid) -> Self {
        uuid::Uuid::from_bytes(guid.data)
    }
}

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
        // Test that GUID formats correctly
        let bytes = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let guid = Guid::from_bytes(bytes);
        let debug_str = format!("{:?}", guid);
        // Just verify it contains hyphens (standard GUID format)
        assert!(debug_str.contains('-'));
    }
}
