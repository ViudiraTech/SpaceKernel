use core::arch::asm;

#[inline]
pub fn disable_interrupts() {
    // SAFETY: cli changes only the local CPU interrupt flag.
    unsafe { asm!("cli", options(nomem, nostack)) }
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
