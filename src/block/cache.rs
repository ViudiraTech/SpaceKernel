/*
 *
 *       src/block/cache.rs
 *       LRU sector buffer cache for block storage devices
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Bounded LRU sector cache with write-through and flush support.

use super::{BlockDevice, BlockError, SECTOR_SIZE};
use crate::sync::SpinLock;
use alloc::sync::Arc;

const CACHE_SLOTS: usize = 256;

#[derive(Clone, Copy, Debug, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub flushes: u64,
    pub evictions: u64,
}

struct CacheEntry {
    valid: bool,
    dirty: bool,
    dev_id: usize,
    lba: u64,
    last_used: u64,
    data: [u8; SECTOR_SIZE],
}

impl CacheEntry {
    const fn empty() -> Self {
        Self {
            valid: false,
            dirty: false,
            dev_id: 0,
            lba: 0,
            last_used: 0,
            data: [0u8; SECTOR_SIZE],
        }
    }
}

struct BlockCache {
    entries: [CacheEntry; CACHE_SLOTS],
    clock: u64,
    stats: CacheStats,
}

impl BlockCache {
    const fn new() -> Self {
        Self {
            entries: [const { CacheEntry::empty() }; CACHE_SLOTS],
            clock: 0,
            stats: CacheStats {
                hits: 0,
                misses: 0,
                writes: 0,
                flushes: 0,
                evictions: 0,
            },
        }
    }

    fn find_slot(&mut self, dev_id: usize, lba: u64) -> Option<usize> {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if entry.valid && entry.dev_id == dev_id && entry.lba == lba {
                self.clock = self.clock.wrapping_add(1);
                entry.last_used = self.clock;
                self.stats.hits = self.stats.hits.saturating_add(1);
                return Some(i);
            }
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        None
    }

    fn select_victim(&mut self) -> usize {
        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.valid {
                return i;
            }
        }
        let mut min_idx = 0;
        let mut min_time = u64::MAX;
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.last_used < min_time {
                min_time = entry.last_used;
                min_idx = i;
            }
        }
        self.stats.evictions = self.stats.evictions.saturating_add(1);
        min_idx
    }
}

static CACHE: SpinLock<BlockCache> = SpinLock::new(BlockCache::new());

/// Read a single 512-byte sector through the global LRU sector cache.
pub fn read_sector(
    dev_id: usize,
    dev: &Arc<dyn BlockDevice>,
    lba: u64,
    out: &mut [u8; SECTOR_SIZE],
) -> Result<(), BlockError> {
    if dev.info().block_size != SECTOR_SIZE {
        return dev.read_blocks(lba, 1, out);
    }
    {
        let mut cache = CACHE.lock();
        if let Some(slot) = cache.find_slot(dev_id, lba) {
            out.copy_from_slice(&cache.entries[slot].data);
            return Ok(());
        }
    }

    // Cache miss: read from underlying hardware block device outside the cache lock
    let mut sector = [0u8; SECTOR_SIZE];
    dev.read_blocks(lba, 1, &mut sector)?;

    {
        let mut cache = CACHE.lock();
        let slot = cache.select_victim();
        cache.clock = cache.clock.wrapping_add(1);
        let clock = cache.clock;
        cache.entries[slot] = CacheEntry {
            valid: true,
            dirty: false,
            dev_id,
            lba,
            last_used: clock,
            data: sector,
        };
    }
    out.copy_from_slice(&sector);
    Ok(())
}

/// Write a single 512-byte sector through the global LRU sector cache (write-through).
pub fn write_sector(
    dev_id: usize,
    dev: &Arc<dyn BlockDevice>,
    lba: u64,
    data: &[u8; SECTOR_SIZE],
) -> Result<(), BlockError> {
    if dev.info().block_size != SECTOR_SIZE {
        return dev.write_blocks(lba, 1, data);
    }

    // Write through to the underlying device immediately for crash consistency
    dev.write_blocks(lba, 1, data)?;

    let mut cache = CACHE.lock();
    cache.stats.writes = cache.stats.writes.saturating_add(1);
    if let Some(slot) = cache.find_slot(dev_id, lba) {
        // Undo the hit increment from find_slot since this is a write update
        cache.stats.hits = cache.stats.hits.saturating_sub(1);
        cache.entries[slot].data.copy_from_slice(data);
        cache.entries[slot].dirty = false;
    } else {
        // Undo the miss increment from find_slot
        cache.stats.misses = cache.stats.misses.saturating_sub(1);
        let slot = cache.select_victim();
        cache.clock = cache.clock.wrapping_add(1);
        let clock = cache.clock;
        cache.entries[slot] = CacheEntry {
            valid: true,
            dirty: false,
            dev_id,
            lba,
            last_used: clock,
            data: *data,
        };
    }
    Ok(())
}

/// Invalidate cached sectors for a specific device.
pub fn invalidate_device(dev_id: usize) {
    let mut cache = CACHE.lock();
    for entry in cache.entries.iter_mut() {
        if entry.valid && entry.dev_id == dev_id {
            entry.valid = false;
            entry.dirty = false;
        }
    }
}

/// Record a flush operation and return current cache statistics.
pub fn record_flush() {
    let mut cache = CACHE.lock();
    cache.stats.flushes = cache.stats.flushes.saturating_add(1);
}

/// Return current block cache statistics.
pub fn stats() -> CacheStats {
    CACHE.lock().stats
}
