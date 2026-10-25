/*
 *       src/block/accounting.rs
 *       Logical block I/O completion statistics
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use crate::sync::SpinLock;
use alloc::collections::BTreeMap;
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub reads: u64,
    pub read_sectors: u64,
    pub read_us: u64,
    pub writes: u64,
    pub write_sectors: u64,
    pub write_us: u64,
    pub inflight: u64,
    pub busy_us: u64,
    pub weighted_us: u64,
}
static STATS: SpinLock<BTreeMap<usize, Stats>> = SpinLock::new(BTreeMap::new());
pub fn stats(id: usize) -> Stats {
    STATS.lock().get(&id).copied().unwrap_or_default()
}
pub struct Request {
    id: usize,
    write: bool,
    start: u64,
    bytes: usize,
}
impl Request {
    pub fn start(id: usize, write: bool) -> Self {
        STATS.lock().entry(id).or_default().inflight += 1;
        Self {
            id,
            write,
            start: crate::time::uptime_micros(),
            bytes: 0,
        }
    }
    pub fn complete(&mut self, bytes: usize) {
        self.bytes = bytes;
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        let elapsed = crate::time::uptime_micros().saturating_sub(self.start);
        let mut all = STATS.lock();
        let s = all.entry(self.id).or_default();
        s.inflight = s.inflight.saturating_sub(1);
        if self.bytes != 0 {
            if self.write {
                s.writes += 1;
                s.write_sectors += (self.bytes as u64).div_ceil(512);
                s.write_us += elapsed;
            } else {
                s.reads += 1;
                s.read_sectors += (self.bytes as u64).div_ceil(512);
                s.read_us += elapsed;
            }
        }
        s.busy_us += elapsed;
        s.weighted_us += elapsed;
    }
}
