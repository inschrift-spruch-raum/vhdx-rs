//! VHDX Sector Bitmap operations
//!
//! Implements Sector Bitmap Block operations for differencing disks.
//! The Sector Bitmap tracks which sectors within a chunk are present
//! in the current file vs. the parent file.
//!
//! MS-VHDX Section 2.4: A sector bitmap block has one bit for each
//! sector in a payload block. If the bit is set, the sector is present
//! in this file; if clear, it must be read from the parent.

/// Sector Bitmap operations for tracking sector presence in differencing disks
pub struct SectorBitmap;

impl SectorBitmap {
    /// Check if a sector is present in the current file
    ///
    /// For differencing disks, returns true if data should be read from
    /// this file, false if it should be read from parent.
    ///
    /// # Arguments
    /// * `bitmap` - The sector bitmap byte array
    /// * `sector_idx` - The sector index within the chunk
    ///
    /// # Returns
    /// `true` if the sector is present in this file
    pub fn is_sector_present(bitmap: &[u8], sector_idx: u64) -> bool {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx >= bitmap.len() {
            return false;
        }

        (bitmap[byte_idx] >> bit_idx) & 1 == 1
    }

    /// Set a sector as present (mark bit as 1)
    ///
    /// # Arguments
    /// * `bitmap` - The sector bitmap byte array
    /// * `sector_idx` - The sector index within the chunk
    pub fn set_sector_present(bitmap: &mut [u8], sector_idx: u64) {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx < bitmap.len() {
            bitmap[byte_idx] |= 1 << bit_idx;
        }
    }

    /// Clear a sector (mark as not present, set bit to 0)
    ///
    /// # Arguments
    /// * `bitmap` - The sector bitmap byte array
    /// * `sector_idx` - The sector index within the chunk
    pub fn clear_sector(bitmap: &mut [u8], sector_idx: u64) {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx < bitmap.len() {
            bitmap[byte_idx] &= !(1 << bit_idx);
        }
    }

    /// Toggle a sector's presence state
    ///
    /// # Arguments
    /// * `bitmap` - The sector bitmap byte array
    /// * `sector_idx` - The sector index within the chunk
    pub fn toggle_sector(bitmap: &mut [u8], sector_idx: u64) {
        let byte_idx = (sector_idx / 8) as usize;
        let bit_idx = (sector_idx % 8) as usize;

        if byte_idx < bitmap.len() {
            bitmap[byte_idx] ^= 1 << bit_idx;
        }
    }

    /// Calculate sector index within chunk from virtual offset
    ///
    /// # Arguments
    /// * `virtual_offset` - The virtual disk offset
    /// * `logical_sector_size` - Size of a logical sector in bytes
    /// * `chunk_size` - Size of a chunk in bytes
    ///
    /// # Returns
    /// The sector index within the chunk
    pub fn sector_index_in_chunk(
        virtual_offset: u64,
        logical_sector_size: u32,
        chunk_size: u64,
    ) -> u64 {
        let offset_in_chunk = virtual_offset % chunk_size;
        offset_in_chunk / logical_sector_size as u64
    }

    /// Calculate the required bitmap size in bytes for a given number of sectors
    ///
    /// # Arguments
    /// * `num_sectors` - Number of sectors to track
    ///
    /// # Returns
    /// Required bitmap size in bytes (rounded up to nearest byte)
    pub fn bitmap_size_for_sectors(num_sectors: u64) -> usize {
        ((num_sectors + 7) / 8) as usize
    }

    /// Calculate the required bitmap size for a chunk
    ///
    /// # Arguments
    /// * `chunk_size` - Size of a chunk in bytes
    /// * `logical_sector_size` - Size of a logical sector in bytes
    ///
    /// # Returns
    /// Required bitmap size in bytes
    pub fn bitmap_size_for_chunk(chunk_size: u64, logical_sector_size: u32) -> usize {
        let num_sectors = chunk_size / logical_sector_size as u64;
        Self::bitmap_size_for_sectors(num_sectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_bitmap_operations() {
        let mut bitmap = vec![0u8; 1024]; // 1KB bitmap

        // Initially all sectors should be not present
        assert!(!SectorBitmap::is_sector_present(&bitmap, 0));
        assert!(!SectorBitmap::is_sector_present(&bitmap, 100));

        // Set sector 0
        SectorBitmap::set_sector_present(&mut bitmap, 0);
        assert!(SectorBitmap::is_sector_present(&bitmap, 0));
        assert!(!SectorBitmap::is_sector_present(&bitmap, 1));

        // Set sector 100
        SectorBitmap::set_sector_present(&mut bitmap, 100);
        assert!(SectorBitmap::is_sector_present(&bitmap, 100));

        // Clear sector 0
        SectorBitmap::clear_sector(&mut bitmap, 0);
        assert!(!SectorBitmap::is_sector_present(&bitmap, 0));

        // Toggle sector 0 back on
        SectorBitmap::toggle_sector(&mut bitmap, 0);
        assert!(SectorBitmap::is_sector_present(&bitmap, 0));
    }

    #[test]
    fn test_sector_index_calculation() {
        // 4GB chunk, 512 byte sectors
        let chunk_size = 4 * 1024 * 1024 * 1024u64;
        let logical_sector_size = 512u32;

        // Offset 0 should be sector 0
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(0, logical_sector_size, chunk_size),
            0
        );

        // Offset 512 should be sector 1
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(512, logical_sector_size, chunk_size),
            1
        );

        // Offset 4GB should be sector 0 of next chunk
        assert_eq!(
            SectorBitmap::sector_index_in_chunk(chunk_size, logical_sector_size, chunk_size),
            0
        );
    }

    #[test]
    fn test_bitmap_size_calculation() {
        // 8 sectors need 1 byte
        assert_eq!(SectorBitmap::bitmap_size_for_sectors(8), 1);

        // 9 sectors need 2 bytes
        assert_eq!(SectorBitmap::bitmap_size_for_sectors(9), 2);

        // 1024 sectors need 128 bytes
        assert_eq!(SectorBitmap::bitmap_size_for_sectors(1024), 128);

        // For a 4GB chunk with 512-byte sectors
        let chunk_size = 4 * 1024 * 1024 * 1024u64;
        let logical_sector_size = 512u32;
        let num_sectors = chunk_size / logical_sector_size as u64;
        assert_eq!(
            SectorBitmap::bitmap_size_for_chunk(chunk_size, logical_sector_size),
            (num_sectors / 8) as usize
        );
    }

    #[test]
    fn test_out_of_bounds() {
        let mut bitmap = vec![0u8; 1]; // Only 8 sectors

        // Out of bounds should return false, not panic
        assert!(!SectorBitmap::is_sector_present(&bitmap, 100));

        // Out of bounds set should be a no-op
        SectorBitmap::set_sector_present(&mut bitmap, 100);
        assert_eq!(bitmap[0], 0);
    }
}
