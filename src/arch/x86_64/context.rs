/*
 *
 *       src/arch/x86_64/context.rs
 *       x86-64 callee-saved context and aligned first task entry
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Context {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    rsp: u64,
}
impl Context {
    pub const fn new() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rsp: 0,
        }
    }
}
/// # Safety
/// Stack must be writable, exclusively owned, and at least 16 bytes long.
pub unsafe fn context_init(ctx: &mut Context, stack_top: usize, entry: usize) {
    let sp = (stack_top & !15) - 16;
    // SAFETY: the caller owns the stack; RET enters with RSP % 16 == 8.
    unsafe {
        (sp as *mut usize).write(entry);
    }
    ctx.rsp = sp as u64;
}
/// # Safety
/// IRQs are masked. Contexts and stacks remain live. The source runqueue
/// stays locked until finish_switch runs on the destination stack.
#[unsafe(naked)]
pub unsafe extern "C" fn switch(_previous: *mut Context, _next: *const Context) {
    core::arch::naked_asm!(
        "mov [rdi], r15",
        "mov [rdi + 8], r14",
        "mov [rdi + 16], r13",
        "mov [rdi + 24], r12",
        "mov [rdi + 32], rbx",
        "mov [rdi + 40], rbp",
        "mov [rdi + 48], rsp",
        "mov r15, [rsi]",
        "mov r14, [rsi + 8]",
        "mov r13, [rsi + 16]",
        "mov r12, [rsi + 24]",
        "mov rbx, [rsi + 32]",
        "mov rbp, [rsi + 40]",
        "mov rsp, [rsi + 48]",
        "ret",
    );
}
