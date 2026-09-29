//! Ordered console delivery. The ring lock is dropped before each device write.

use super::ring;
use crate::{arch, sync::SpinLock, tty};
use core::{
    fmt::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

struct Drain {
    next: u64,
}
static DRAIN: SpinLock<Drain> = SpinLock::new(Drain { next: 0 });
static PENDING: AtomicBool = AtomicBool::new(false);

pub(super) fn mark_pending() {
    PENDING.store(true, Ordering::Release);
}

pub fn flush() {
    let Some(mut drain) = DRAIN.try_lock() else {
        return;
    };
    loop {
        PENDING.store(false, Ordering::Release);
        loop {
            let next = ring::next_for_drain(&mut drain.next);
            let Some(item) = next else { break };
            tty::write_kernel(&item.bytes[..item.length]);
            drain.next += 1;
        }
        if !PENDING.swap(false, Ordering::AcqRel) {
            break;
        }
    }
    drop(drain);
    if PENDING.load(Ordering::Acquire) {
        flush();
    }
}

pub fn emergency(arguments: fmt::Arguments<'_>) {
    struct Serial;
    impl Write for Serial {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            for byte in text.bytes() {
                arch::serial_write(byte);
            }
            Ok(())
        }
    }
    let _ = Serial.write_str("\r\n*** KERNEL EMERGENCY *** ");
    let _ = Serial.write_fmt(arguments);
    let _ = Serial.write_str("\r\n");
}
