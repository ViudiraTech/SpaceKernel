//! Bounded allocation-free log storage. Device I/O never runs under this lock.

use super::{Level, drain};
use crate::{sync::SpinLock, time};
use core::fmt::{self, Write};

const CAPACITY: usize = 128;
const MAX_RECORD: usize = 512;

#[derive(Clone, Copy)]
pub struct Record {
    pub sequence: u64,
    pub timestamp_us: u64,
    pub level: Level,
    pub length: usize,
    pub truncated: bool,
    pub bytes: [u8; MAX_RECORD],
}

impl Record {
    const fn empty() -> Self {
        Self {
            sequence: 0,
            timestamp_us: 0,
            level: Level::Info,
            length: 0,
            truncated: false,
            bytes: [0; MAX_RECORD],
        }
    }
}

struct Ring {
    records: [Record; CAPACITY],
    next: u64,
}

impl Ring {
    const fn new() -> Self {
        Self {
            records: [Record::empty(); CAPACITY],
            next: 0,
        }
    }

    fn get(&self, sequence: u64) -> Option<Record> {
        let oldest = self.next.saturating_sub(CAPACITY as u64);
        if sequence < oldest || sequence >= self.next {
            return None;
        }
        Some(self.records[sequence as usize % CAPACITY])
    }
}

static RING: SpinLock<Ring> = SpinLock::new(Ring::new());

struct Buffer<'a> {
    bytes: &'a mut [u8],
    len: usize,
    truncated: bool,
}

impl Write for Buffer<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let available = self.bytes.len() - self.len;
        let count = text.len().min(available);
        // Retain a valid UTF-8 prefix when a formatted message is truncated.
        let count = (0..=count)
            .rev()
            .find(|&n| text.is_char_boundary(n))
            .unwrap_or(0);
        self.bytes[self.len..self.len + count].copy_from_slice(&text.as_bytes()[..count]);
        self.len += count;
        self.truncated |= count < text.len();
        Ok(())
    }
}

/// `printk` never allocates. The ring lock protects only bounded memory
/// copies; device output happens later under the independent console lock.
pub fn record(level: Level, arguments: fmt::Arguments<'_>) {
    let now = time::uptime_micros();
    let mut item = Record::empty();
    item.level = level;
    item.timestamp_us = now;
    let mut writer = Buffer {
        bytes: &mut item.bytes,
        len: 0,
        truncated: false,
    };
    let _ = write!(
        writer,
        "[{sec:>5}.{mic:06}] [{label}] ",
        sec = now / 1_000_000,
        mic = now % 1_000_000,
        label = level.label()
    );
    let _ = writer.write_fmt(arguments);
    let _ = writer.write_str("\n");
    item.length = writer.len;
    item.truncated = writer.truncated;

    // A same-CPU NMI can interrupt a ring holder. Bounded retries avoid
    // deadlock; the emergency serial path keeps the diagnostic visible.
    let mut guard = None;
    for _ in 0..1024 {
        guard = RING.try_lock();
        if guard.is_some() {
            break;
        }
        core::hint::spin_loop();
    }
    let Some(mut ring) = guard else {
        drain::emergency(format_args!("printk ring busy: {}", level.label()));
        return;
    };
    item.sequence = ring.next;
    let slot = ring.next as usize % CAPACITY;
    ring.records[slot] = item;
    ring.next = ring.next.wrapping_add(1);
    drop(ring);
    drain::mark_pending();
    drain::flush();
}

/// Copy records for a future /dev/kmsg reader without exposing ring storage.
pub fn read(sequence: u64) -> Option<Record> {
    RING.lock().get(sequence)
}

pub fn next_sequence() -> u64 {
    RING.lock().next
}

pub(super) fn next_for_drain(next: &mut u64) -> Option<Record> {
    let ring = RING.lock();
    *next = (*next).max(ring.next.saturating_sub(CAPACITY as u64));
    ring.get(*next)
}
