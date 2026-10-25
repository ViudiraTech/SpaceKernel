/*
 *
 *       src/user/process.rs
 *       User process creation, address space binding and lifecycle execution
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! User process creation and execution.

use crate::{
    arch, kinfo, sched,
    sync::SpinLock,
    user::{address_space::UserAddressSpace, elf, stack},
};
use alloc::sync::Arc;

pub unsafe fn install_tls(tp: u64) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xc000_0100u32,
            in("eax") (tp as u32),
            in("edx") ((tp >> 32) as u32),
            options(nostack)
        );
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("msr tpidr_el0, {}", in(reg) tp, options(nostack));
    }
    #[cfg(target_arch = "riscv64")]
    unsafe {
        core::arch::asm!("mv tp, {}", in(reg) tp, options(nostack));
    }
}

pub fn spawn_user_elf(data: &[u8], name: &str) -> Result<u64, &'static str> {
    let mut space = UserAddressSpace::new()?;
    let loaded = elf::load_elf(&mut space, data)?;
    let user_sp = stack::setup_user_stack(&mut space, &loaded, name)?;
    let space_arc = Arc::new(SpinLock::new(space));

    let space_clone = space_arc.clone();
    let entry_va = loaded.initial_pc;
    let tls_tp = loaded.tls_tp;

    // Spawn a new task in the scheduler
    let tid = sched::spawn(
        move || {
            // Bind address space and configure kernel stack for current task
            if let Some(current_task) = sched::current() {
                let mut task = current_task.lock();
                task.set_address_space(space_clone.clone());
                if let Some(stack_top) = task.kernel_stack_top() {
                    arch::set_kernel_stack(crate::smp::current_cpu(), stack_top as u64);
                }
            }

            // Activate user address space
            space_clone.lock().activate();

            kinfo!(
                "Entering user mode: entry={:#x} sp={:#x}",
                entry_va,
                user_sp
            );

            // Drop to unprivileged user mode!
            unsafe {
                install_tls(tls_tp);
                let _ = crate::fpu::enable_user();
                arch::enter_user_mode(entry_va, user_sp);
            }
        },
        0,
    )
    .map_err(|_| "failed to spawn user task")?;

    Ok(tid)
}
