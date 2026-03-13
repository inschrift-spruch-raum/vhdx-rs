//! VHDX Header Section structures and operations
//!
//! The header section contains:
//! - Header 1 (offset 64KB, 4KB)
//! - Header 2 (offset 128KB, 4KB)

use crate::crc32c::crc32c_with_zero_field;
use crate::error::{Result, VhdxError};
use crate::guid::Guid;
use byteorder::{ByteOrder, LittleEndian};

/// Header signature: "head"
pub const HEADER_SIGNATURE: &[u8] = b"head";

/// VHDX Header structure
///
/// 4KB structure at offset 64KB or 128KB
#[derive(Debug, Clone)]
pub struct VhdxHeader {
    pub signature: [u8; 4],
    pub checksum: u32,
    pub sequence_number: u64,
    pub file_write_guid: Guid,
    pub data_write_guid: Guid,
    pub log_guid: Guid,
    pub log_version: u16,
    pub version: u16,
    pub log_length: u32,
    pub log_offset: u64,
}

impl VhdxHeader {
    /// Size of header structure
    pub const SIZE: usize = 4096;
    /// Offset of Header 1
    pub const OFFSET_1: u64 = 64 * 1024;
    /// Offset of Header 2
    pub const OFFSET_2: u64 = 128 * 1024;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::FileTooSmall);
        }

        // Check signature
        let mut signature = [0u8; 4];
        signature.copy_from_slice(&data[0..4]);

        if &signature != HEADER_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(HEADER_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        // Read fields
        let checksum = LittleEndian::read_u32(&data[4..8]);
        let sequence_number = LittleEndian::read_u64(&data[8..16]);

        let mut file_write_guid = [0u8; 16];
        file_write_guid.copy_from_slice(&data[16..32]);

        let mut data_write_guid = [0u8; 16];
        data_write_guid.copy_from_slice(&data[32..48]);

        let mut log_guid = [0u8; 16];
        log_guid.copy_from_slice(&data[48..64]);

        let log_version = LittleEndian::read_u16(&data[64..66]);
        let version = LittleEndian::read_u16(&data[66..68]);
        let log_length = LittleEndian::read_u32(&data[68..72]);
        let log_offset = LittleEndian::read_u64(&data[72..80]);

        Ok(VhdxHeader {
            signature,
            checksum,
            sequence_number,
            file_write_guid: Guid::from_bytes(file_write_guid),
            data_write_guid: Guid::from_bytes(data_write_guid),
            log_guid: Guid::from_bytes(log_guid),
            log_version,
            version,
            log_length,
            log_offset,
        })
    }

    /// Verify the checksum
    pub fn verify_checksum(&self, data: &[u8]) -> bool {
        if data.len() < Self::SIZE {
            return false;
        }
        let calculated = crc32c_with_zero_field(data, 4, 4);
        calculated == self.checksum
    }

    /// Calculate checksum for this header
    pub fn calculate_checksum(&self, data: &[u8]) -> u32 {
        crc32c_with_zero_field(data, 4, 4)
    }

    /// Update checksum in place
    pub fn update_checksum(&mut self, data: &mut [u8]) {
        let checksum = self.calculate_checksum(data);
        self.checksum = checksum;
        LittleEndian::write_u32(&mut data[4..8], checksum);
    }

    /// Check if this header is valid (signature and checksum)
    pub fn is_valid(&self, data: &[u8]) -> bool {
        self.signature == HEADER_SIGNATURE && self.verify_checksum(data)
    }

    /// Create a new header with default values
    pub fn new(sequence_number: u64) -> Self {
        let mut signature = [0u8; 4];
        signature.copy_from_slice(HEADER_SIGNATURE);
        VhdxHeader {
            signature,
            checksum: 0,
            sequence_number,
            file_write_guid: Guid::new_v4(),
            data_write_guid: Guid::new_v4(),
            log_guid: Guid::new_v4(),
            log_version: 0,
            version: 1, // VHDX version 2
            log_length: 0,
            log_offset: 0,
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![0u8; Self::SIZE];

        // Signature
        data[0..4].copy_from_slice(&self.signature);

        // Checksum (will be calculated later)
        LittleEndian::write_u32(&mut data[4..8], self.checksum);

        // Sequence number
        LittleEndian::write_u64(&mut data[8..16], self.sequence_number);

        // GUIDs
        data[16..32].copy_from_slice(&self.file_write_guid.to_bytes());
        data[32..48].copy_from_slice(&self.data_write_guid.to_bytes());
        data[48..64].copy_from_slice(&self.log_guid.to_bytes());

        // Version fields
        LittleEndian::write_u16(&mut data[64..66], self.log_version);
        LittleEndian::write_u16(&mut data[66..68], self.version);
        LittleEndian::write_u32(&mut data[68..72], self.log_length);
        LittleEndian::write_u64(&mut data[72..80], self.log_offset);

        // Rest is already zeroed
        data
    }

    /// Check version compatibility
    pub fn check_version(&self) -> Result<()> {
        if self.version != 1 {
            return Err(VhdxError::UnsupportedVersion(self.version as u32));
        }
        if self.log_version != 0 {
            // Only valid if log_guid is zero (no log)
            if !self.log_guid.is_zero() {
                return Err(VhdxError::UnsupportedVersion(self.log_version as u32));
            }
        }
        Ok(())
    }
}

/// Read both headers and determine the current one
///
/// Returns (current_header_index, current_header, other_header)
/// Index 0 = Header 1 at 64KB, Index 1 = Header 2 at 128KB
pub fn read_headers(file: &mut std::fs::File) -> Result<(usize, VhdxHeader, VhdxHeader)> {
    use std::io::{Read, Seek, SeekFrom};

    // Read Header 1
    let mut header1_data = vec![0u8; VhdxHeader::SIZE];
    file.seek(SeekFrom::Start(VhdxHeader::OFFSET_1))?;
    file.read_exact(&mut header1_data)?;
    let header1 = VhdxHeader::from_bytes(&header1_data)?;

    // Read Header 2
    let mut header2_data = vec![0u8; VhdxHeader::SIZE];
    file.seek(SeekFrom::Start(VhdxHeader::OFFSET_2))?;
    file.read_exact(&mut header2_data)?;
    let header2 = VhdxHeader::from_bytes(&header2_data)?;

    // Determine which header is current
    let header1_valid = header1.is_valid(&header1_data);
    let header2_valid = header2.is_valid(&header2_data);

    match (header1_valid, header2_valid) {
        (true, true) => {
            // Both valid - use higher sequence number
            if header1.sequence_number > header2.sequence_number {
                Ok((0, header1, header2))
            } else {
                Ok((1, header2, header1))
            }
        }
        (true, false) => Ok((0, header1, header2)),
        (false, true) => Ok((1, header2, header1)),
        (false, false) => Err(VhdxError::NoValidHeader),
    }
}

/// Update headers safely (power-fail safe)
///
/// This updates the non-current header first, then the current header
pub fn update_headers(
    file: &mut std::fs::File,
    current_idx: usize,
    new_header: &VhdxHeader,
) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};

    // Determine which header to update first (the non-current one)
    let update_order = if current_idx == 0 {
        vec![(VhdxHeader::OFFSET_2, 1), (VhdxHeader::OFFSET_1, 0)]
    } else {
        vec![(VhdxHeader::OFFSET_1, 0), (VhdxHeader::OFFSET_2, 1)]
    };

    for (offset, _idx) in update_order {
        let mut data = new_header.to_bytes();

        // Calculate and update checksum
        let checksum = crc32c_with_zero_field(&data, 4, 4);
        LittleEndian::write_u32(&mut data[4..8], checksum);

        // Write to file
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&data)?;
        file.flush()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vhdx_header() {
        let header = VhdxHeader::new(1);
        let mut bytes = header.to_bytes();

        // Update checksum in the bytes
        let checksum = crc32c_with_zero_field(&bytes, 4, 4);
        LittleEndian::write_u32(&mut bytes[4..8], checksum);

        let header2 = VhdxHeader::from_bytes(&bytes).unwrap();
        assert!(header2.is_valid(&bytes));
        assert_eq!(header.sequence_number, header2.sequence_number);
        assert_eq!(header.version, header2.version);
    }
}
