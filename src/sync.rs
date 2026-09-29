use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::arch;

/// IRQ-safe spin lock. Never sleep or allocate while it is held.
///
/// Lock order: virtual memory -> physical memory. A physical-memory holder
/// must not acquire any virtual-memory or slab lock. Slab -> physical memory
/// is allowed. Logging has its own terminal lock and must never call an
/// allocator while holding it. PCI config locks may enter VMM -> PMM; no code
/// may acquire a PCI config lock while holding VMM, PMM, or the slab locks.
pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
    flags: u64,
    // Restoring interrupt state on another CPU would corrupt that CPU's state.
    _not_send: PhantomData<*mut ()>,
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        let flags = arch::irq_save();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Guard {
            lock: self,
            flags,
            _not_send: PhantomData,
        }
    }

    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        let flags = arch::irq_save();
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(Guard {
                lock: self,
                flags,
                _not_send: PhantomData,
            })
        } else {
            arch::irq_restore(flags);
            None
        }
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the guard is the sole owner while the lock is held.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard is the sole owner while the lock is held.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        arch::irq_restore(self.flags);
    }
}
