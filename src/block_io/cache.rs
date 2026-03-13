//! Block cache for performance optimization
//!
//! Provides an in-memory cache for frequently accessed blocks
//! to reduce I/O operations.

use std::collections::HashMap;

/// Block cache for performance optimization
///
/// Caches recently accessed blocks in memory to reduce disk I/O.
/// Implements a simple eviction policy when the cache is full.
#[derive(Debug)]
pub struct BlockCache {
    /// Cached blocks: block_idx -> block_data
    cache: HashMap<u64, Vec<u8>>,
    /// Cache size limit in blocks
    max_blocks: usize,
    /// Access count for LRU tracking
    access_count: HashMap<u64, u64>,
    /// Global access counter
    global_counter: u64,
}

impl BlockCache {
    /// Create new block cache with specified capacity
    ///
    /// # Arguments
    /// * `max_blocks` - Maximum number of blocks to cache
    pub fn new(max_blocks: usize) -> Self {
        BlockCache {
            cache: HashMap::with_capacity(max_blocks),
            max_blocks,
            access_count: HashMap::new(),
            global_counter: 0,
        }
    }

    /// Get cached block
    ///
    /// # Arguments
    /// * `block_idx` - The block index to retrieve
    ///
    /// # Returns
    /// Some reference to block data if cached, None otherwise
    pub fn get(&mut self, block_idx: u64) -> Option<&Vec<u8>> {
        if self.cache.contains_key(&block_idx) {
            // Update access count
            self.global_counter += 1;
            self.access_count.insert(block_idx, self.global_counter);
            self.cache.get(&block_idx)
        } else {
            None
        }
    }

    /// Put block in cache
    ///
    /// # Arguments
    /// * `block_idx` - The block index to cache
    /// * `data` - The block data to cache
    pub fn put(&mut self, block_idx: u64, data: Vec<u8>) {
        if self.cache.len() >= self.max_blocks {
            // Evict least recently used entry
            self.evict_lru();
        }

        self.global_counter += 1;
        self.access_count.insert(block_idx, self.global_counter);
        self.cache.insert(block_idx, data);
    }

    /// Invalidate cached block
    ///
    /// # Arguments
    /// * `block_idx` - The block index to invalidate
    pub fn invalidate(&mut self, block_idx: u64) {
        self.cache.remove(&block_idx);
        self.access_count.remove(&block_idx);
    }

    /// Clear entire cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_count.clear();
        self.global_counter = 0;
    }

    /// Get current cache size (number of blocks)
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get cache capacity
    pub fn capacity(&self) -> usize {
        self.max_blocks
    }

    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some((&oldest_key, _)) = self.access_count.iter().min_by_key(|(_, &count)| count) {
            self.cache.remove(&oldest_key);
            self.access_count.remove(&oldest_key);
        }
    }
}

impl Default for BlockCache {
    fn default() -> Self {
        BlockCache::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_cache_basic() {
        let mut cache = BlockCache::new(10);

        // Put some blocks
        for i in 0..5 {
            cache.put(i, vec![i as u8; 1024]);
        }

        // Retrieve blocks
        for i in 0..5 {
            assert!(cache.get(i).is_some());
            assert_eq!(cache.get(i).unwrap()[0], i as u8);
        }

        // Check cache size
        assert_eq!(cache.len(), 5);
    }

    #[test]
    fn test_block_cache_eviction() {
        let mut cache = BlockCache::new(5);

        // Put more blocks than capacity
        for i in 0..10 {
            cache.put(i, vec![i as u8; 1024]);
        }

        // Cache size should not exceed capacity
        assert!(cache.len() <= 5);
        assert_eq!(cache.len(), 5);

        // Recent blocks should still be in cache
        for i in 5..10 {
            assert!(cache.get(i).is_some());
        }
    }

    #[test]
    fn test_block_cache_invalidate() {
        let mut cache = BlockCache::new(10);

        cache.put(1, vec![1; 1024]);
        cache.put(2, vec![2; 1024]);

        assert!(cache.get(1).is_some());

        cache.invalidate(1);

        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
    }

    #[test]
    fn test_block_cache_clear() {
        let mut cache = BlockCache::new(10);

        for i in 0..5 {
            cache.put(i, vec![i as u8; 1024]);
        }

        cache.clear();

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        for i in 0..5 {
            assert!(cache.get(i).is_none());
        }
    }

    #[test]
    fn test_block_cache_default() {
        let cache: BlockCache = Default::default();
        assert_eq!(cache.capacity(), 100);
        assert!(cache.is_empty());
    }
}
