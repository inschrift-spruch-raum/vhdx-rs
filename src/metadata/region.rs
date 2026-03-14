//! Metadata Region container
//!
//! The complete metadata region containing the table and all metadata items.

use crate::error::{Result, VhdxError};

use super::disk_id::{VirtualDiskId, VIRTUAL_DISK_ID_GUID};
use super::disk_size::{VirtualDiskSize, VIRTUAL_DISK_SIZE_GUID};
use super::file_parameters::{FileParameters, FILE_PARAMETERS_GUID};
use super::parent_locator::{ParentLocator, PARENT_LOCATOR_GUID};
use super::sector_size::{SectorSize, LOGICAL_SECTOR_SIZE_GUID, PHYSICAL_SECTOR_SIZE_GUID};
use super::table::{MetadataTableEntry, MetadataTableHeader};

/// Whitelist of recognized required metadata GUIDs per MS-VHDX specification
/// These are the only metadata items that can have is_required=true
const KNOWN_REQUIRED_METADATA_GUIDS: [crate::common::guid::Guid; 6] = [
    FILE_PARAMETERS_GUID,      // CAA16737-FA36-4D43-B3B6-33F0AA44E76B
    VIRTUAL_DISK_SIZE_GUID,    // 2FA54224-CD1B-4876-B211-5DBED83BF4B8
    VIRTUAL_DISK_ID_GUID,      // BECA4B1E-C294-4701-8F99-C63D33312C71
    LOGICAL_SECTOR_SIZE_GUID,  // 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
    PHYSICAL_SECTOR_SIZE_GUID, // CDA348C7-889D-4916-90F7-89D5DA63A0C5
    PARENT_LOCATOR_GUID,       // A558951E-B615-4723-A4B7-6A1A4B2B5A6A
];

/// Complete Metadata Region
#[derive(Debug, Clone)]
pub struct MetadataRegion {
    pub header: MetadataTableHeader,
    pub entries: Vec<MetadataTableEntry>,
    pub data: Vec<u8>,  // Raw metadata data after the table
    data_offset: usize, // Offset from region start to data (header + entries)
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

        // Validate that all required metadata items are recognized (MS-VHDX spec Section 2.2)
        for entry in &entries {
            if entry.is_required && !KNOWN_REQUIRED_METADATA_GUIDS.contains(&entry.item_id) {
                return Err(VhdxError::UnknownRequiredMetadata {
                    guid: entry.item_id.to_string(),
                });
            }
        }

        Ok(MetadataRegion {
            header,
            entries,
            data: metadata_data,
            data_offset,
        })
    }

    /// Get File Parameters
    pub fn file_parameters(&self) -> Result<FileParameters> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.item_id == FILE_PARAMETERS_GUID)
            .ok_or_else(|| VhdxError::InvalidMetadata("FileParameters not found".to_string()))?;

        let offset = entry.offset as usize - self.data_offset;
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

        let offset = entry.offset as usize - self.data_offset;
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

        let offset = entry.offset as usize - self.data_offset;
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

        let offset = entry.offset as usize - self.data_offset;
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

        let offset = entry.offset as usize - self.data_offset;
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

        let offset = entry.offset as usize - self.data_offset;
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
    use byteorder::{ByteOrder, LittleEndian};

    fn create_region_bytes(entries: &[([u8; 16], u32, bool)]) -> Vec<u8> {
        let mut data =
            vec![0u8; MetadataTableHeader::SIZE + entries.len() * MetadataTableEntry::SIZE];

        // Write header signature
        data[0..8].copy_from_slice(b"metadata");
        // Write entry count
        LittleEndian::write_u16(&mut data[10..12], entries.len() as u16);

        // Write entries
        for (i, (guid, offset, is_required)) in entries.iter().enumerate() {
            let entry_offset = MetadataTableHeader::SIZE + i * MetadataTableEntry::SIZE;
            // item_id (16 bytes)
            data[entry_offset..entry_offset + 16].copy_from_slice(guid);
            // offset (4 bytes)
            LittleEndian::write_u32(&mut data[entry_offset + 16..entry_offset + 20], *offset);
            // length (4 bytes)
            LittleEndian::write_u32(&mut data[entry_offset + 20..entry_offset + 24], 16);
            // flags (4 bytes) - bit 2 is is_required
            let flags = if *is_required { 0x04 } else { 0x00 };
            LittleEndian::write_u32(&mut data[entry_offset + 24..entry_offset + 28], flags);
        }

        data
    }

    #[test]
    fn test_known_required_metadata_passes() {
        // Test with File Parameters GUID (known required metadata)
        let file_params_guid: [u8; 16] = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let data = create_region_bytes(&[(file_params_guid, 0x1000, true)]);

        let result = MetadataRegion::from_bytes(&data);
        assert!(
            result.is_ok(),
            "Known required metadata should pass validation"
        );
    }

    #[test]
    fn test_unknown_required_metadata_rejected() {
        // Test with unknown GUID marked as required
        let unknown_guid: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let data = create_region_bytes(&[(unknown_guid, 0x1000, true)]);

        let result = MetadataRegion::from_bytes(&data);
        assert!(
            result.is_err(),
            "Unknown required metadata should be rejected"
        );
        match result.unwrap_err() {
            VhdxError::UnknownRequiredMetadata { guid } => {
                assert!(!guid.is_empty(), "Error should contain the GUID");
            }
            e => panic!("Expected UnknownRequiredMetadata error, got {:?}", e),
        }
    }

    #[test]
    fn test_unknown_non_required_metadata_allowed() {
        // Test with unknown GUID marked as NOT required (should be allowed/ignored per spec)
        let unknown_guid: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let data = create_region_bytes(&[(unknown_guid, 0x1000, false)]);

        let result = MetadataRegion::from_bytes(&data);
        assert!(
            result.is_ok(),
            "Unknown non-required metadata should be allowed (ignored per spec)"
        );
    }

    #[test]
    fn test_mixed_metadata_entries() {
        // Test with mix of known required, unknown non-required, and known optional
        let file_params_guid: [u8; 16] = [
            0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
            0xE7, 0x6B,
        ];
        let disk_size_guid: [u8; 16] = [
            0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B,
            0xF4, 0xB8,
        ];
        let unknown_guid: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];

        let data = create_region_bytes(&[
            (file_params_guid, 0x1000, true), // Known required
            (unknown_guid, 0x2000, false),    // Unknown non-required (allowed)
            (disk_size_guid, 0x3000, false),  // Known but not marked required
        ]);

        let result = MetadataRegion::from_bytes(&data);
        assert!(result.is_ok(), "Mixed valid entries should pass");
    }

    #[test]
    fn test_all_six_known_guids_pass() {
        // Test all 6 known required metadata GUIDs (using actual constant byte values)
        let guids: [[u8; 16]; 6] = [
            [
                0x37, 0x67, 0xA1, 0xCA, 0x36, 0xFA, 0x43, 0x4D, 0xB3, 0xB6, 0x33, 0xF0, 0xAA, 0x44,
                0xE7, 0x6B,
            ], // File Parameters: CAA16737-FA36-4D43-B3B6-33F0AA44E76B
            [
                0x24, 0x42, 0xA5, 0x2F, 0x1B, 0xCD, 0x76, 0x48, 0xB2, 0x11, 0x5D, 0xBE, 0xD8, 0x3B,
                0xF4, 0xB8,
            ], // Virtual Disk Size: 2FA54224-CD1B-4876-B211-5DBED83BF4B8
            [
                0xAB, 0x12, 0xCA, 0xBE, 0xE6, 0xB2, 0x23, 0x45, 0x93, 0xEF, 0xC3, 0x09, 0xE0, 0x00,
                0xC7, 0x46,
            ], // Virtual Disk ID: BECA12AB-B2E6-4523-93EF-C309E000C746
            [
                0x1D, 0xBF, 0x41, 0x81, 0x6F, 0xA9, 0x09, 0x47, 0xBA, 0x47, 0xF2, 0x33, 0xA8, 0xFA,
                0xAB, 0x5F,
            ], // Logical Sector Size: 8141BF1D-A96F-4709-BA47-F233A8FAAB5F
            [
                0xC7, 0x48, 0xA3, 0xCD, 0x5D, 0x44, 0x71, 0x44, 0x9C, 0xC9, 0xE9, 0x88, 0x52, 0x51,
                0xC5, 0x56,
            ], // Physical Sector Size: CDA348C7-445D-4471-9CC9-E9885251C556
            [
                0x2D, 0x5F, 0xD3, 0xA8, 0x0B, 0xB3, 0x4D, 0x45, 0xAB, 0xF7, 0xD3, 0xD8, 0x48, 0x34,
                0xAB, 0x0C,
            ], // Parent Locator: A8D35F2D-B30B-454D-ABF7-D3D84834AB0C
        ];

        let entries: Vec<([u8; 16], u32, bool)> = guids
            .iter()
            .enumerate()
            .map(|(i, guid)| (*guid, 0x1000 + (i as u32 * 0x100), true))
            .collect();

        let data = create_region_bytes(&entries);
        let result = MetadataRegion::from_bytes(&data);
        assert!(
            result.is_ok(),
            "All 6 known required metadata GUIDs should pass"
        );
    }
}
