use bitvec::prelude::*;
use std::io::{Read, Seek, Write};

use crate::bat::{Bat, BatState, PayloadBlockState, SectorBitmapState};
use crate::constants::MIB;
use crate::error::{Error, Result};
use crate::medium::{Len, SetLen, SyncData, read_exact_at, write_all_at};

use super::core::Sector;

impl<T> Sector<'_, '_, T>
where
    T: Read + Write + Seek + Len + SetLen + SyncData,
{
    /// Write data to this sector range at the given byte offset.
    ///
    /// `byte_offset` is relative to the first sector in this range (0-based).
    /// The resulting byte range `[byte_offset, byte_offset + data.len())` must
    /// fit within `sector_count * logical_sector_size`. Dynamic sparse payload
    /// blocks are allocated as needed.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    pub(super) fn write_at(&mut self, data: &[u8], byte_offset: u64) -> Result<()> {
        if !self.io().file.is_write() {
            return Err(Error::ReadOnly);
        }

        let lss = self.logical_sector_size as usize;
        let range_bytes = self.count * lss as u64;

        // Empty write is a no-op
        if data.is_empty() {
            return Ok(());
        }

        // Validate byte range
        let byte_end = byte_offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| Error::InvalidParameter("byte_offset + data.len() overflow".into()))?;
        if byte_end > range_bytes {
            return Err(Error::InvalidParameter(format!(
                "byte range [{byte_offset}, {byte_end}) exceeds sector range of {range_bytes} bytes"
            )));
        }

        let start_byte = usize::try_from(byte_offset)
            .map_err(|_| Error::InvalidParameter("byte_offset does not fit usize".into()))?;
        let end_byte = start_byte + data.len();

        let first_sector_rel = start_byte / lss;
        let first_skip = start_byte % lss;
        let aligned_end = end_byte.is_multiple_of(lss);

        // Fast path: sector-aligned start AND sector-aligned end
        if first_skip == 0 && aligned_end {
            let sectors_to_write = data.len() / lss;
            return self.write_full_sectors(
                data,
                self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
                u64::try_from(sectors_to_write).expect("sector count fits u64"),
            );
        }

        // Slow path: read-modify-write
        let last_sector_rel = (end_byte - 1) / lss;
        let affected_count = last_sector_rel - first_sector_rel + 1;

        // 1. Read current data for affected sectors
        let mut temp = vec![0u8; affected_count * lss];
        self.read_full_sectors(
            &mut temp,
            self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
            u64::try_from(affected_count).expect("sector count fits u64"),
        )?;

        // 2. Patch the byte range
        temp[first_skip..first_skip + data.len()].copy_from_slice(data);

        // 3. Write back all affected sectors
        self.write_full_sectors(
            &temp,
            self.start + u64::try_from(first_sector_rel).expect("sector index fits u64"),
            u64::try_from(affected_count).expect("sector count fits u64"),
        )?;

        Ok(())
    }

    /// Write `sector_count` full sectors starting at absolute `start_sector`.
    /// `data.len()` must equal `sector_count * logical_sector_size`.
    ///
    /// # Panics
    ///
    /// Panics if arithmetic overflow occurs during sector/offset conversion.
    /// This should not happen with well-formed VHDX files.
    fn write_full_sectors(
        &mut self, data: &[u8], start_sector: u64, sector_count: u64,
    ) -> Result<()> {
        let lss = self.logical_sector_size as usize;
        let spb = self.sectors_per_block();
        let mut buf_offset = 0usize;
        let mut current_sector = start_sector;
        let mut remaining = sector_count;

        while remaining > 0 {
            let block_idx = current_sector / spb;
            let sector_in_block = current_sector % spb;
            let remaining_in_block = spb - sector_in_block;
            let sectors_this_round = remaining.min(remaining_in_block);
            let bytes_this_round =
                usize::try_from(sectors_this_round).expect("sector count fits usize") * lss;

            let entry = self.resolve_bat_entry_for_block(block_idx)?;
            let state = entry.state;

            match state {
                BatState::Payload(payload_state) => match payload_state {
                    PayloadBlockState::FullyPresent => {
                        let file_offset =
                            entry.file_offset_mb() * u64::from(MIB) + sector_in_block * lss as u64;
                        write_all_at(
                            self.io_mut().file.inner_mut(),
                            file_offset,
                            &data[buf_offset..buf_offset + bytes_this_round],
                        )?;
                    }
                    PayloadBlockState::PartiallyPresent => {
                        self.require_sector_bitmap_present(block_idx)?;
                        let file_offset =
                            entry.file_offset_mb() * u64::from(MIB) + sector_in_block * lss as u64;
                        write_all_at(
                            self.io_mut().file.inner_mut(),
                            file_offset,
                            &data[buf_offset..buf_offset + bytes_this_round],
                        )?;
                        self.io_mut().file.inner_mut().sync_data()?;
                        self.mark_child_sectors_present(
                            block_idx,
                            sector_in_block,
                            sectors_this_round,
                        )?;
                        self.io_mut().file.inner_mut().sync_data()?;
                    }
                    PayloadBlockState::NotPresent if self.io().has_parent => {
                        let file_offset_mb = self.reserve_payload_block()?;
                        let file_offset =
                            file_offset_mb * u64::from(MIB) + sector_in_block * lss as u64;
                        write_all_at(
                            self.io_mut().file.inner_mut(),
                            file_offset,
                            &data[buf_offset..buf_offset + bytes_this_round],
                        )?;
                        self.io_mut().file.inner_mut().sync_data()?;
                        self.ensure_sector_bitmap_present(block_idx)?;
                        self.mark_child_sectors_present(
                            block_idx,
                            sector_in_block,
                            sectors_this_round,
                        )?;
                        self.io_mut().file.inner_mut().sync_data()?;
                        self.publish_payload_block(
                            block_idx,
                            file_offset_mb,
                            PayloadBlockState::PartiallyPresent,
                        )?;
                    }
                    PayloadBlockState::NotPresent
                    | PayloadBlockState::Zero
                    | PayloadBlockState::Unmapped
                    | PayloadBlockState::Undefined => {
                        let file_offset_mb = self.reserve_payload_block()?;
                        let file_offset =
                            file_offset_mb * u64::from(MIB) + sector_in_block * lss as u64;
                        write_all_at(
                            self.io_mut().file.inner_mut(),
                            file_offset,
                            &data[buf_offset..buf_offset + bytes_this_round],
                        )?;
                        self.io_mut().file.inner_mut().sync_data()?;
                        self.publish_payload_block(
                            block_idx,
                            file_offset_mb,
                            PayloadBlockState::FullyPresent,
                        )?;
                    }
                },
                BatState::SectorBitmap(_) => {
                    return Err(Error::BlockNotPresent {
                        block_idx,
                        state: "sector bitmap entry (expected payload)".into(),
                    });
                }
            }

            buf_offset += bytes_this_round;
            current_sector += sectors_this_round;
            remaining -= sectors_this_round;
        }

        Ok(())
    }

    fn reserve_payload_block(&mut self) -> Result<u64> {
        self.reserve_block(u64::from(self.block_size))
    }

    fn reserve_sector_bitmap_block(&mut self) -> Result<u64> {
        self.reserve_block(u64::from(MIB))
    }

    fn reserve_block(&mut self, block_size: u64) -> Result<u64> {
        let file_len = self.io_mut().file.inner_mut().len()?;
        let payload_offset = file_len.div_ceil(block_size) * block_size;
        let payload_end = payload_offset
            .checked_add(block_size)
            .ok_or_else(|| Error::InvalidParameter("payload block end overflow".into()))?;
        self.io_mut().file.inner_mut().set_len(payload_end)?;

        Ok(payload_offset / u64::from(MIB))
    }

    fn publish_payload_block(
        &mut self, block_idx: u64, file_offset_mb: u64, state: PayloadBlockState,
    ) -> Result<()> {
        let mut raw_entry = [0u8; 8];
        let bits = raw_entry.view_bits_mut::<Lsb0>();
        bits[0..3].store::<u8>(state as u8);
        bits[20..64].store::<u64>(file_offset_mb);

        let bat_array_idx = block_idx + block_idx / self.chunk_ratio;
        self.io_mut().file.write_bat_entry(bat_array_idx, raw_entry)
    }

    fn ensure_sector_bitmap_present(&mut self, block_idx: u64) -> Result<u64> {
        let sb_bat_idx = self.sector_bitmap_bat_index(block_idx);
        let bat_buf = self.io_mut().file.bat_buf()?;
        let bat = Bat::new(&bat_buf, self.chunk_ratio);
        let sb_entry = bat.entry(sb_bat_idx)?;

        match sb_entry.sector_bitmap_state() {
            Some(SectorBitmapState::Present) => Ok(sb_entry.file_offset_mb()),
            Some(SectorBitmapState::NotPresent) => {
                let file_offset_mb = self.reserve_sector_bitmap_block()?;
                let zero_bitmap = vec![0u8; MIB as usize];
                write_all_at(
                    self.io_mut().file.inner_mut(),
                    file_offset_mb * u64::from(MIB),
                    &zero_bitmap,
                )?;
                self.io_mut().file.inner_mut().sync_data()?;
                self.publish_sector_bitmap_block(sb_bat_idx, file_offset_mb)?;
                Ok(file_offset_mb)
            }
            None => Err(Error::InvalidSectorBitmapState(sb_entry.raw_state())),
        }
    }

    fn publish_sector_bitmap_block(
        &mut self, bat_array_idx: u64, file_offset_mb: u64,
    ) -> Result<()> {
        let mut raw_entry = [0u8; 8];
        let bits = raw_entry.view_bits_mut::<Lsb0>();
        bits[0..3].store::<u8>(SectorBitmapState::Present as u8);
        bits[20..64].store::<u64>(file_offset_mb);

        self.io_mut().file.write_bat_entry(bat_array_idx, raw_entry)
    }

    fn mark_child_sectors_present(
        &mut self, block_idx: u64, sector_in_block: u64, sector_count: u64,
    ) -> Result<()> {
        let sb_bat_idx = self.sector_bitmap_bat_index(block_idx);
        let bat_buf = self.io_mut().file.bat_buf()?;
        let bat = Bat::new(&bat_buf, self.chunk_ratio);
        let sb_entry = bat.entry(sb_bat_idx)?;
        let sb_state = sb_entry
            .sector_bitmap_state()
            .ok_or(Error::InvalidSectorBitmapState(sb_entry.raw_state()))?;
        if sb_state != SectorBitmapState::Present {
            return Err(Error::StateMismatch {
                state: sb_entry.raw_state(),
                description: "sector bitmap not Present for differencing write".into(),
            });
        }

        let bitmap_offset = sb_entry.file_offset_mb() * u64::from(MIB);
        let mut bitmap = vec![0u8; MIB as usize];
        read_exact_at(self.io_mut().file.inner_mut(), bitmap_offset, &mut bitmap)?;

        let spb = self.sectors_per_block();
        let block_in_chunk = block_idx % self.chunk_ratio;
        let bits = bitmap.view_bits_mut::<Lsb0>();
        for i in 0..sector_count {
            let sector_in_chunk = block_in_chunk * spb + sector_in_block + i;
            let bit_idx = usize::try_from(sector_in_chunk).expect("bitmap bit index fits usize");
            if bit_idx >= bits.len() {
                return Err(Error::InvalidMetadata(format!(
                    "sector bitmap index out of range: bit {bit_idx}"
                )));
            }
            bits.set(bit_idx, true);
        }

        write_all_at(self.io_mut().file.inner_mut(), bitmap_offset, &bitmap)?;
        Ok(())
    }

    fn require_sector_bitmap_present(&mut self, block_idx: u64) -> Result<()> {
        let sb_bat_idx = self.sector_bitmap_bat_index(block_idx);
        let bat_buf = self.io_mut().file.bat_buf()?;
        let bat = Bat::new(&bat_buf, self.chunk_ratio);
        let sb_entry = bat.entry(sb_bat_idx)?;
        let sb_state = sb_entry
            .sector_bitmap_state()
            .ok_or(Error::InvalidSectorBitmapState(sb_entry.raw_state()))?;
        if sb_state == SectorBitmapState::Present {
            Ok(())
        } else {
            Err(Error::StateMismatch {
                state: sb_entry.raw_state(),
                description: "sector bitmap not Present for PartiallyPresent payload".into(),
            })
        }
    }
}
