use crate::common::constants::{
    DATA_DESCRIPTOR_SIGNATURE, DATA_SECTOR_SIZE, DESCRIPTOR_SIZE, LOG_ENTRY_HEADER_SIZE,
    LOG_ENTRY_SIGNATURE, ZERO_DESCRIPTOR_SIGNATURE,
};
use crate::error::{Error, Result};
use crate::types::Guid;

pub struct Log {
    raw_data: Vec<u8>,
}

impl Log {
    #[must_use]
    pub const fn new(data: Vec<u8>) -> Self {
        Self { raw_data: data }
    }

    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw_data
    }

    #[must_use]
    pub const fn entry(&self, _index: usize) -> Option<LogEntry<'_>> {
        let _ = self;
        None
    }

    #[must_use]
    pub fn entries(&self) -> Vec<LogEntry<'_>> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + LOG_ENTRY_HEADER_SIZE <= self.raw_data.len() {
            if let Ok(entry) = self.try_parse_entry_at(offset) {
                let entry_len = usize::try_from(entry.header().entry_length()).unwrap_or(0);
                if entry_len < LOG_ENTRY_HEADER_SIZE {
                    offset += DATA_SECTOR_SIZE;
                    continue;
                }
                entries.push(entry);
                offset += entry_len;
            } else {
                offset += DATA_SECTOR_SIZE;
            }
        }

        entries
    }

    fn try_parse_entry_at(&self, offset: usize) -> Result<LogEntry<'_>> {
        if offset + LOG_ENTRY_HEADER_SIZE > self.raw_data.len() {
            return Err(Error::LogEntryCorrupted("Not enough data".to_string()));
        }
        LogEntry::new(&self.raw_data[offset..])
    }

    #[must_use]
    pub fn is_replay_required(&self) -> bool {
        !self.entries().is_empty()
    }

    pub fn replay(&self, file: &mut std::fs::File) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        let entries = self.entries();
        if entries.is_empty() {
            return Ok(());
        }

        for entry in entries {
            let header = entry.header();

            if header.signature() != LOG_ENTRY_SIGNATURE {
                return Err(Error::LogEntryCorrupted(
                    "Invalid log entry signature".to_string(),
                ));
            }

            let descriptors = entry.descriptors();
            let data_sectors = entry.data();
            let mut data_sector_index = 0;

            for desc in descriptors {
                match desc {
                    Descriptor::Data(data_desc) => {
                        if data_sector_index < data_sectors.len() {
                            let sector = &data_sectors[data_sector_index];
                            let file_offset = data_desc.file_offset();

                            file.seek(SeekFrom::Start(file_offset))?;
                            let leading = data_desc.leading_bytes();
                            let trailing = data_desc.trailing_bytes();

                            if leading > 0 {
                                file.write_all(&vec![0u8; usize::try_from(leading).unwrap_or(0)])?;
                            }
                            file.write_all(sector.data())?;
                            if trailing > 0 {
                                file.write_all(&vec![0u8; usize::try_from(trailing).unwrap_or(0)])?;
                            }

                            data_sector_index += 1;
                        }
                    }
                    Descriptor::Zero(zero_desc) => {
                        let file_offset = zero_desc.file_offset();
                        let length = zero_desc.zero_length();

                        file.seek(SeekFrom::Start(file_offset))?;
                        file.write_all(&vec![0u8; usize::try_from(length).unwrap_or(0)])?;
                    }
                }
            }
        }

        Ok(())
    }
}

pub struct LogEntry<'a> {
    data: &'a [u8],
}

impl<'a> LogEntry<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < LOG_ENTRY_HEADER_SIZE {
            return Err(Error::LogEntryCorrupted("Entry too small".to_string()));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn header(&self) -> LogEntryHeader<'_> {
        LogEntryHeader::new(&self.data[0..LOG_ENTRY_HEADER_SIZE])
    }

    #[must_use]
    pub fn descriptor(&self, index: usize) -> Option<Descriptor<'_>> {
        let header = self.header();
        if index >= usize::try_from(header.descriptor_count()).unwrap_or(0) {
            return None;
        }

        let desc_offset = LOG_ENTRY_HEADER_SIZE + index * DESCRIPTOR_SIZE;
        if desc_offset + DESCRIPTOR_SIZE > self.data.len() {
            return None;
        }

        Descriptor::parse(&self.data[desc_offset..desc_offset + DESCRIPTOR_SIZE]).ok()
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<Descriptor<'_>> {
        let count = usize::try_from(self.header().descriptor_count()).unwrap_or(0);
        (0..count).filter_map(|i| self.descriptor(i)).collect()
    }

    #[must_use]
    pub fn data(&self) -> Vec<DataSector<'_>> {
        let header = self.header();
        let desc_count = usize::try_from(header.descriptor_count()).unwrap_or(0);
        let data_start = LOG_ENTRY_HEADER_SIZE + desc_count * DESCRIPTOR_SIZE;

        let data_sectors_needed: usize = self
            .descriptors()
            .iter()
            .filter_map(|d| match d {
                Descriptor::Data(_) => Some(1),
                Descriptor::Zero(_) => None,
            })
            .sum();

        let mut sectors = Vec::with_capacity(data_sectors_needed);
        for i in 0..data_sectors_needed {
            let offset = data_start + i * DATA_SECTOR_SIZE;
            if offset + DATA_SECTOR_SIZE > self.data.len() {
                break;
            }
            if let Ok(sector) = DataSector::new(&self.data[offset..offset + DATA_SECTOR_SIZE]) {
                sectors.push(sector);
            }
        }

        sectors
    }
}

pub struct LogEntryHeader<'a> {
    data: &'a [u8],
}

impl<'a> LogEntryHeader<'a> {
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

    #[must_use]
    pub fn entry_length(&self) -> u32 {
        u32::from_le_bytes(self.data[8..12].try_into().unwrap())
    }

    #[must_use]
    pub fn tail(&self) -> u32 {
        u32::from_le_bytes(self.data[12..16].try_into().unwrap())
    }

    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    #[must_use]
    pub fn descriptor_count(&self) -> u32 {
        u32::from_le_bytes(self.data[24..28].try_into().unwrap())
    }

    #[must_use]
    pub fn log_guid(&self) -> Guid {
        Guid::from_bytes(self.data[32..48].try_into().unwrap())
    }

    #[must_use]
    pub fn flushed_file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[48..56].try_into().unwrap())
    }

    #[must_use]
    pub fn last_file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[56..64].try_into().unwrap())
    }
}

#[derive(Debug)]
pub enum Descriptor<'a> {
    Data(DataDescriptor<'a>),
    Zero(ZeroDescriptor<'a>),
}

impl<'a> Descriptor<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted("Descriptor too small".to_string()));
        }

        let signature = &data[0..4];
        if signature == DATA_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Data(DataDescriptor::new(data)?))
        } else if signature == ZERO_DESCRIPTOR_SIGNATURE {
            Ok(Descriptor::Zero(ZeroDescriptor::new(data)?))
        } else {
            Err(Error::InvalidSignature {
                expected: "desc or zero".to_string(),
                found: String::from_utf8_lossy(signature).to_string(),
            })
        }
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        match self {
            Descriptor::Data(d) => d.raw(),
            Descriptor::Zero(z) => z.raw(),
        }
    }
}

#[derive(Debug)]
pub struct DataDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> DataDescriptor<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Data Descriptor too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn trailing_bytes(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    #[must_use]
    pub fn leading_bytes(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[24..32].try_into().unwrap())
    }
}

#[derive(Debug)]
pub struct ZeroDescriptor<'a> {
    data: &'a [u8],
}

impl<'a> ZeroDescriptor<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(Error::LogEntryCorrupted(
                "Zero Descriptor too small".to_string(),
            ));
        }
        Ok(Self { data })
    }

    #[must_use]
    pub const fn raw(&self) -> &[u8] {
        self.data
    }

    #[must_use]
    pub fn zero_length(&self) -> u64 {
        u64::from_le_bytes(self.data[8..16].try_into().unwrap())
    }

    #[must_use]
    pub fn file_offset(&self) -> u64 {
        u64::from_le_bytes(self.data[16..24].try_into().unwrap())
    }

    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        u64::from_le_bytes(self.data[24..32].try_into().unwrap())
    }
}

pub struct DataSector<'a> {
    data: &'a [u8],
}

impl<'a> DataSector<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() != DATA_SECTOR_SIZE {
            return Err(Error::InvalidFile(format!(
                "Data Sector must be {} bytes, got {}",
                DATA_SECTOR_SIZE,
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
    pub fn sequence_high(&self) -> u32 {
        u32::from_le_bytes(self.data[4..8].try_into().unwrap())
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data[8..4092]
    }

    #[must_use]
    pub fn sequence_low(&self) -> u32 {
        u32::from_le_bytes(self.data[4092..4096].try_into().unwrap())
    }

    #[must_use]
    pub fn sequence_number(&self) -> u64 {
        (u64::from(self.sequence_high()) << 32) | u64::from(self.sequence_low())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_header() {
        let mut data = [0u8; 64];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        data[4..8].copy_from_slice(&0x1234_5678_u32.to_le_bytes());
        data[8..12].copy_from_slice(&0x1000_u32.to_le_bytes());
        data[16..24].copy_from_slice(&0x1_u64.to_le_bytes());
        data[24..28].copy_from_slice(&2_u32.to_le_bytes());

        let header = LogEntryHeader::new(&data);
        assert_eq!(header.signature(), LOG_ENTRY_SIGNATURE);
        assert_eq!(header.checksum(), 0x1234_5678);
        assert_eq!(header.entry_length(), 0x1000);
        assert_eq!(header.sequence_number(), 1);
        assert_eq!(header.descriptor_count(), 2);
    }

    #[test]
    fn test_data_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        data[4..8].copy_from_slice(&0x100_u32.to_le_bytes());
        data[8..16].copy_from_slice(&0x200_u64.to_le_bytes());
        data[16..24].copy_from_slice(&0x0010_0000_u64.to_le_bytes());
        data[24..32].copy_from_slice(&0x1_u64.to_le_bytes());

        let desc = DataDescriptor::new(&data).unwrap();
        assert_eq!(desc.trailing_bytes(), 0x100);
        assert_eq!(desc.leading_bytes(), 0x200);
        assert_eq!(desc.file_offset(), 0x0010_0000);
        assert_eq!(desc.sequence_number(), 1);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = [0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        data[8..16].copy_from_slice(&0x1000_u64.to_le_bytes());
        data[16..24].copy_from_slice(&0x0020_0000_u64.to_le_bytes());

        let desc = ZeroDescriptor::new(&data).unwrap();
        assert_eq!(desc.zero_length(), 0x1000);
        assert_eq!(desc.file_offset(), 0x0020_0000);
    }
}
