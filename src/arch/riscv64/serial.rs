/*
 *
 *       src/arch/riscv64/serial.rs
 *       RISC-V early serial console access
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::sync::atomic::{AtomicUsize, Ordering};

pub const SERIAL_PHYS: u64 = 0x1000_0000;
static SERIAL: AtomicUsize = AtomicUsize::new(0);

pub fn serial_init(virtual_address: u64) {
    SERIAL.store(virtual_address as usize, Ordering::Release);
}

pub fn serial_write(byte: u8) {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: the VMM maps the QEMU virt 16550 UART as device memory.
    unsafe {
        while ((base + 5) as *const u8).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        (base as *mut u8).write_volatile(byte);
    }
}

pub fn serial_try_read() -> Option<u8> {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    // SAFETY: the same 16550 device page used by serial_write is mapped.
    unsafe {
        (((base + 5) as *const u8).read_volatile() & 1 != 0)
            .then(|| (base as *const u8).read_volatile())
    }
}

fn handle_serial_irq(_source: u32) {
    while let Some(byte) = serial_try_read() {
        crate::tty::receive(crate::tty::TtyDevice::Serial0, byte);
    }
}

pub fn serial_enable_irq() {
    let base = SERIAL.load(Ordering::Acquire);
    if base != 0 {
        // SAFETY: 16550 UART IER is at offset 1, MCR is at offset 4.
        unsafe {
            ((base + 1) as *mut u8).write_volatile(1);
            let mcr = ((base + 4) as *const u8).read_volatile();
            ((base + 4) as *mut u8).write_volatile(mcr | 0x0b);
        }
        let _ = super::plic::request(10, handle_serial_irq);
    }
}
