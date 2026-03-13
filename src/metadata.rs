//! VHDX Metadata Region structures and operations
//!
//! The metadata region contains file parameters and disk properties

use crate::error::{Result, VhdxError};
use crate::guid::Guid;
use byteorder::{ByteOrder, LittleEndian};

/// Metadata Table signature: "metadata"
pub const METADATA_SIGNATURE: &[u8] = b"metadata";

/// File Parameters GUID: CAA16737-FA36-4D43-B3B6-33F0AA44E76B
pub const FILE_PARAMETERS_GUID: Guid = Guid([
    0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44, 0xE7, 0x6B,
]);

/// Virtual Disk Size GUID: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
pub const VIRTUAL_DISK_SIZE_GUID: Guid = Guid([
    0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B, 0xF4, 0xB8,
]);

/// Virtual Disk ID GUID: BECA12AB-B2E6-4523-93EF-C309E000C746
pub const VIRTUAL_DISK_ID_GUID: Guid = Guid([
    0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00, 0xC7, 0x46,
]);

/// Logical Sector Size GUID: 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
pub const LOGICAL_SECTOR_SIZE_GUID: Guid = Guid([
    0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA, 0xAB, 0x5F,
]);

/// Physical Sector Size GUID: CDA348C7-445D-4471-9CC9-E9885251C556
pub const PHYSICAL_SECTOR_SIZE_GUID: Guid = Guid([
    0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51, 0xC5, 0x56,
]);

/// Parent Locator GUID: A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
pub const PARENT_LOCATOR_GUID: Guid = Guid([
    0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34, 0xAB, 0x0C,
]);

/// Metadata Table Header
#[derive(Debug, Clone)]
pub struct MetadataTableHeader {
    pub signature: [u8; 8],
    pub entry_count: u16,
}

impl MetadataTableHeader {
    /// Size of header
    pub const SIZE: usize = 64 * 1024; // 64KB

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::FileTooSmall);
        }

        let mut signature = [0u8; 8];
        signature.copy_from_slice(&data[0..8]);

        if &signature != METADATA_SIGNATURE {
            return Err(VhdxError::InvalidSignature {
                expected: String::from_utf8_lossy(METADATA_SIGNATURE).to_string(),
                got: String::from_utf8_lossy(&signature).to_string(),
            });
        }

        let entry_count = LittleEndian::read_u16(&data[10..12]);

        Ok(MetadataTableHeader {
            signature,
            entry_count,
        })
    }
}

/// Metadata Table Entry
#[derive(Debug, Clone)]
pub struct MetadataTableEntry {
    pub item_id: Guid,
    pub offset: u32,
    pub length: u32,
    pub is_user: bool,
    pub is_virtual_disk: bool,
}

impl MetadataTableEntry {
    /// Size of entry
    pub const SIZE: usize = 32;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::FileTooSmall);
        }

        let mut item_id = [0u8; 16];
        item_id.copy_from_slice(&data[0..16]);
        let item_id = Guid::from_bytes(item_id);

        let offset = LittleEndian::read_u32(&data[16..20]);
        let length = LittleEndian::read_u32(&data[20..24]);
        let flags = LittleEndian::read_u32(&data[24..28]);

        let is_user = flags & 0x1 != 0;
        let is_virtual_disk = flags & 0x2 != 0;

        Ok(MetadataTableEntry {
            item_id,
            offset,
            length,
            is_user,
            is_virtual_disk,
        })
    }

    /// Check if this is a system metadata item
    pub fn is_system(&self) -> bool {
        !self.is_user
    }
}

/// File Parameters metadata item
#[derive(Debug, Clone)]
pub struct FileParameters {
    pub block_size: u32,
    pub has_parent: bool,
}

impl FileParameters {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::InvalidMetadata(
                "FileParameters too small".to_string(),
            ));
        }

        let block_size = LittleEndian::read_u32(&data[0..4]);
        let has_parent = LittleEndian::read_u32(&data[4..8]) != 0;

        // Validate block size (1MB to 256MB, must be 1MB multiple)
        if block_size < 1024 * 1024 || block_size > 256 * 1024 * 1024 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Invalid block size: {}",
                block_size
            )));
        }

        if block_size % (1024 * 1024) != 0 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Block size {} not 1MB aligned",
                block_size
            )));
        }

        Ok(FileParameters {
            block_size,
            has_parent,
        })
    }
}

/// Virtual Disk Size metadata item
#[derive(Debug, Clone)]
pub struct VirtualDiskSize {
    pub size: u64,
}

impl VirtualDiskSize {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(VhdxError::InvalidMetadata(
                "VirtualDiskSize too small".to_string(),
            ));
        }

        let size = LittleEndian::read_u64(&data[0..8]);

        if size == 0 {
            return Err(VhdxError::InvalidMetadata(
                "Virtual disk size cannot be zero".to_string(),
            ));
        }

        Ok(VirtualDiskSize { size })
    }
}

/// Sector Size metadata item
#[derive(Debug, Clone)]
pub struct SectorSize {
    pub size: u32,
}

impl SectorSize {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(VhdxError::InvalidMetadata(
                "SectorSize too small".to_string(),
            ));
        }

        let size = LittleEndian::read_u32(&data[0..4]);

        // Only 512 or 4096 are valid
        if size != 512 && size != 4096 {
            return Err(VhdxError::InvalidMetadata(format!(
                "Invalid sector size: {}. Must be 512 or 4096",
                size
            )));
        }

        Ok(SectorSize { size })
    }
}

/// Virtual Disk ID metadata item
#[derive(Debug, Clone)]
pub struct VirtualDiskId {
    pub guid: Guid,
}

impl VirtualDiskId {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(VhdxError::InvalidMetadata(
                "VirtualDiskId too small".to_string(),
            ));
        }

        let mut guid = [0u8; 16];
        guid.copy_from_slice(&data[0..16]);

        Ok(VirtualDiskId {
            guid: Guid::from_bytes(guid),
        })
    }
}

/// Parent Locator Entry
#[derive(Debug, Clone)]
pub struct ParentLocatorEntry {
    pub key_offset: u32,
    pub value_offset: u32,
    pub key_length: u16,
    pub value_length: u16,
}

impl ParentLocatorEntry {
    /// Size of entry
    pub const SIZE: usize = 12;

    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(VhdxError::FileTooSmall);
        }

        let key_offset = LittleEndian::read_u32(&data[0..4]);
        let value_offset = LittleEndian::read_u32(&data[4..8]);
        let key_length = LittleEndian::read_u16(&data[8..10]);
        let value_length = LittleEndian::read_u16(&data[10..12]);

        Ok(ParentLocatorEntry {
            key_offset,
            value_offset,
            key_length,
            value_length,
        })
    }
}

/// Parent Locator metadata item
#[derive(Debug, Clone)]
pub struct ParentLocator {
    pub key_count: u32,
    pub entries: Vec<ParentLocatorEntry>,
    pub key_values: Vec<(String, String)>, // Key-value pairs
}

impl ParentLocator {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(VhdxError::InvalidMetadata(
                "ParentLocator too small".to_string(),
            ));
        }

        let key_count = LittleEndian::read_u32(&data[4..8]);

        // Parse entries
        let mut entries = Vec::with_capacity(key_count as usize);
        let entries_start = 16;

        for i in 0..key_count as usize {
            let entry_offset = entries_start + i * ParentLocatorEntry::SIZE;
            if entry_offset + ParentLocatorEntry::SIZE > data.len() {
                return Err(VhdxError::InvalidMetadata(
                    "ParentLocator entry extends beyond data".to_string(),
                ));
            }
            let entry = ParentLocatorEntry::from_bytes(&data[entry_offset..])?;
            entries.push(entry);
        }

        // Parse key-value strings (UTF-16 LE)
        let mut key_values = Vec::with_capacity(key_count as usize);
        for entry in &entries {
            let key = read_utf16_string(data, entry.key_offset, entry.key_length)?;
            let value = read_utf16_string(data, entry.value_offset, entry.value_length)?;
            key_values.push((key, value));
        }

        Ok(ParentLocator {
            key_count,
            entries,
            key_values,
        })
    }

    /// Get value by key
    pub fn get(&self, key: &str) -> Option<&String> {
        self.key_values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Get parent path (try relative, then absolute, then URI)
    pub fn parent_path(&self) -> Option<&String> {
        self.get("relative_path")
            .or_else(|| self.get("absolute_win32_path"))
            .or_else(|| self.get("absolute_uri"))
    }

    /// Get parent linkage GUID
    pub fn parent_linkage(&self) -> Option<&String> {
        self.get("parent_linkage")
    }
}

/// Read UTF-16 LE string from bytes
fn read_utf16_string(data: &[u8], offset: u32, length: u16) -> Result<String> {
    let start = offset as usize;
    let len = length as usize * 2; // UTF-16 is 2 bytes per char

    if start + len > data.len() {
        return Err(VhdxError::InvalidMetadata(
            "String extends beyond data".to_string(),
        ));
    }

    let mut chars = Vec::with_capacity(length as usize);
    for i in (start..start + len).step_by(2) {
        if i + 1 < data.len() {
            let ch = LittleEndian::read_u16(&data[i..i + 2]);
            if ch == 0 {
                break;
            }
            chars.push(ch);
        }
    }

    String::from_utf16(&chars)
        .map_err(|_| VhdxError::InvalidMetadata("Invalid UTF-16 string".to_string()))
}

/// Complete Metadata Region
#[derive(Debug, Clone)]
pub struct MetadataRegion {
    pub header: MetadataTableHeader,
    pub entries: Vec<MetadataTableEntry>,
    pub data: Vec<u8>, // Raw metadata data after the table
}

impl MetadataRegion {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < MetadataTableHeader::SIZE {
            return Err(VhdxError::FileTooSmall);
        }

        let header = MetadataTableHeader::from_bytes(data)?;

        // Parse entries
        let mut entries = Vec::with_capacity(header.entry_count as usize);
        let entries_start = MetadataTableHeader::SIZE;

        for i in 0..header.entry_count as usize {
            let entry_offset = entries_start + i * MetadataTableEntry::SIZE;
            if entry_offset + MetadataTableEntry::SIZE > data.len() {
                return Err(VhdxError::InvalidMetadata(
                    "Metadata entry extends beyond data".to_string(),
                ));
            }
            let entry = MetadataTableEntry::from_bytes(&data[entry_offset..])?;
            entries.push(entry);
        }

        // The rest is metadata data
        let data_offset = entries_start + header.entry_count as usize * MetadataTableEntry::SIZE;
        let metadata_data = data[data_offset..].to_vec();

        Ok(MetadataRegion {
            header,
            entries,
            data: metadata_data,
        })
    }

    /// Get File Parameters
    pub fn file_parameters(&self) -> Result<FileParameters> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == FILE_PARAMETERS_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("FileParameters not found".to_string()))?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        FileParameters::from_bytes(data)
    }

    /// Get Virtual Disk Size
    pub fn virtual_disk_size(&self) -> Result<VirtualDiskSize> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == VIRTUAL_DISK_SIZE_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("VirtualDiskSize not found".to_string()))?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        VirtualDiskSize::from_bytes(data)
    }

    /// Get Logical Sector Size
    pub fn logical_sector_size(&self) -> Result<SectorSize> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == LOGICAL_SECTOR_SIZE_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("LogicalSectorSize not found".to_string()))?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        SectorSize::from_bytes(data)
    }

    /// Get Physical Sector Size
    pub fn physical_sector_size(&self) -> Result<SectorSize> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == PHYSICAL_SECTOR_SIZE_GUID)
            .ok_or_else(|| {
                VhdxError::InvalidMetadata("PhysicalSectorSize not found".to_string())
            })?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        SectorSize::from_bytes(data)
    }

    /// Get Virtual Disk ID
    pub fn virtual_disk_id(&self) -> Result<VirtualDiskId> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == VIRTUAL_DISK_ID_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("VirtualDiskId not found".to_string()))?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        VirtualDiskId::from_bytes(data)
    }

    /// Get Parent Locator (for differencing disks)
    pub fn parent_locator(&self) -> Result<ParentLocator> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == PARENT_LOCATOR_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("ParentLocator not found".to_string()))?;

        let offset = entry.offset as usize - MetadataTableHeader::SIZE;
        let data = &self.data[offset..offset + entry.length as usize];
        ParentLocator::from_bytes(data)
    }

    /// Check if this is a differencing disk
    pub fn is_differencing(&self) -> Result<bool> {
        match self.file_parameters() {
            Ok(params) => Ok(params.has_parent),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_parameters() {
        let mut data = vec![0u8; 16];
        LittleEndian::write_u32(&mut data[0..4], 1024 * 1024); // 1MB block size
        LittleEndian::write_u32(&mut data[4..8], 0); // No parent

        let params = FileParameters::from_bytes(&data).unwrap();
        assert_eq!(params.block_size, 1024 * 1024);
        assert!(!params.has_parent);
    }

    #[test]
    fn test_sector_size() {
        let mut data = vec![0u8; 4];
        LittleEndian::write_u32(&mut data[0..4], 512);

        let sector = SectorSize::from_bytes(&data).unwrap();
        assert_eq!(sector.size, 512);
    }

    #[test]
    fn test_virtual_disk_size() {
        let mut data = vec![0u8; 8];
        LittleEndian::write_u64(&mut data[0..8], 10 * 1024 * 1024 * 1024); // 10GB

        let size = VirtualDiskSize::from_bytes(&data).unwrap();
        assert_eq!(size.size, 10 * 1024 * 1024 * 1024);
    }
}
