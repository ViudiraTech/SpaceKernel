/*
 *
 *       src/arch/x86_64/serial.rs
 *       x86-64 early serial console access
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::asm;

const COM1: u16 = 0x3f8;

pub fn serial_init(_unused: u64) {
    out(COM1 + 1, 0);
    out(COM1 + 3, 0x80);
    out(COM1, 1);
    out(COM1 + 1, 0);
    out(COM1 + 3, 3);
    out(COM1 + 2, 0xc7);
    out(COM1 + 4, 3);
}

pub fn serial_write(byte: u8) {
    while input(COM1 + 5) & 0x20 == 0 {
        core::hint::spin_loop();
    }
    out(COM1, byte);
}

pub fn serial_try_read() -> Option<u8> {
    (input(COM1 + 5) & 1 != 0).then(|| input(COM1))
}

fn out(port: u16, byte: u8) {
    // SAFETY: port I/O is supported for the QEMU PC 16550 UART.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") byte, options(nomem, nostack, preserves_flags))
    }
}

fn input(port: u16) -> u8 {
    let byte: u8;
    // SAFETY: port I/O is supported for the QEMU PC 16550 UART.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") byte, options(nomem, nostack, preserves_flags))
    }
    byte
}
