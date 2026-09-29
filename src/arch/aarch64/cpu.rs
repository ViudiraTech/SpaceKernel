use core::arch::asm;

pub fn disable_interrupts() {
    // SAFETY: masks interrupts on the local processing element.
    unsafe { asm!("msr daifset, #0xf", options(nomem, nostack)) }
}

pub fn irq_save() -> u64 {
    let flags: u64;
    // SAFETY: capture and mask interrupts on the same processing element.
    unsafe { asm!("mrs {}, daif", "msr daifset, #0xf", out(reg) flags, options(nomem, nostack)) }
    flags
}

pub fn irq_restore(flags: u64) {
    // SAFETY: flags were captured by irq_save on this processing element.
    unsafe { asm!("msr daif, {}", in(reg) flags, options(nomem, nostack)) }
}

pub fn halt() {
    // SAFETY: WFE is usable at the privilege level supplied by Limine.
    unsafe { asm!("wfe", options(nomem, nostack)) }
}

pub fn counter() -> u64 {
    let value: u64;
    // SAFETY: Limine runs the kernel with the architectural counter accessible.
    unsafe { asm!("mrs {}, cntpct_el0", out(reg) value, options(nomem, nostack)) }
    value
}
