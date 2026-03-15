//! VHDX Chunk calculation logic
//!
//! Implements chunk size and chunk ratio calculations according to MS-VHDX.
//!
//! Chunk Formula:
//! - ChunkSize = 2^23 * LogicalSectorSize
//! - ChunkRatio = ChunkSize / BlockSize
//!
//! MS-VHDX Section 2.4: The chunk size is fixed at 2^23 times the logical
//! sector size, which is 4GB for 512-byte sectors or 32GB for 4096-byte sectors.

/// Chunk-related information and calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkInfo {
    /// Chunk size in bytes
    pub chunk_size: u64,
    /// Number of payload blocks per chunk (chunk ratio)
    pub chunk_ratio: u64,
    /// Logical sector size in bytes
    pub logical_sector_size: u32,
    /// Block size in bytes
    pub block_size: u64,
}

impl ChunkInfo {
    /// Create new ChunkInfo from block size and logical sector size
    ///
    /// # Arguments
    /// * `block_size` - Block size in bytes
    /// * `logical_sector_size` - Logical sector size in bytes
    ///
    /// # Returns
    /// New ChunkInfo with calculated chunk_size and chunk_ratio
    pub fn new(block_size: u64, logical_sector_size: u32) -> Self {
        let chunk_size = Self::calculate_chunk_size(logical_sector_size);
        let chunk_ratio = Self::calculate_chunk_ratio(block_size, logical_sector_size);

        ChunkInfo {
            chunk_size,
            chunk_ratio,
            logical_sector_size,
            block_size,
        }
    }

    /// Calculate chunk size
    ///
    /// Formula: ChunkSize = 2^23 * LogicalSectorSize
    ///
    /// # Arguments
    /// * `logical_sector_size` - Logical sector size in bytes
    ///
    /// # Returns
    /// Chunk size in bytes
    ///
    /// # Examples
    /// - 512 byte sectors -> 4GB chunks
    /// - 4096 byte sectors -> 32GB chunks
    pub fn calculate_chunk_size(logical_sector_size: u32) -> u64 {
        (1u64 << 23) * logical_sector_size as u64
    }

    /// Calculate chunk ratio (payload blocks per chunk)
    ///
    /// Formula: ChunkRatio = ChunkSize / BlockSize
    ///
    /// # Arguments
    /// * `block_size` - Block size in bytes
    /// * `logical_sector_size` - Logical sector size in bytes
    ///
    /// # Returns
    /// Number of payload blocks per chunk
    pub fn calculate_chunk_ratio(block_size: u64, logical_sector_size: u32) -> u64 {
        let chunk_size = Self::calculate_chunk_size(logical_sector_size);
        chunk_size / block_size
    }

    /// Calculate chunk index from virtual offset
    ///
    /// # Arguments
    /// * `virtual_offset` - Virtual disk offset in bytes
    ///
    /// # Returns
    /// Chunk index
    pub fn chunk_index_from_offset(&self, virtual_offset: u64) -> u64 {
        virtual_offset / self.chunk_size
    }

    /// Calculate block index from virtual offset
    ///
    /// # Arguments
    /// * `virtual_offset` - Virtual disk offset in bytes
    ///
    /// # Returns
    /// Block index (global, not within chunk)
    pub fn block_index_from_offset(&self, virtual_offset: u64) -> u64 {
        virtual_offset / self.block_size
    }

    /// Calculate offset within block from virtual offset
    ///
    /// # Arguments
    /// * `virtual_offset` - Virtual disk offset in bytes
    ///
    /// # Returns
    /// Offset within the block (0 to block_size-1)
    pub fn offset_in_block(&self, virtual_offset: u64) -> u64 {
        virtual_offset % self.block_size
    }

    /// Calculate block index within chunk
    ///
    /// # Arguments
    /// * `block_idx` - Global block index
    ///
    /// # Returns
    /// Block index within the chunk (0 to chunk_ratio-1)
    pub fn block_index_in_chunk(&self, block_idx: u64) -> u64 {
        block_idx % self.chunk_ratio
    }

    /// Calculate chunk index from block index
    ///
    /// # Arguments
    /// * `block_idx` - Global block index
    ///
    /// # Returns
    /// Chunk index
    pub fn chunk_index_from_block(&self, block_idx: u64) -> u64 {
        block_idx / self.chunk_ratio
    }

    /// Calculate BAT index for a payload block
    ///
    /// Formula: bat_index = chunk_index * (chunk_ratio + 1) + block_in_chunk
    ///
    /// # Arguments
    /// * `block_idx` - Global block index
    ///
    /// # Returns
    /// BAT index for the payload block
    pub fn payload_bat_index(&self, block_idx: u64) -> u64 {
        let chunk_idx = self.chunk_index_from_block(block_idx);
        let block_in_chunk = self.block_index_in_chunk(block_idx);
        chunk_idx * (self.chunk_ratio + 1) + block_in_chunk
    }

    /// Calculate BAT index for a sector bitmap block
    ///
    /// Formula: bat_index = chunk_idx * (chunk_ratio + 1) + chunk_ratio
    ///
    /// # Arguments
    /// * `chunk_idx` - Chunk index
    ///
    /// # Returns
    /// BAT index for the sector bitmap block
    pub fn sector_bitmap_bat_index(&self, chunk_idx: u64) -> u64 {
        chunk_idx * (self.chunk_ratio + 1) + self.chunk_ratio
    }

    /// Calculate number of payload blocks needed for a virtual disk size
    ///
    /// # Arguments
    /// * `virtual_disk_size` - Virtual disk size in bytes
    ///
    /// # Returns
    /// Number of payload blocks (rounded up)
    pub fn num_payload_blocks(&self, virtual_disk_size: u64) -> u64 {
        virtual_disk_size.div_ceil(self.block_size)
    }

    /// Calculate number of sector bitmap blocks needed
    ///
    /// # Arguments
    /// * `num_payload_blocks` - Number of payload blocks
    ///
    /// # Returns
    /// Number of sector bitmap blocks (rounded up)
    pub fn num_sector_bitmap_blocks(&self, num_payload_blocks: u64) -> u64 {
        num_payload_blocks.div_ceil(self.chunk_ratio)
    }

    /// Calculate total number of BAT entries needed
    ///
    /// # Arguments
    /// * `virtual_disk_size` - Virtual disk size in bytes
    ///
    /// # Returns
    /// Total number of BAT entries (payload + sector bitmap)
    pub fn total_bat_entries(&self, virtual_disk_size: u64) -> u64 {
        let num_payload = self.num_payload_blocks(virtual_disk_size);
        let num_bitmap = self.num_sector_bitmap_blocks(num_payload);
        num_payload + num_bitmap
    }
}

/// Utility struct for chunk calculations
pub struct ChunkCalculator;

impl ChunkCalculator {
    /// Calculate chunk size
    ///
    /// Formula: ChunkSize = 2^23 * LogicalSectorSize
    pub fn chunk_size(logical_sector_size: u32) -> u64 {
        ChunkInfo::calculate_chunk_size(logical_sector_size)
    }

    /// Calculate chunk ratio
    ///
    /// Formula: ChunkRatio = ChunkSize / BlockSize
    pub fn chunk_ratio(block_size: u64, logical_sector_size: u32) -> u64 {
        ChunkInfo::calculate_chunk_ratio(block_size, logical_sector_size)
    }

    /// Calculate number of payload blocks
    pub fn num_payload_blocks(virtual_disk_size: u64, block_size: u64) -> u64 {
        virtual_disk_size.div_ceil(block_size)
    }

    /// Calculate number of sector bitmap blocks
    pub fn num_sector_bitmap_blocks(num_payload_blocks: u64, chunk_ratio: u64) -> u64 {
        num_payload_blocks.div_ceil(chunk_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_size_calculation() {
        // 512 byte sectors -> 4GB chunks
        assert_eq!(ChunkInfo::calculate_chunk_size(512), 4 * 1024 * 1024 * 1024);

        // 4096 byte sectors -> 32GB chunks
        assert_eq!(
            ChunkInfo::calculate_chunk_size(4096),
            32 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_chunk_ratio_calculation() {
        // 1MB blocks, 512 byte sectors -> 4096 blocks per chunk
        let chunk_ratio = ChunkInfo::calculate_chunk_ratio(1024 * 1024, 512);
        assert_eq!(chunk_ratio, 4096);

        // 32MB blocks, 512 byte sectors -> 128 blocks per chunk
        let chunk_ratio = ChunkInfo::calculate_chunk_ratio(32 * 1024 * 1024, 512);
        assert_eq!(chunk_ratio, 128);

        // 1MB blocks, 4096 byte sectors -> 32768 blocks per chunk
        let chunk_ratio = ChunkInfo::calculate_chunk_ratio(1024 * 1024, 4096);
        assert_eq!(chunk_ratio, 32768);
    }

    #[test]
    fn test_chunk_info() {
        let info = ChunkInfo::new(1024 * 1024, 512);

        assert_eq!(info.chunk_size, 4 * 1024 * 1024 * 1024);
        assert_eq!(info.chunk_ratio, 4096);
        assert_eq!(info.logical_sector_size, 512);
        assert_eq!(info.block_size, 1024 * 1024);
    }

    #[test]
    fn test_offset_calculations() {
        let info = ChunkInfo::new(1024 * 1024, 512);

        // Block 0 at offset 0
        assert_eq!(info.block_index_from_offset(0), 0);
        assert_eq!(info.offset_in_block(0), 0);

        // Block 1 at offset 1MB
        assert_eq!(info.block_index_from_offset(1024 * 1024), 1);
        assert_eq!(info.offset_in_block(1024 * 1024), 0);

        // Offset 512KB within block 0
        assert_eq!(info.block_index_from_offset(512 * 1024), 0);
        assert_eq!(info.offset_in_block(512 * 1024), 512 * 1024);

        // Chunk boundaries
        assert_eq!(info.chunk_index_from_offset(0), 0);
        assert_eq!(info.chunk_index_from_offset(4 * 1024 * 1024 * 1024), 1);
    }

    #[test]
    fn test_bat_index_calculations() {
        let info = ChunkInfo::new(1024 * 1024, 512); // 1MB blocks, 4096 ratio

        // Block 0: chunk 0, block 0 -> index 0
        assert_eq!(info.payload_bat_index(0), 0);

        // Block 4095: chunk 0, block 4095 -> index 4095
        assert_eq!(info.payload_bat_index(4095), 4095);

        // Sector bitmap 0: chunk 0, after all payload blocks -> index 4096
        assert_eq!(info.sector_bitmap_bat_index(0), 4096);

        // Block 4096: chunk 1, block 0 -> index 4097 (after sector bitmap of chunk 0)
        assert_eq!(info.payload_bat_index(4096), 4097);

        // Sector bitmap 1: chunk 1 -> index 8193
        assert_eq!(info.sector_bitmap_bat_index(1), 8193);
    }

    #[test]
    fn test_block_counts() {
        let info = ChunkInfo::new(1024 * 1024, 512); // 1MB blocks

        // 100GB disk
        let virtual_size = 100 * 1024 * 1024 * 1024u64;
        let num_payload = info.num_payload_blocks(virtual_size);
        assert_eq!(num_payload, 100 * 1024); // 100K blocks

        let num_bitmap = info.num_sector_bitmap_blocks(num_payload);
        assert_eq!(num_bitmap, 25); // 100K / 4096 = 24.4, rounded up

        let total = info.total_bat_entries(virtual_size);
        assert_eq!(total, 100 * 1024 + 25);
    }

    #[test]
    fn test_chunk_calculator() {
        assert_eq!(ChunkCalculator::chunk_size(512), 4 * 1024 * 1024 * 1024);
        assert_eq!(ChunkCalculator::chunk_ratio(1024 * 1024, 512), 4096);
        assert_eq!(
            ChunkCalculator::num_payload_blocks(100 * 1024 * 1024 * 1024, 1024 * 1024),
            100 * 1024
        );
    }
}
