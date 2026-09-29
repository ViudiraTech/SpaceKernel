use alloc::{boxed::Box, vec::Vec};
use core::arch::asm;

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
    // SAFETY: sscratch contains this hart's pinned stack state, while stvec
    // selects the immutable direct-mode trap entry.
    unsafe {
        asm!("csrw sscratch, {state}", "csrw stvec, {vector}", "fence.i",
            state = in(reg) state_ptr, vector = in(reg) vector,
            options(nostack, preserves_flags));
    }
}
