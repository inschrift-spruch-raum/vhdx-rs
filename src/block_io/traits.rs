//! Block I/O traits
//!
//! Defines the common interface for block-level I/O operations
//! across different VHDX disk types.

use crate::error::Result;

/// Trait for block-level I/O operations
///
/// This trait defines the common interface for reading from and writing to
/// virtual disk blocks across fixed, dynamic, and differencing disk types.
pub trait BlockIo {
    /// Read data from virtual offset
    ///
    /// # Arguments
    /// * `virtual_offset` - The virtual offset within the disk to read from
    /// * `buf` - The buffer to read data into
    ///
    /// # Returns
    /// The number of bytes read (may be less than requested for sparse regions)
    fn read(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;

    /// Write data to virtual offset
    ///
    /// For dynamic and differencing disks, this may allocate new blocks.
    ///
    /// # Arguments
    /// * `virtual_offset` - The virtual offset within the disk to write to
    /// * `buf` - The data to write
    ///
    /// # Returns
    /// The number of bytes written
    fn write(&mut self, virtual_offset: u64, buf: &[u8]) -> Result<usize>;

    /// Get virtual disk size
    fn virtual_disk_size(&self) -> u64;

    /// Get block size
    fn block_size(&self) -> u32;
}

/// Trait for block allocation operations (dynamic and differencing disks)
pub trait BlockAllocator {
    /// Allocate a new block
    ///
    /// # Arguments
    /// * `block_idx` - The block index to allocate
    ///
    /// # Returns
    /// The file offset of the allocated block
    fn allocate_block(&mut self, block_idx: u64) -> Result<u64>;

    /// Check if a block is allocated
    ///
    /// # Arguments
    /// * `block_idx` - The block index to check
    ///
    /// # Returns
    /// `true` if the block is allocated, `false` otherwise
    fn is_block_allocated(&self, block_idx: u64) -> bool;
}

/// Trait for differencing disk operations
pub trait DifferencingIo: BlockIo {
    /// Set parent for differencing disks
    fn set_parent(&mut self, parent: Box<dyn BlockIo>);

    /// Check if this disk has a parent
    fn has_parent(&self) -> bool;

    /// Read from parent disk (for unallocated blocks)
    fn read_from_parent(&mut self, virtual_offset: u64, buf: &mut [u8]) -> Result<usize>;
}
