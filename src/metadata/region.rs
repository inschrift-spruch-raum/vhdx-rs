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
