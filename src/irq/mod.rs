/*
 *
 *       src/irq/mod.rs
 *       Architecture independent synchronous interrupt dispatch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Architecture independent synchronous interrupt dispatch.
//!
//! Controller drivers own routing and acknowledgement. A handler is called with
//! interrupts masked on the current CPU and must not sleep.

use crate::sync::SpinLock;
use core::sync::atomic::{AtomicU64, Ordering};

pub type Handler = fn(u32);
pub const MAX_IRQS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqError {
    Invalid,
    InUse,
    NotRegistered,
    Unsupported,
    NoController,
    NoVector,
    Timeout,
    Mapping,
}

static HANDLERS: SpinLock<[Option<Handler>; MAX_IRQS]> = SpinLock::new([None; MAX_IRQS]);
static COUNTS: [AtomicU64; MAX_IRQS] = [const { AtomicU64::new(0) }; MAX_IRQS];
static UNHANDLED: AtomicU64 = AtomicU64::new(0);

pub fn register(irq: u32, handler: Handler) -> Result<(), IrqError> {
    let mut handlers = HANDLERS.lock();
    let slot = handlers.get_mut(irq as usize).ok_or(IrqError::Invalid)?;
    if slot.is_some() {
        return Err(IrqError::InUse);
    }
    *slot = Some(handler);
    Ok(())
}

/// The caller must mask the source and synchronize any in-flight handler first.
pub fn unregister(irq: u32) -> Result<(), IrqError> {
    HANDLERS
        .lock()
        .get_mut(irq as usize)
        .ok_or(IrqError::Invalid)?
        .take()
        .map(|_| ())
        .ok_or(IrqError::NotRegistered)
}

pub fn dispatch(irq: u32) {
    let handler = HANDLERS.lock().get(irq as usize).copied().flatten();
    if let Some(handler) = handler {
        COUNTS[irq as usize].fetch_add(1, Ordering::Relaxed);
        handler(irq);
    } else {
        UNHANDLED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn count(irq: u32) -> Option<u64> {
    COUNTS.get(irq as usize).map(|n| n.load(Ordering::Relaxed))
}

pub fn unhandled_count() -> u64 {
    UNHANDLED.load(Ordering::Relaxed)
}
