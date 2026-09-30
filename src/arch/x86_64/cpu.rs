/*
 *
 *       src/arch/x86_64/cpu.rs
 *       x86-64 interrupt state and architectural counter operations
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::asm;

#[inline]
pub fn disable_interrupts() {
    // SAFETY: cli changes only the local CPU interrupt flag.
    unsafe { asm!("cli", options(nomem, nostack)) }
}

pub fn enable_interrupts() {
    // SAFETY: the BSP enables delivery only after IDT and APIC setup.
    unsafe { asm!("sti", options(nomem, nostack)) }
}

#[inline]
pub fn irq_save() -> u64 {
    let flags: u64;
    // SAFETY: flags are captured and interrupts disabled on the same CPU.
    unsafe { asm!("pushfq", "pop {}", "cli", out(reg) flags, options(nomem)) }
    flags
}

#[inline]
pub fn irq_restore(flags: u64) {
    if flags & (1 << 9) != 0 {
        // SAFETY: restore IF only when it was enabled before the lock.
        unsafe { asm!("sti", options(nomem, nostack)) }
    }
}

#[inline]
pub fn halt() {
    // SAFETY: ring 0 halt with interrupts disabled.
    unsafe { asm!("hlt", options(nomem, nostack)) }
}

pub fn counter() -> u64 {
    // SAFETY: RDTSC has no memory side effects; Limine reports its frequency.
    unsafe { core::arch::x86_64::_rdtsc() }
}
