use core::sync::atomic::{AtomicU64, Ordering};

use crate::{arch, boot};

static FREQUENCY: AtomicU64 = AtomicU64::new(0);
static ORIGIN: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let frequency = boot::counter_frequency()
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
