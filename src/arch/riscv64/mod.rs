/*
 *
 *       src/arch/riscv64/mod.rs
 *       RISC-V architecture backend interfaces
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

pub mod clint;
mod context;
mod scheduler;
pub(crate) use scheduler::*;
mod cpu;
pub mod cpuid;
mod exceptions;
pub mod fpu;
mod paging;
pub mod plic;
mod serial;

pub use context::{Context, context_init, switch};
pub use cpu::{counter, disable_interrupts, enable_interrupts, halt, irq_restore, irq_save};
pub(crate) use exceptions::current_cpu_index;
pub use exceptions::{init_bsp as init_exceptions, load_cpu as load_exceptions};
pub use paging::{
    flush_page, flush_table, page_root, paging_geometry, pte_is_table, pte_leaf, pte_phys,
    pte_present, pte_table, set_page_root, tlb_shootdown,
};
pub use serial::{SERIAL_PHYS, serial_enable_irq, serial_init, serial_try_read, serial_write};

pub fn set_kernel_stack(cpu: usize, stack_top: u64) {
    exceptions::set_kernel_stack(cpu, stack_top);
}

pub unsafe fn enter_user_mode(entry: u64, user_sp: u64) -> ! {
    unsafe {
        let mut sstatus: u64;
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus, options(nomem, nostack));
        sstatus &= !(1 << 8); // SPP = 0 (U-mode)
        sstatus |= 1 << 5; // SPIE = 1 (enable interrupts in U-mode)
        sstatus |= 1 << 18; // SUM = 1 (permit S-mode to access user memory)
        core::arch::asm!(
            "csrc sstatus, 2",
            "csrw sstatus, {status}",
            "csrw sepc, {entry}",
            "mv sp, {sp}",
            "sret",
            status = in(reg) sstatus,
            entry = in(reg) entry,
            sp = in(reg) user_sp,
            options(noreturn)
        );
    }
}
