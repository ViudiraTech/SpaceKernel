/*
 *
 *       src/time.rs
 *       Counter frequency discovery and kernel uptime
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::sync::atomic::{AtomicU64, Ordering};

use crate::{arch, boot};

static FREQUENCY: AtomicU64 = AtomicU64::new(0);
static ORIGIN: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let frequency = boot::counter_frequency()
        .or_else(|| crate::cpuid::info().and_then(|cpu| cpu.counter_hz))
        .filter(|value| *value != 0)
        .unwrap_or(0);
    ORIGIN.store(arch::counter(), Ordering::Release);
    FREQUENCY.store(frequency, Ordering::Release);
}

pub fn uptime_micros() -> u64 {
    let frequency = FREQUENCY.load(Ordering::Acquire);
    if frequency == 0 {
        return 0;
    }
    let ticks = arch::counter().wrapping_sub(ORIGIN.load(Ordering::Acquire));
    ((ticks as u128 * 1_000_000) / frequency as u128).min(u64::MAX as u128) as u64
}

pub fn counter_frequency() -> Option<u64> {
    let value = FREQUENCY.load(Ordering::Acquire);
    (value != 0).then_some(value)
}
