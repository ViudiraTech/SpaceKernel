/*
 *
 *       src/block/bench.rs
 *       Hardware storage driver benchmark suite (IDE vs AHCI vs NVMe vs RAMDisk)
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Performance benchmark suite measuring 4 KiB sequential DMA read/write throughput,
//! 512 B single-sector IOPS/latency, LRU cache hit throughput, and hardware flush latency.

use super::{BlockDevice, BlockDeviceKind, SECTOR_SIZE, cache, devices};
use crate::{kinfo, time};
use alloc::{string::String, sync::Arc, vec::Vec};

#[derive(Clone, Debug)]
pub struct DriverBenchResult {
    pub name: String,
    pub kind: &'static str,
    pub model: String,
    // 4 KiB (8-sector) sequential read (512 KiB total = 128 I/Os)
    pub seq_read_kib_s: u64,
    pub seq_read_iops: u64,
    pub seq_read_lat_us: u64,
    // 4 KiB (8-sector) sequential write (512 KiB total = 128 I/Os)
    pub seq_write_kib_s: u64,
    pub seq_write_iops: u64,
    pub seq_write_lat_us: u64,
    // 512 B single-sector uncached read (128 sectors)
    pub rand_read_kib_s: u64,
    pub rand_read_iops: u64,
    pub rand_read_lat_us: u64,
    // 512 B single-sector uncached write (128 sectors)
    pub rand_write_kib_s: u64,
    pub rand_write_iops: u64,
    pub rand_write_lat_us: u64,
    // 512 B hot LRU cache read (1024 hits)
    pub cache_read_kib_s: u64,
    pub cache_read_iops: u64,
    // Hardware flush latency (average of 16 flushes)
    pub flush_lat_us: u64,
}

pub fn benchmark_device(dev_id: usize, dev: &Arc<dyn BlockDevice>) -> Option<DriverBenchResult> {
    let info = dev.info();
    if info.kind == BlockDeviceKind::Partition
        || info.read_only
        || info.block_size != SECTOR_SIZE
        || info.total_blocks < 3072
    {
        return None;
    }

    // Use LBA 2048..3072 (512 KiB = 1024 sectors) in the unallocated tail of the disk
    let base_lba = 2048u64;
    let seq_io_sectors = 8usize; // 4 KiB per request
    let seq_io_count = 128usize; // 128 * 4 KiB = 512 KiB total
    let seq_total_kib = (seq_io_sectors * seq_io_count * SECTOR_SIZE / 1024) as u64;

    let mut buf_4k = [0u8; 4096];
    for (i, b) in buf_4k.iter_mut().enumerate() {
        *b = ((i * 13 + dev_id * 7 + 0x3C) & 0xFF) as u8;
    }

    // 1. 4 KiB Sequential Write (512 KiB)
    let t0 = time::uptime_micros();
    for i in 0..seq_io_count {
        let lba = base_lba + (i * seq_io_sectors) as u64;
        dev.write_blocks(lba, seq_io_sectors, &buf_4k).ok()?;
    }
    let seq_write_elapsed_us = time::uptime_micros().saturating_sub(t0).max(1);
    let _ = dev.flush();
    let seq_write_kib_s = (seq_total_kib * 1_000_000) / seq_write_elapsed_us;
    let seq_write_iops = (seq_io_count as u64 * 1_000_000) / seq_write_elapsed_us;
    let seq_write_lat_us = seq_write_elapsed_us / (seq_io_count as u64);

    // 2. 4 KiB Sequential Read (512 KiB)
    let mut read_4k = [0u8; 4096];
    let t1 = time::uptime_micros();
    for i in 0..seq_io_count {
        let lba = base_lba + (i * seq_io_sectors) as u64;
        dev.read_blocks(lba, seq_io_sectors, &mut read_4k).ok()?;
    }
    let seq_read_elapsed_us = time::uptime_micros().saturating_sub(t1).max(1);
    let seq_read_kib_s = (seq_total_kib * 1_000_000) / seq_read_elapsed_us;
    let seq_read_iops = (seq_io_count as u64 * 1_000_000) / seq_read_elapsed_us;
    let seq_read_lat_us = seq_read_elapsed_us / (seq_io_count as u64);

    // 3. 512 B Single-Sector Uncached Write (128 sectors = 64 KiB)
    let single_count = 128usize;
    let single_kib = (single_count * SECTOR_SIZE / 1024) as u64;
    let sec_write = [0xA5u8; SECTOR_SIZE];
    let t2 = time::uptime_micros();
    for i in 0..single_count {
        let lba = base_lba + ((i * 7) % 512) as u64;
        dev.write_blocks(lba, 1, &sec_write).ok()?;
    }
    let rand_write_elapsed_us = time::uptime_micros().saturating_sub(t2).max(1);
    let rand_write_kib_s = (single_kib * 1_000_000) / rand_write_elapsed_us;
    let rand_write_iops = (single_count as u64 * 1_000_000) / rand_write_elapsed_us;
    let rand_write_lat_us = rand_write_elapsed_us / (single_count as u64);

    // 4. 512 B Single-Sector Uncached Read (128 sectors = 64 KiB)
    let mut sec_read = [0u8; SECTOR_SIZE];
    let t3 = time::uptime_micros();
    for i in 0..single_count {
        let lba = base_lba + ((i * 7) % 512) as u64;
        dev.read_blocks(lba, 1, &mut sec_read).ok()?;
    }
    let rand_read_elapsed_us = time::uptime_micros().saturating_sub(t3).max(1);
    let rand_read_kib_s = (single_kib * 1_000_000) / rand_read_elapsed_us;
    let rand_read_iops = (single_count as u64 * 1_000_000) / rand_read_elapsed_us;
    let rand_read_lat_us = rand_read_elapsed_us / (single_count as u64);

    // 5. LRU Block Cache Hot Read (1024 hits = 512 KiB)
    cache::read_sector(dev_id, dev, base_lba, &mut sec_read).ok()?;
    let cache_iters = 1024u64;
    let cache_kib = (cache_iters * SECTOR_SIZE as u64) / 1024;
    let t4 = time::uptime_micros();
    for _ in 0..cache_iters {
        cache::read_sector(dev_id, dev, base_lba, &mut sec_read).ok()?;
    }
    let cache_elapsed_us = time::uptime_micros().saturating_sub(t4).max(1);
    let cache_read_kib_s = (cache_kib * 1_000_000) / cache_elapsed_us;
    let cache_read_iops = (cache_iters * 1_000_000) / cache_elapsed_us;

    // 6. Hardware Flush Latency (16 iterations)
    let flush_iters = 16u64;
    let t5 = time::uptime_micros();
    for _ in 0..flush_iters {
        dev.flush().ok()?;
    }
    let flush_elapsed_us = time::uptime_micros().saturating_sub(t5).max(1);
    let flush_lat_us = flush_elapsed_us / flush_iters;

    // Zero the benchmark region and invalidate cache
    let zero_4k = [0u8; 4096];
    for i in 0..seq_io_count {
        let lba = base_lba + (i * seq_io_sectors) as u64;
        let _ = dev.write_blocks(lba, seq_io_sectors, &zero_4k);
    }
    let _ = dev.flush();
    cache::invalidate_device(dev_id);

    Some(DriverBenchResult {
        name: info.name,
        kind: info.kind.as_str(),
        model: info.model,
        seq_read_kib_s,
        seq_read_iops,
        seq_read_lat_us,
        seq_write_kib_s,
        seq_write_iops,
        seq_write_lat_us,
        rand_read_kib_s,
        rand_read_iops,
        rand_read_lat_us,
        rand_write_kib_s,
        rand_write_iops,
        rand_write_lat_us,
        cache_read_kib_s,
        cache_read_iops,
        flush_lat_us,
    })
}

/// Run the storage benchmark across all whole-disk devices and return results.
pub fn run_all() -> Vec<DriverBenchResult> {
    let mut results = Vec::new();
    for (dev_id, dev) in devices().iter().enumerate() {
        if let Some(res) = benchmark_device(dev_id, dev) {
            kinfo!(
                "bench [{} / {}]: 4K SeqRead={} KiB/s ({} IOPS, {} us) | 4K SeqWrite={} KiB/s ({} IOPS, {} us) | 512B Read={} IOPS ({} us) | 512B Write={} IOPS ({} us) | CacheRead={} KiB/s | Flush={} us",
                res.name,
                res.kind,
                res.seq_read_kib_s,
                res.seq_read_iops,
                res.seq_read_lat_us,
                res.seq_write_kib_s,
                res.seq_write_iops,
                res.seq_write_lat_us,
                res.rand_read_iops,
                res.rand_read_lat_us,
                res.rand_write_iops,
                res.rand_write_lat_us,
                res.cache_read_kib_s,
                res.flush_lat_us
            );
            results.push(res);
        }
    }
    results
}
