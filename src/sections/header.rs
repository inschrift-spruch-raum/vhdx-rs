use crate::common::constants::{
    FILE_TYPE_SIGNATURE, FILE_TYPE_SIZE, HEADER_1_OFFSET, HEADER_2_OFFSET, HEADER_SECTION_SIZE,
    HEADER_SIGNATURE, HEADER_SIZE, LOG_VERSION, REGION_TABLE_1_OFFSET, REGION_TABLE_2_OFFSET,
    REGION_TABLE_SIZE, VHDX_VERSION,
};
use crate::error::{Error, Result};
use crate::sections::crc32c_with_zero_field;
use crate::types::Guid;

pub struct Header {
    raw_data: Vec<u8>,
}

impl Header {
    pub fn new(data: Vec<u8>) -> Result<Self> {
        if data.len() != HEADER_SECTION_SIZE {
            return Err(Error::InvalidFile(format!(
                "Header section must be {} bytes, got {}",
                HEADER_SECTION_SIZE,
                data.len()
            )));
        }
        Ok(Self { raw_data: data })
    }

    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    #[must_use]
    pub fn file_type(&self) -> FileTypeIdentifier<'_> {
        FileTypeIdentifier::new(&self.raw_data[0..FILE_TYPE_SIZE])
    }

    #[must_use]
    pub fn header(&self, index: usize) -> Option<HeaderStructure<'_>> {
        match index {
            0 => {
                let h1 = HeaderStructure::new(
                    &self.raw_data[HEADER_1_OFFSET..HEADER_1_OFFSET + HEADER_SIZE],
                )
                .ok()?;
                let h2 = HeaderStructure::new(
                    &self.raw_data[HEADER_2_OFFSET..HEADER_2_OFFSET + HEADER_SIZE],
                )
                .ok()?;

                if h1.sequence_number() > h2.sequence_number() {
                    Some(h1)
                } else {
                    Some(h2)
                }
            }
            1 => {
                HeaderStructure::new(&self.raw_data[HEADER_1_OFFSET..HEADER_1_OFFSET + HEADER_SIZE])
                    .ok()
            }
            2 => {
                HeaderStructure::new(&self.raw_data[HEADER_2_OFFSET..HEADER_2_OFFSET + HEADER_SIZE])
                    .ok()
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn region_table(&self, index: usize) -> Option<RegionTable<'_>> {
        let offset = match index {
            0 | 1 => REGION_TABLE_1_OFFSET,
            2 => REGION_TABLE_2_OFFSET,
            _ => return None,
        };
        RegionTable::new(&self.raw_data[offset..offset + REGION_TABLE_SIZE]).ok()
    }
}

pub struct FileTypeIdentifier<'a> {
    data: &'a [u8],
}

impl<'a> FileTypeIdentifier<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.data[0..8]
    }

    #[must_use]
    pub fn creator(&self) -> String {
        let creator_bytes = &self.data[8..8 + 512.min(self.data.len().saturating_sub(8))];
        let utf16: Vec<u16> = creator_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&utf16)
    }

    #[must_use]
    pub fn create(creator: Option<&str>) -> Vec<u8> {
        let mut data = vec![0u8; FILE_TYPE_SIZE];
        data[0..8].copy_from_slice(FILE_TYPE_SIGNATURE);

        if let Some(creator) = creator {
            let utf16: Vec<u16> = creator.encode_utf16().collect();
            for (i, &c) in utf16.iter().enumerate() {
                if 8 + i * 2 + 2 > data.len() {
                    break;
                }
                data[8 + i * 2..8 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
            }
        }

        data
    }
}

pub struct HeaderStructure<'a> {
    data: &'a [u8],
}

impl<'a> HeaderStructure<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != HEADER_SIZE {
            return Err(Error::CorruptedHeader(format!(
                "Header must be {} bytes, got {}",
                HEADER_SIZE,
                data.len()
            )));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.data[0..4]
    }

    #[must_use]
    pub fn checksum(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    pub fn verify_checksum(&self) -> Result<()> {
        let expected = self.checksum();
        let actual = crc32c_with_zero_field(self.data, 4, 4);
        if expected != actual {
            return Err(Error::InvalidChecksum { expected, actual });
        }
        Ok(())
    }

    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    #[must_use]
    pub fn file_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[16..32].try_into().unwrap())
    }

    #[must_use]
    pub fn data_write_guid(&self) -> Guid {
        Guid::from_bytes(self.data[32..48].try_into().unwrap())
    }

    #[must_use]
    pub fn log_guid(&self) -> Guid {
        Guid::from_bytes(self.data[48..64].try_into().unwrap())
    }

    #[must_use]
    pub fn log_version(&self) -> u16 {
        u16::from_le_bytes(self.data[64..66].try_into().unwrap())
    }

    #[must_use]
    pub fn version(&self) -> u16 {
        u16::from_le_bytes(self.data[66..68].try_into().unwrap())
    }

    #[must_use]
    pub fn log_length(&self) -> u32 {
        u32::from_le_bytes(self.data[68..72].try_into().unwrap())
    }

    #[must_use]
    pub fn log_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[72..80].try_into().unwrap())
    }

    #[must_use]
    pub fn create(
        sequence_number: u64, file_write_guid: Guid, data_write_guid: Guid, log_guid: Guid,
        log_length: u32, log_offset: u64,
    ) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];

        data[0..4].copy_from_slice(HEADER_SIGNATURE);
        data[4..8].copy_from_slice(&[0; 4]);
        data[8..16].copy_from_slice(&sequence_number.to_le_bytes());
        data[16..32].copy_from_slice(file_write_guid.as_bytes());
        data[32..48].copy_from_slice(data_write_guid.as_bytes());
        data[48..64].copy_from_slice(log_guid.as_bytes());
        data[64..66].copy_from_slice(&LOG_VERSION.to_le_bytes());
        data[66..68].copy_from_slice(&VHDX_VERSION.to_le_bytes());
        data[68..72].copy_from_slice(&log_length.to_le_bytes());
        data[72..80].copy_from_slice(&log_offset.to_le_bytes());

        let checksum = crc32c::crc32c(&data);
        data[4..8].copy_from_slice(&checksum.to_le_bytes());

        data
    }
}

pub struct RegionTable<'a> {
    data: &'a [u8],
}

impl<'a> RegionTable<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != REGION_TABLE_SIZE {
            return Err(Error::InvalidRegionTable(format!(
                "Region Table must be {} bytes, got {}",
                REGION_TABLE_SIZE,
                data.len()
            )));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn header(&self) -> RegionTableHeader<'_> {
        RegionTableHeader::new(&self.data[0..16])
    }

    #[must_use]
    pub fn entry(&self, index: u32) -> Option<RegionTableEntry<'_>> {
        let header = self.header();
        if index >= header.entry_count() {
            return None;
        }
        let offset = 16 + index as usize * 32;
        if offset + 32 > self.data.len() {
            return None;
        }
        RegionTableEntry::new(&self.data[offset..offset + 32]).ok()
    }

    #[must_use]
    pub fn entries(&self) -> Vec<RegionTableEntry<'_>> {
        let count = self.header().entry_count();
        (0..count).filter_map(|i| self.entry(i)).collect()
    }

    #[must_use]
    pub fn find_entry(&self, guid: &Guid) -> Option<RegionTableEntry<'_>> {
        self.entries().into_iter().find(|e| e.guid() == *guid)
    }
}

pub struct RegionTableHeader<'a> {
    data: &'a [u8],
}

impl<'a> RegionTableHeader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn signature(&self) -> &[u8] {
        &self.data[0..4]
    }

    #[must_use]
    pub fn checksum(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    pub fn verify_checksum(&self) -> Result<()> {
        let expected = self.checksum();
        let actual = crc32c_with_zero_field(self.data, 4, 4);
        if expected != actual {
            return Err(Error::InvalidChecksum { expected, actual });
        }
        Ok(())
    }

    #[must_use]
    pub fn entry_count(&self) -> u32 {
        u32::from_le_bytes(self.data[8..12].try_into().unwrap())
    }
}

pub struct RegionTableEntry<'a> {
    data: &'a [u8],
}

impl<'a> RegionTableEntry<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != 32 {
            return Err(Error::InvalidRegionTable(
                "Entry must be 32 bytes".to_string(),
            ));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn guid(&self) -> Guid {
        Guid::from_bytes(self.data[0..16].try_into().unwrap())
    }

    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    #[must_use]
    pub fn length(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    #[must_use]
    pub fn required(&self) -> bool {
        u32::from_le_bytes(self.data[28..32].try_into().unwrap()) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_identifier() {
        let data = FileTypeIdentifier::create(Some("TestCreator"));
        let ft = FileTypeIdentifier::new(&data);
        assert_eq!(ft.signature(), FILE_TYPE_SIGNATURE);
        assert_eq!(ft.creator(), "TestCreator");
    }

    #[test]
    fn test_header_structure() {
        let guid = Guid::nil();
        let data = HeaderStructure::create(1, guid, guid, guid, 0, 0);
        let header = HeaderStructure::new(&data).unwrap();
        assert_eq!(header.sequence_number(), 1);
        assert_eq!(header.version(), 1);
        assert_eq!(header.log_version(), 0);
    }

    #[test]
    fn test_region_table_entry() {
        let mut data = [0u8; 32];
        let guid_bytes = [
            0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD,
            0x4A, 0x08,
        ];
        data[0..16].copy_from_slice(&guid_bytes);
        data[16..24].copy_from_slice(&0x0010_0000_u64.to_le_bytes());
        data[24..28].copy_from_slice(&0x0010_0000_u32.to_le_bytes());
        data[28..32].copy_from_slice(&1u32.to_le_bytes());

        let entry = RegionTableEntry::new(&data).unwrap();
        assert_eq!(entry.file_offset(), 0x0010_0000);
        assert_eq!(entry.length(), 0x0010_0000);
        assert!(entry.required());
    }
}
