/*
 *
 *       src/arch/aarch64/exceptions/vector.rs
 *       AArch64 exception entry and interrupt dispatch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::{asm, global_asm};

use crate::{arch, printk};

global_asm!(include_str!("vector.S"));

#[unsafe(no_mangle)]
extern "C" fn spacekernel_aarch64_irq() {
    crate::arch::gic::handle_irq();
    crate::sched::irq_exit();
}

unsafe extern "C" {
    static __spacekernel_aarch64_vectors: u8;
}

pub(super) fn address() -> usize {
    core::ptr::addr_of!(__spacekernel_aarch64_vectors) as usize
}

#[unsafe(no_mangle)]
extern "C" fn spacekernel_aarch64_exception(vector: u64, original_sp: u64) -> ! {
    let (esr, elr, far, spsr): (u64, u64, u64, u64);
    // SAFETY: these registers describe the exception currently handled at EL1.
    unsafe {
        asm!("mrs {esr}, esr_el1", "mrs {elr}, elr_el1", "mrs {far}, far_el1",
            "mrs {spsr}, spsr_el1", esr = out(reg) esr, elr = out(reg) elr,
            far = out(reg) far, spsr = out(reg) spsr, options(nomem, nostack));
    }
    arch::disable_interrupts();
    printk::emergency(format_args!(
        "aarch64 exception vector={vector} ESR={esr:#x} ELR={elr:#x} FAR={far:#x} SPSR={spsr:#x} SP={original_sp:#x}"
    ));
    loop {
        arch::halt();
    }
}
