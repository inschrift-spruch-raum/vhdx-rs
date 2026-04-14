use crate::File;
use crate::PayloadBlockState;
use crate::error::{Error, Result};

pub struct IO<'a> {
    file: &'a File,
}

impl<'a> IO<'a> {
    pub const fn new(file: &'a File) -> Self {
        Self { file }
    }

    #[must_use]
    pub fn sector(&self, sector: u64) -> Option<Sector<'a>> {
        let sector_size = u64::from(self.file.logical_sector_size());
        let block_size = u64::from(self.file.block_size());

        let sectors_per_block = block_size / sector_size;
        let block_idx = sector / sectors_per_block;
        let block_sector_idx = u32::try_from(sector % sectors_per_block)
            .expect("sector index within block should fit in u32");

        let total_sectors = self.file.virtual_disk_size() / sector_size;
        if sector >= total_sectors {
            return None;
        }

        Some(Sector {
            file: self.file,
            block_idx,
            block_sector_idx,
            size: self.file.logical_sector_size(),
        })
    }

    pub fn read_sectors(&self, start_sector: u64, buf: &mut [u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let num_sectors = buf.len() / sector_size;

        if !buf.len().is_multiple_of(sector_size) {
            return Err(Error::InvalidParameter(
                "Buffer size must be a multiple of sector size".to_string(),
            ));
        }

        let mut total_read = 0;
        for i in 0..num_sectors {
            let sector_num = start_sector + i as u64;
            if let Some(sector) = self.sector(sector_num) {
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                let bytes_read = sector.read(sector_buf)?;
                total_read += bytes_read;
            } else {
                let sector_buf = &mut buf[i * sector_size..(i + 1) * sector_size];
                for item in sector_buf.iter_mut() {
                    *item = 0;
                }
                total_read += sector_size;
            }
        }

        Ok(total_read)
    }

    pub fn write_sectors(&self, _start_sector: u64, data: &[u8]) -> Result<usize> {
        let sector_size = self.file.logical_sector_size() as usize;
        let _num_sectors = data.len() / sector_size;

        if !data.len().is_multiple_of(sector_size) {
            return Err(Error::InvalidParameter(
                "Data size must be a multiple of sector size".to_string(),
            ));
        }

        Err(Error::InvalidParameter(
            "IO::write_sectors requires mutable access (not yet fully implemented)".to_string(),
        ))
    }
}

pub struct Sector<'a> {
    file: &'a File,
    block_idx: u64,
    block_sector_idx: u32,
    size: u32,
}

impl Sector<'_> {
    #[must_use]
    pub const fn block_idx(&self) -> u64 {
        self.block_idx
    }

    #[must_use]
    pub const fn block_sector_idx(&self) -> u32 {
        self.block_sector_idx
    }

    #[must_use]
    pub fn global_sector(&self) -> u64 {
        let sectors_per_block = u64::from(self.file.block_size() / self.size);
        self.block_idx * sectors_per_block + u64::from(self.block_sector_idx)
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        if buf.len() != self.size as usize {
            return Err(Error::InvalidParameter(format!(
                "Buffer size {} does not match sector size {}",
                buf.len(),
                self.size
            )));
        }

        let sector_offset = self.global_sector() * u64::from(self.size);
        self.file.read(sector_offset, buf)
    }

    #[must_use]
    pub const fn payload(&self) -> PayloadBlock<'_> {
        PayloadBlock {
            file: self.file,
            block_idx: self.block_idx,
        }
    }
}

pub struct PayloadBlock<'a> {
    file: &'a File,
    block_idx: u64,
}

impl PayloadBlock<'_> {
    #[must_use]
    pub const fn block_idx(&self) -> u64 {
        self.block_idx
    }

    pub fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let block_size = u64::from(self.file.block_size());
        if offset >= block_size {
            return Ok(0);
        }

        let block_offset = self.block_idx * block_size + offset;
        self.file.read(block_offset, buf)
    }

    #[must_use]
    pub fn bat_entry(&self) -> Option<crate::BatEntry> {
        if let Ok(bat) = self.file.sections().bat() {
            usize::try_from(self.block_idx)
                .ok()
                .and_then(|idx| bat.entry(idx))
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_allocated(&self) -> bool {
        if let Some(entry) = self.bat_entry() {
            matches!(
                entry.state,
                crate::BatState::Payload(PayloadBlockState::FullyPresent)
            )
        } else {
            false
        }
    }
}
