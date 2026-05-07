//! BAT (Block Allocation Table) section parser.
//!
//! The BAT is an array of 64-bit entries mapping virtual disk blocks to file offsets.
//! Payload block entries and sector bitmap block entries are interleaved based on the
//! chunk ratio: every `chunk_ratio` payload entries is followed by 1 sector bitmap entry.

use bitvec::prelude::*;

use crate::error::{Error, Result};

/// Payload block state (MS-VHDX §2.5.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadBlockState {
    /// Block not present; contents undefined. FileOffsetMB reserved.
    NotPresent = 0,
    /// Block contents undefined; may contain arbitrary data.
    Undefined = 1,
    /// Block contents are zero.
    Zero = 2,
    /// Block was unmapped; contents no longer relied upon.
    Unmapped = 3,
    /// Block fully present at FileOffsetMB.
    FullyPresent = 6,
    /// Block partially present (differencing only); sector bitmap required.
    PartiallyPresent = 7,
}

impl PayloadBlockState {
    /// Interpret a raw 3-bit state value as a payload block state.
    ///
    /// Returns `None` for reserved values (4, 5).
    pub(crate) fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::NotPresent),
            1 => Some(Self::Undefined),
            2 => Some(Self::Zero),
            3 => Some(Self::Unmapped),
            6 => Some(Self::FullyPresent),
            7 => Some(Self::PartiallyPresent),
            _ => None,
        }
    }
}

/// Sector bitmap block state (MS-VHDX §2.5.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorBitmapState {
    /// Block not allocated; contents undefined.
    NotPresent = 0,
    /// Block present at FileOffsetMB (differencing only).
    Present = 6,
}

impl SectorBitmapState {
    /// Interpret a raw 3-bit state value as a sector bitmap state.
    ///
    /// Returns `None` for reserved values (1-5, 7).
    pub(crate) fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::NotPresent),
            6 => Some(Self::Present),
            _ => None,
        }
    }
}

/// Interpreted BAT entry state, distinguishing payload vs sector bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatState {
    /// Entry describes a payload block.
    Payload(PayloadBlockState),
    /// Entry describes a sector bitmap block.
    SectorBitmap(SectorBitmapState),
}

/// Zero-copy view into a single 64-bit BAT entry.
///
/// Each entry encodes a 3-bit state field and a 44-bit file offset (in MB).
/// The entry also carries whether it represents a payload block or sector bitmap
/// block, determined by its position and the chunk ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatEntry<'a> {
    /// Raw 8 bytes from the BAT buffer.
    bytes: &'a [u8; 8],
    /// Whether this entry is a sector bitmap block entry (vs payload).
    is_sector_bitmap: bool,
}

impl<'a> BatEntry<'a> {
    /// Raw 3-bit state value (bits 0-2).
    ///
    /// Use [`payload_state`](Self::payload_state) or
    /// [`sector_bitmap_state`](Self::sector_bitmap_state) for typed interpretation.
    #[inline]
    pub(crate) fn raw_state(&self) -> u8 {
        self.bytes.view_bits::<Lsb0>()[0..3].load::<u8>()
    }

    /// File offset in MB (bits 20-63, 44-bit field).
    #[inline]
    pub fn file_offset_mb(&self) -> u64 {
        self.bytes.view_bits::<Lsb0>()[20..64].load::<u64>()
    }

    /// Interpret the raw state as a payload block state.
    ///
    /// Returns `None` for reserved values (4, 5).
    pub(crate) fn payload_state(&self) -> Option<PayloadBlockState> {
        PayloadBlockState::from_raw(self.raw_state())
    }

    /// Interpret the raw state as a sector bitmap state.
    ///
    /// Returns `None` for reserved values (1-5, 7).
    pub(crate) fn sector_bitmap_state(&self) -> Option<SectorBitmapState> {
        SectorBitmapState::from_raw(self.raw_state())
    }

    /// Typed state, resolved using the entry type determined by chunk-ratio interleaving.
    ///
    /// Returns an error for reserved/invalid raw state values (4, 5 for payload;
    /// 1-5, 7 for sector bitmap). Use [`payload_state`](Self::payload_state) or
    /// [`sector_bitmap_state`](Self::sector_bitmap_state) if you need a non-fallible check.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidBlockState`] for reserved/invalid payload states,
    /// or [`Error::InvalidSectorBitmapState`] for reserved/invalid sector bitmap states.
    pub fn state(&self) -> Result<BatState> {
        let raw = self.raw_state();
        if self.is_sector_bitmap {
            match SectorBitmapState::from_raw(raw) {
                Some(s) => Ok(BatState::SectorBitmap(s)),
                None => Err(Error::InvalidSectorBitmapState(raw)),
            }
        } else {
            match PayloadBlockState::from_raw(raw) {
                Some(s) => Ok(BatState::Payload(s)),
                None => Err(Error::InvalidBlockState(raw)),
            }
        }
    }

    /// Whether this entry represents a sector bitmap block (vs a payload block).
    pub(crate) fn is_sector_bitmap(&self) -> bool {
        self.is_sector_bitmap
    }
}

/// BAT (Block Allocation Table) — zero-copy parser over a raw byte buffer.
///
/// The BAT entries are interleaved: every `chunk_ratio` payload block entries
/// is followed by one sector bitmap block entry.
#[derive(Clone, Copy)]
pub struct Bat<'a> {
    /// Raw BAT region bytes.
    data: &'a [u8],
    /// Chunk ratio = (2²³ × LogicalSectorSize) / BlockSize.
    chunk_ratio: u64,
}

impl<'a> Bat<'a> {
    /// Create a new BAT parser from raw bytes and chunk ratio.
    ///
    /// `chunk_ratio` determines how payload and sector bitmap entries are interleaved.
    /// It is calculated as `(2^23 * LogicalSectorSize) / BlockSize`.
    pub(crate) fn new(data: &'a [u8], chunk_ratio: u64) -> Self {
        Self { data, chunk_ratio }
    }

    /// Total number of 64-bit entries in the BAT buffer.
    pub fn len(&self) -> usize {
        self.data.len() / 8
    }

    /// Whether the BAT buffer is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The chunk ratio used for payload/sector-bitmap interleaving.
    pub(crate) fn chunk_ratio(&self) -> u64 {
        self.chunk_ratio
    }

    /// Determine whether the entry at `index` is a sector bitmap block entry.
    ///
    /// Layout: every `chunk_ratio + 1` entries, the last one is a sector bitmap entry.
    fn is_sector_bitmap_entry(&self, index: usize) -> bool {
        if self.chunk_ratio == 0 {
            return false;
        }
        let stride = self.chunk_ratio as usize + 1;
        index % stride == self.chunk_ratio as usize
    }

    /// Get a zero-copy entry view by index.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BatEntryNotFound`] if the index is out of bounds.
    pub fn entry(&self, index: u64) -> Result<BatEntry<'a>> {
        let idx = index as usize;
        let offset = idx * 8;
        if offset.saturating_add(8) > self.data.len() {
            return Err(Error::BatEntryNotFound { index });
        }
        let bytes: &'a [u8; 8] = self.data[offset..offset + 8]
            .try_into()
            .expect("length already validated");
        Ok(BatEntry {
            bytes,
            is_sector_bitmap: self.is_sector_bitmap_entry(idx),
        })
    }

    /// Unchecked entry access (used by the iterator).
    fn entry_unchecked(&self, index: usize) -> BatEntry<'a> {
        let offset = index * 8;
        let bytes: &'a [u8; 8] = self.data[offset..offset + 8]
            .try_into()
            .expect("iterator index is within bounds");
        BatEntry {
            bytes,
            is_sector_bitmap: self.is_sector_bitmap_entry(index),
        }
    }

    /// Iterate all entries as zero-copy views.
    ///
    /// The returned iterator borrows this BAT and yields exactly [`len`](Self::len) items.
    pub fn entries(&self) -> impl Iterator<Item = BatEntry<'a>> + '_ {
        (0..self.len()).map(move |i| self.entry_unchecked(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a BAT buffer with the given 64-bit LE entries.
    fn make_buf(entries: &[u64]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(entries.len() * 8);
        for &e in entries {
            buf.extend_from_slice(&e.to_le_bytes());
        }
        buf
    }

    #[test]
    fn entry_count_matches_buffer() {
        let buf = make_buf(&[0, 0, 0, 0, 0, 0]);
        let bat = Bat::new(&buf, 4);
        assert_eq!(bat.len(), 6);
        assert!(!bat.is_empty());
    }

    #[test]
    fn empty_bat() {
        let bat = Bat::new(&[], 4);
        assert_eq!(bat.len(), 0);
        assert!(bat.is_empty());
    }

    #[test]
    fn state_extraction_all_payload_values() {
        // chunk_ratio = 4 → entries 0-3 are payload, entry 4 is sector bitmap
        let buf = make_buf(&[0, 1, 2, 3, 0, 6, 7, 4, 5]);
        let bat = Bat::new(&buf, 4);

        // Payload entries (indices 0-3)
        assert_eq!(bat.entry(0).unwrap().payload_state(), Some(PayloadBlockState::NotPresent));
        assert_eq!(bat.entry(1).unwrap().payload_state(), Some(PayloadBlockState::Undefined));
        assert_eq!(bat.entry(2).unwrap().payload_state(), Some(PayloadBlockState::Zero));
        assert_eq!(bat.entry(3).unwrap().payload_state(), Some(PayloadBlockState::Unmapped));

        // Entry 4 is sector bitmap
        assert!(bat.entry(4).unwrap().is_sector_bitmap());

        // Entry 5 = next chunk, payload again
        assert_eq!(bat.entry(5).unwrap().payload_state(), Some(PayloadBlockState::FullyPresent));
        assert_eq!(bat.entry(6).unwrap().payload_state(), Some(PayloadBlockState::PartiallyPresent));

        // Reserved values 4, 5 → None
        assert_eq!(bat.entry(7).unwrap().payload_state(), None);
        assert_eq!(bat.entry(8).unwrap().payload_state(), None);
    }

    #[test]
    fn state_extraction_sector_bitmap() {
        let buf = make_buf(&[0, 0, 0, 0, 0, 0, 0, 0, 6]);
        let bat = Bat::new(&buf, 4);

        // Entry 4 is sector bitmap
        let sb = bat.entry(4).unwrap();
        assert!(sb.is_sector_bitmap());
        assert_eq!(sb.sector_bitmap_state(), Some(SectorBitmapState::NotPresent));
        assert_eq!(sb.state().unwrap(), BatState::SectorBitmap(SectorBitmapState::NotPresent));

        // Entry 8 (index 8) = chunk_stride=5, 8%5=3 → payload, not SB
        // Entry 9 would be SB (9%5=4), but only 9 entries
        // Let's add more: entry 9 is SB
        let buf2 = make_buf(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 6]);
        let bat2 = Bat::new(&buf2, 4);
        let sb2 = bat2.entry(9).unwrap();
        assert!(sb2.is_sector_bitmap());
        assert_eq!(sb2.sector_bitmap_state(), Some(SectorBitmapState::Present));
    }

    #[test]
    fn file_offset_mb_extraction() {
        // FileOffsetMB in bits 20-63
        // Value = state in bits 0-2, offset_mb in bits 20-63
        let offset_mb: u64 = 0x1234; // arbitrary
        let mut raw_bytes = [0u8; 8];
        {
            let bits = raw_bytes.view_bits_mut::<Lsb0>();
            bits[0..3].store::<u8>(6u8); // FullyPresent
            bits[20..64].store::<u64>(offset_mb);
        }
        let buf = make_buf(&[u64::from_le_bytes(raw_bytes)]);
        let bat = Bat::new(&buf, 4);

        let entry = bat.entry(0).unwrap();
        assert_eq!(entry.raw_state(), 6);
        assert_eq!(entry.file_offset_mb(), offset_mb);
    }

    #[test]
    fn file_offset_mb_44bit_max() {
        // Max 44-bit value
        let max_offset: u64 = 0xFFF_FFFF_FFF;
        let mut raw_bytes = [0u8; 8];
        {
            let bits = raw_bytes.view_bits_mut::<Lsb0>();
            bits[0..3].store::<u8>(0u8); // NotPresent
            bits[20..64].store::<u64>(max_offset);
        }
        let buf = make_buf(&[u64::from_le_bytes(raw_bytes)]);
        let bat = Bat::new(&buf, 4);

        assert_eq!(bat.entry(0).unwrap().file_offset_mb(), max_offset);
    }

    #[test]
    fn entry_out_of_bounds() {
        let buf = make_buf(&[0, 0]);
        let bat = Bat::new(&buf, 4);

        assert!(bat.entry(2).is_err());
        assert!(bat.entry(100).is_err());
    }

    #[test]
    fn entries_iterator_yields_correct_count() {
        let buf = make_buf(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let bat = Bat::new(&buf, 4);

        let entries: Vec<_> = bat.entries().collect();
        assert_eq!(entries.len(), 8);

        // Verify each entry's raw_state matches the original value
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.raw_state(), i as u8);
        }
    }

    #[test]
    fn interleaving_pattern() {
        // chunk_ratio = 3 → stride = 4
        // Indices: 0=P, 1=P, 2=P, 3=SB, 4=P, 5=P, 6=P, 7=SB
        let buf = make_buf(&[0; 8]);
        let bat = Bat::new(&buf, 3);

        assert!(!bat.entry(0).unwrap().is_sector_bitmap());
        assert!(!bat.entry(1).unwrap().is_sector_bitmap());
        assert!(!bat.entry(2).unwrap().is_sector_bitmap());
        assert!(bat.entry(3).unwrap().is_sector_bitmap());
        assert!(!bat.entry(4).unwrap().is_sector_bitmap());
        assert!(!bat.entry(5).unwrap().is_sector_bitmap());
        assert!(!bat.entry(6).unwrap().is_sector_bitmap());
        assert!(bat.entry(7).unwrap().is_sector_bitmap());
    }

    #[test]
    fn bat_entry_state_returns_correct_variant() {
        let buf = make_buf(&[6, 0, 0, 0, 6]);
        let bat = Bat::new(&buf, 4);

        // Entry 0: payload, raw_state=6 → FullyPresent
        assert_eq!(
            bat.entry(0).unwrap().state().unwrap(),
            BatState::Payload(PayloadBlockState::FullyPresent)
        );

        // Entry 4: sector bitmap, raw_state=6 → Present
        assert_eq!(
            bat.entry(4).unwrap().state().unwrap(),
            BatState::SectorBitmap(SectorBitmapState::Present)
        );
    }

    #[test]
    fn invalid_state_4_returns_error_for_payload() {
        // chunk_ratio = 4 → entry 0 is payload with raw_state=4 (reserved)
        let buf = make_buf(&[4]);
        let bat = Bat::new(&buf, 4);
        let err = bat.entry(0).unwrap().state().unwrap_err();
        assert!(matches!(err, Error::InvalidBlockState(4)));
    }

    #[test]
    fn invalid_state_5_returns_error_for_payload() {
        // chunk_ratio = 4 → entry 0 is payload with raw_state=5 (reserved)
        let buf = make_buf(&[5]);
        let bat = Bat::new(&buf, 4);
        let err = bat.entry(0).unwrap().state().unwrap_err();
        assert!(matches!(err, Error::InvalidBlockState(5)));
    }
}
