/*
 *
 *       src/arch/aarch64/context.rs
 *       AArch64 task context and switch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! AArch64 task context structure and switching.
//!
//! Saves callee-saved registers: x19-x29, lr (x30), sp.

/// AArch64 task context (callee-saved registers + sp + lr)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // fp
    pub x30: u64, // lr
    pub sp: u64,
}

impl Context {
    /// Create a zeroed context.
    pub const fn new() -> Self {
        Self {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: 0,
            sp: 0,
        }
    }
}

/// Initialize a context for a new task.
///
/// # Safety
/// Stack must be valid and properly aligned (16-byte).
pub unsafe fn context_init(ctx: &mut Context, stack_top: usize, entry: usize) {
    ctx.sp = (stack_top & !15) as u64;
    ctx.x30 = entry as u64;
}

/// Switch from prev context to next context.
///
/// # Safety
/// IRQs are masked and both contexts/stacks remain live. The source
/// runqueue lock is released by finish_switch on the incoming stack.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn switch(_prev: *mut Context, _next: *const Context) {
    core::arch::naked_asm!(
        // Save callee-saved registers to prev
        "stp x19, x20, [x0, #0]",
        "stp x21, x22, [x0, #16]",
        "stp x23, x24, [x0, #32]",
        "stp x25, x26, [x0, #48]",
        "stp x27, x28, [x0, #64]",
        "stp x29, x30, [x0, #80]",
        "mov x9, sp",
        "str x9, [x0, #96]",
        // Load callee-saved registers from next
        "ldp x19, x20, [x1, #0]",
        "ldp x21, x22, [x1, #16]",
        "ldp x23, x24, [x1, #32]",
        "ldp x25, x26, [x1, #48]",
        "ldp x27, x28, [x1, #64]",
        "ldp x29, x30, [x1, #80]",
        "ldr x9, [x1, #96]",
        "mov sp, x9",
        "ret",
    )
}
