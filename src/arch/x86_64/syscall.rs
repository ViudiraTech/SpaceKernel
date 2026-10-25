/*
 *
 *       src/arch/x86_64/syscall.rs
 *       Fast syscall (LSTAR) handling and user privilege transition for x86-64
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Fast system call MSR initialization and assembly entry.

use core::arch::global_asm;

global_asm!(
    r#"
    .section .text
    .global spacekernel_x86_64_syscall_entry
spacekernel_x86_64_syscall_entry:
    swapgs
    mov gs:[16], rsp
    mov rsp, gs:[8]
    push qword ptr gs:[16]

    push r15
    push r14
    push r13
    push r12
    push rbp
    push rbx
    push r11
    push rcx

    push r9
    mov r9, r8
    mov r8, r10
    mov rcx, rdx
    mov rdx, rsi
    mov rsi, rdi
    mov rdi, rax

    call spacekernel_syscall_c

    add rsp, 8

    pop rcx
    pop r11
    pop rbx
    pop rbp
    pop r12
    pop r13
    pop r14
    pop r15

    pop rsp
    swapgs
    sysretq
"#
);

unsafe extern "C" {
    fn spacekernel_x86_64_syscall_entry();
}

pub fn init_syscall_msrs() {
    let (low, high): (u32, u32);
    // 1. Enable SCE (bit 0) in IA32_EFER (0xc000_0080)
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xc000_0080u32,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xc000_0080u32,
            in("eax") low | 1,
            in("edx") high,
            options(nostack)
        );
    }

    // 2. IA32_STAR (0xc000_0081):
    // Bits 47:32 = Kernel CS (0x08), Kernel SS (0x10)
    // Bits 63:48 = User Base (0x10) -> sysretq loads SS = 0x18 | 3 = 0x1b, CS = 0x20 | 3 = 0x23
    let star_high: u32 = (0x0010 << 16) | 0x0008;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xc000_0081u32,
            in("eax") 0u32,
            in("edx") star_high,
            options(nostack)
        );
    }

    // 3. IA32_LSTAR (0xc000_0082): Entry point RIP
    let entry_addr = spacekernel_x86_64_syscall_entry as *const () as usize as u64;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xc000_0082u32,
            in("eax") entry_addr as u32,
            in("edx") (entry_addr >> 32) as u32,
            options(nostack)
        );
    }

    // 4. IA32_FMASK (0xc000_0084): Mask flags during syscall
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xc000_0084u32,
            in("eax") 0x257fd5u32,
            in("edx") 0u32,
            options(nostack)
        );
    }
}

/// Enter user mode via IRETQ.
pub unsafe fn enter_user_mode(entry: u64, user_sp: u64) -> ! {
    const USER_DS: u64 = (3 << 3) | 3; // Selector 0x18 | 3 = 0x1b
    const USER_CS: u64 = (4 << 3) | 3; // Selector 0x20 | 3 = 0x23
    const RFLAGS_USER: u64 = 0x202; // IF enabled, bit 1 reserved

    unsafe {
        core::arch::asm!(
            "cli",
            "swapgs",
            "push {ss}",
            "push {rsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            "xor eax, eax",
            "xor ebx, ebx",
            "xor ecx, ecx",
            "xor edx, edx",
            "xor esi, esi",
            "xor edi, edi",
            "xor ebp, ebp",
            "xor r8d, r8d",
            "xor r9d, r9d",
            "xor r10d, r10d",
            "xor r11d, r11d",
            "xor r12d, r12d",
            "xor r13d, r13d",
            "xor r14d, r14d",
            "xor r15d, r15d",
            "iretq",
            ss = in(reg) USER_DS,
            rsp = in(reg) user_sp,
            rflags = in(reg) RFLAGS_USER,
            cs = in(reg) USER_CS,
            rip = in(reg) entry,
            options(noreturn)
        );
    }
}
