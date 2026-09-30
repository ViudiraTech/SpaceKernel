/*
 *
 *       src/arch/aarch64/serial.rs
 *       AArch64 early serial console access
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::sync::atomic::{AtomicUsize, Ordering};

pub const SERIAL_PHYS: u64 = 0x0900_0000;
static SERIAL: AtomicUsize = AtomicUsize::new(0);

pub fn serial_init(virtual_address: u64) {
    SERIAL.store(virtual_address as usize, Ordering::Release);
}

pub fn serial_write(byte: u8) {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: the VMM maps the QEMU virt PL011 page as device memory.
    unsafe {
        while ((base + 0x18) as *const u32).read_volatile() & (1 << 5) != 0 {
            core::hint::spin_loop();
        }
        (base as *mut u32).write_volatile(byte as u32);
    }
}

pub fn serial_try_read() -> Option<u8> {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    // SAFETY: the same PL011 device page used by serial_write is mapped.
    unsafe {
        (((base + 0x18) as *const u32).read_volatile() & (1 << 4) == 0)
            .then(|| (base as *const u32).read_volatile() as u8)
    }
}
