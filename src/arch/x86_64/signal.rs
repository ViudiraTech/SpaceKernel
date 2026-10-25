/*
 *
 *       src/arch/x86_64/signal.rs
 *       Full register frames and the timer IRQ stub used for signals
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Syscall and interrupt frames that preserve every user general register.
//! Signal delivery rewrites these frames so a handler can be entered without a
//! separate kernel-to-user transition, and `rt_sigreturn` can restore them.

use core::arch::global_asm;

/// General registers as pushed by the entry stubs.  The order matches the
/// push sequence in `spacekernel_x86_64_syscall_entry` and the timer stub.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GprFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub rcx: u64,
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
}

/// Frame at `gs:[24]` during a syscall.  `rcx`/`r11` hold the user return RIP
/// and RFLAGS that `sysretq` consumes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SyscallFrame {
    pub gpr: GprFrame,
    pub rsp: u64,
}

/// Frame at the top of the kernel stack during a hardware interrupt.  `rsp` and
/// `ss` are only present when the interrupted context was ring 3.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InterruptFrame {
    pub gpr: GprFrame,
    pub rip: u64,
    pub cs: u64,
    pub flags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl SyscallFrame {
    pub fn user_rip(&self) -> u64 {
        self.gpr.rcx
    }

    pub fn set_user_rip(&mut self, rip: u64) {
        self.gpr.rcx = rip;
    }

    pub fn user_flags(&self) -> u64 {
        self.gpr.r11
    }

    pub fn set_user_flags(&mut self, flags: u64) {
        self.gpr.r11 = flags;
    }
}

impl InterruptFrame {
    pub fn from_user(&self) -> bool {
        self.cs & 3 == 3
    }
}

// The timer vector gets a dedicated stub because the generic
// `extern "x86-interrupt"` handlers do not expose the general registers that a
// signal frame must capture.
global_asm!(
    r#"
    .section .text
    .global spacekernel_x86_64_timer_stub
spacekernel_x86_64_timer_stub:
    cld                       /* Rust/LLVM assume DF=0; IRETQ restores it */
    push rax
    push rdi
    push rsi
    push rdx
    push r10
    push r8
    push r9
    push rcx
    push r11
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov rbp, rsp
    mov rax, [rsp + 128]      /* hardware CS */
    test al, 3
    jz 1f
    swapgs
1:
    and rsp, -16
    mov rdi, rbp
    call spacekernel_timer_irq_c
    mov rsp, rbp
    mov rax, [rsp + 128]
    test al, 3
    jz 2f
    swapgs
2:
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    pop r11
    pop rcx
    pop r9
    pop r8
    pop r10
    pop rdx
    pop rsi
    pop rdi
    pop rax
    iretq
"#
);

unsafe extern "C" {
    pub fn spacekernel_x86_64_timer_stub();
}

/// Timer vector handler with full user register capture.  Called by the naked
/// stub above with the interrupt frame pointer in RDI.
#[unsafe(no_mangle)]
pub extern "C" fn spacekernel_timer_irq_c(frame: *mut InterruptFrame) {
    // SAFETY: the naked stub passes its live interrupt frame.
    let frame = unsafe { &mut *frame };
    let user = frame.from_user();
    crate::fpu::disable_current();
    crate::arch::apic::dispatch(crate::arch::timer_vector());
    crate::sched::irq_exit();
    if user {
        crate::signal::deliver_on_interrupt(frame);
        // SAFETY: this CPU owns the task, IRQs are masked, and this is the
        // final user-return boundary.
        unsafe {
            let _ = crate::fpu::enable_user();
        }
    }
}
