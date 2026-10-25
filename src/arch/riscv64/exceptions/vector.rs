/*
 *
 *       src/arch/riscv64/exceptions/vector.rs
 *       RISC-V exception entry and interrupt dispatch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::{asm, global_asm};

use crate::{arch, printk};

global_asm!(include_str!("vector.S"));

unsafe extern "C" {
    static __spacekernel_riscv64_trap: u8;
}

pub(super) fn address() -> usize {
    core::ptr::addr_of!(__spacekernel_riscv64_trap) as usize
}

#[unsafe(no_mangle)]
extern "C" fn spacekernel_riscv64_irq(cause: u64) {
    let status: u64;
    // SAFETY: reads sstatus of the interrupted context before scheduling.
    unsafe {
        asm!("csrr {status}, sstatus", status = out(reg) status, options(nomem, nostack));
    }
    let user = (status & (1 << 8)) == 0;
    crate::fpu::disable_current();
    let irq_code = cause & !(1u64 << 63);
    match irq_code {
        // Supervisor external interrupt (PLIC)
        9 => {
            crate::arch::plic::handle_irq();
        }
        // Supervisor timer interrupt (CLINT / SBI / Sstc)
        5 => {
            crate::arch::clint::handle_timer_irq();
        }
        // Supervisor software interrupt (IPI)
        1 => {
            crate::arch::clint::handle_software_irq();
        }
        _ => {}
    }
    crate::sched::irq_exit();
    if user {
        // SAFETY: returning to U-mode; the task's FPU state is restored on switch.
        unsafe {
            let _ = crate::fpu::enable_user();
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn spacekernel_riscv64_exception(original_sp: u64, original_t0: u64) -> ! {
    let (cause, epc, tval, status): (u64, u64, u64, u64);
    // SAFETY: supervisor CSRs describe the currently entered trap.
    unsafe {
        asm!("csrr {cause}, scause", "csrr {epc}, sepc", "csrr {tval}, stval",
            "csrr {status}, sstatus", cause = out(reg) cause, epc = out(reg) epc,
            tval = out(reg) tval, status = out(reg) status, options(nomem, nostack));
    }
    arch::disable_interrupts();
    if status & (1 << 8) == 0 {
        crate::fpu::disable_current();
        crate::syscall::set_last_user_exit_code(139);
        crate::sched::exit(139);
    }
    printk::emergency(format_args!(
        "riscv64 trap cause={cause:#x} EPC={epc:#x} TVAL={tval:#x} STATUS={status:#x} SP={original_sp:#x} T0={original_t0:#x}"
    ));
    loop {
        arch::halt();
    }
}
