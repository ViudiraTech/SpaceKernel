/*
 *
 *       src/arch/riscv64/context.rs
 *       RISC-V 64 task context and switch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! RISC-V 64 task context structure and switching.
//!
//! Saves callee-saved registers: ra, sp, s0-s11.

/// RISC-V 64 task context (callee-saved registers + ra + sp)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub ra: u64,
    pub sp: u64,
    pub s0: u64,
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
}

impl Context {
    /// Create a zeroed context.
    pub const fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }
}

/// Initialize a context for a new task.
///
/// # Safety
/// Stack must be valid and properly aligned (16-byte).
pub unsafe fn context_init(ctx: &mut Context, stack_top: usize, entry: usize) {
    ctx.sp = (stack_top & !15) as u64;
    ctx.ra = entry as u64;
}

/// Switch from prev context to next context.
///
/// # Safety
/// IRQs are masked and both contexts/stacks remain live. The source
/// runqueue lock is released by finish_switch on the incoming stack.
#[unsafe(naked)]
pub unsafe extern "C" fn switch(_prev: *mut Context, _next: *const Context) {
    core::arch::naked_asm!(
        // Save callee-saved registers to prev
        "sd ra, 0(a0)",
        "sd sp, 8(a0)",
        "sd s0, 16(a0)",
        "sd s1, 24(a0)",
        "sd s2, 32(a0)",
        "sd s3, 40(a0)",
        "sd s4, 48(a0)",
        "sd s5, 56(a0)",
        "sd s6, 64(a0)",
        "sd s7, 72(a0)",
        "sd s8, 80(a0)",
        "sd s9, 88(a0)",
        "sd s10, 96(a0)",
        "sd s11, 104(a0)",
        // Load callee-saved registers from next
        "ld ra, 0(a1)",
        "ld sp, 8(a1)",
        "ld s0, 16(a1)",
        "ld s1, 24(a1)",
        "ld s2, 32(a1)",
        "ld s3, 40(a1)",
        "ld s4, 48(a1)",
        "ld s5, 56(a1)",
        "ld s6, 64(a1)",
        "ld s7, 72(a1)",
        "ld s8, 80(a1)",
        "ld s9, 88(a1)",
        "ld s10, 96(a1)",
        "ld s11, 104(a1)",
        "ret",
    )
}
