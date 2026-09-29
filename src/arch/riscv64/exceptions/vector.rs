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
    printk::emergency(format_args!(
        "riscv64 trap cause={cause:#x} EPC={epc:#x} TVAL={tval:#x} STATUS={status:#x} SP={original_sp:#x} T0={original_t0:#x}"
    ));
    loop {
        arch::halt();
    }
}
