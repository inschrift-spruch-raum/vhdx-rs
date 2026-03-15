//! File Type Identifier for VHDX
//!
//! Stored at offset 0, 64KB in size

use crate::error::{Result, VhdxError};
use byteorder::{ByteOrder, LittleEndian};

/// File Type Identifier signature: "vhdxfile"
pub const FILE_TYPE_SIGNATURE: &[u8] = b"vhdxfile";

/// File Type Identifier structure
///
/// Stored at offset 0, 64KB in size
#[derive(Debug, Clone)]
pub struct FileTypeIdentifier {
    pub signature: [u8; 8],
    pub creator: Vec<u16>, // UTF-16 string
}

impl FileTypeIdentifier {
    /// Size of the file type identifier structure (64KB)
    pub const SIZE: usize = 64 * 1024;
    /// Offset in file
    pub const OFFSET: u64 = 0;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::FileTooSmall(
                "file size is insufficient".to_string(),
            ));
        }

        // Check signature
        let mut signature = [0u8; 8];
        signature.copy_from_slice(&data[0..8]);

        if signature != FILE_TYPE_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(FILE_TYPE_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        // Parse creator string (UTF-16 at offset 8, up to 512 bytes)
        let mut creator = Vec::new();
        for i in (8..520).step_by(2) {
            if i + 1 < data.len() {
                let ch = LittleEndian::read_u16(&data[i..i + 2]);
                if ch == 0 {
                    break;
                }
                creator.push(ch);
            }
        }

        Ok(FileTypeIdentifier { signature, creator })
    }

    /// Get creator as String
    pub fn creator_string(&self) -> Option<String> {
        if self.creator.is_empty() {
            None
        } else {
            String::from_utf16(&self.creator).ok()
        }
    }

    /// Create a new file type identifier
    pub fn new(creator: Option<&str>) -> Self {
        let mut signature = [0u8; 8];
        signature.copy_from_slice(FILE_TYPE_SIGNATURE);

        let creator = creator
            .map(|s| s.encode_utf16().collect())
            .unwrap_or_default();

        FileTypeIdentifier { signature, creator }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![0u8; Self::SIZE];

        // Signature
        data[0..8].copy_from_slice(&self.signature);

        // Creator (UTF-16)
        for (i, &ch) in self.creator.iter().enumerate() {
            let offset = 8 + i * 2;
            if offset + 1 < data.len() {
                LittleEndian::write_u16(&mut data[offset..offset + 2], ch);
            }
        }

        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_identifier() {
        let ft = FileTypeIdentifier::new(Some("TestCreator"));
        let bytes = ft.to_bytes();
        let ft2 = FileTypeIdentifier::from_bytes(&bytes).unwrap();

        assert_eq!(ft.signature, ft2.signature);
        assert_eq!(ft.creator, ft2.creator);
        assert_eq!(ft2.creator_string(), Some("TestCreator".to_string()));
    }
}
