/*
 *
 *       src/arch/riscv64/exceptions/per_cpu.rs
 *       RISC-V CPU-local exception state and emergency stack ownership
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use alloc::{boxed::Box, vec::Vec};
use core::{
    arch::asm,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{boot, sync::SpinLock};

use super::vector;

const STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

#[repr(C)]
struct HartState {
    // vector.S reads the top of this stack at offset zero.
    stack_top: usize,
    _stack: Box<Stack>,
}

static POINTERS: [AtomicUsize; 256] = [const { AtomicUsize::new(0) }; 256];

#[allow(
    clippy::vec_box,
    reason = "CPU-local addresses remain pinned independently of Vec movement or growth"
)]
static STATES: SpinLock<Vec<Box<HartState>>> = SpinLock::new(Vec::new());

pub fn init_bsp() -> usize {
    let count = boot::cpu_count();
    assert!((1..=256).contains(&count), "invalid Limine CPU count");
    let mut prepared = Vec::with_capacity(count);
    for _ in 0..count {
        let stack = Box::new(Stack([0; STACK_SIZE]));
        let stack_top = stack.0.as_ptr().wrapping_add(STACK_SIZE) as usize;
        prepared.push(Box::new(HartState {
            stack_top,
            _stack: stack,
        }));
    }
    let mut states = STATES.lock();
    assert!(states.is_empty(), "trap state initialized twice");
    *states = prepared;
    drop(states);
    load_cpu(boot::bsp_cpu_index());
    count
}

/// AP startup calls this before enabling supervisor interrupts on the hart.
pub fn load_cpu(index: usize) {
    let states = STATES.lock();
    let state = states.get(index).expect("unknown hart index");
    let state_ptr = &**state as *const HartState as usize;
    let vector = vector::address();
    POINTERS[index].store(state_ptr, Ordering::Release);
    // SAFETY: sscratch contains this hart's pinned stack state, while stvec
    // selects the immutable direct-mode trap entry.
    unsafe {
        asm!("csrw sscratch, {state}", "csrw stvec, {vector}", "fence.i",
            state = in(reg) state_ptr, vector = in(reg) vector,
            options(nostack, preserves_flags));
    }
}

/// Resolve the installed hart-local pointer without dereferencing an arbitrary
/// boot or uninitialized sscratch value.
pub fn current_cpu_index() -> Option<usize> {
    let state: usize;
    // SAFETY: reading sscratch does not require the pointer to be initialized.
    unsafe {
        asm!("csrr {}, sscratch", out(reg) state, options(nomem,nostack));
    }
    if state == 0 {
        return None;
    }
    POINTERS
        .iter()
        .position(|pointer| pointer.load(Ordering::Acquire) == state)
}
