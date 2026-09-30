/*
 *
 *       src/arch/aarch64/exceptions/per_cpu.rs
 *       AArch64 CPU-local exception state and emergency stack ownership
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use alloc::{boxed::Box, vec::Vec};
use core::arch::asm;

use crate::{boot, sync::SpinLock};

use super::vector;

const STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

#[repr(C)]
struct CpuState {
    // vector.S reads the top of this stack at offset zero.
    stack_top: usize,
    _stack: Box<Stack>,
}

#[allow(
    clippy::vec_box,
    reason = "CPU-local addresses remain pinned independently of Vec movement or growth"
)]
static STATES: SpinLock<Vec<Box<CpuState>>> = SpinLock::new(Vec::new());

pub fn init_bsp() -> usize {
    let count = boot::cpu_count();
    assert!((1..=256).contains(&count), "invalid Limine CPU count");
    let mut prepared = Vec::with_capacity(count);
    for _ in 0..count {
        let stack = Box::new(Stack([0; STACK_SIZE]));
        let stack_top = stack.0.as_ptr().wrapping_add(STACK_SIZE) as usize;
        prepared.push(Box::new(CpuState {
            stack_top,
            _stack: stack,
        }));
    }
    let mut states = STATES.lock();
    assert!(states.is_empty(), "exception state initialized twice");
    *states = prepared;
    drop(states);
    load_cpu(boot::bsp_cpu_index());
    count
}

/// AP startup calls this before unmasking any interrupt on that core.
pub fn load_cpu(index: usize) {
    let states = STATES.lock();
    let state = states.get(index).expect("unknown CPU index");
    let state_ptr = &**state as *const CpuState as usize;
    let vectors = vector::address();
    // SAFETY: the vector table is 2048-byte aligned and immutable; the
    // CPU-local state and stack remain pinned for the kernel lifetime.
    unsafe {
        // Limine may enter EL1t using SP_EL0. IRQ entry uses SP_EL1; move the
        // active virtual stack to EL1h so it cannot use a stale firmware stack.
        asm!("mov {stack}, sp", "msr spsel, #1", "mov sp, {stack}", "isb",
            stack = out(reg) _, options(preserves_flags));
        asm!("msr tpidr_el1, {state}", "msr vbar_el1, {vectors}", "isb",
            state = in(reg) state_ptr, vectors = in(reg) vectors,
            options(nostack, preserves_flags));
    }
}
