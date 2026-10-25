/*
 *       user/test/crt0_x86_64.S
 *       Freestanding x86_64 test entry and signal restorer
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

.section .text
.global _start
_start:
    xor %ebp, %ebp
    and $-16, %rsp
    call main
    mov %eax, %edi
    mov $231, %eax
    syscall
1:
    jmp 1b

.global clone_thread
.clone_thread_body:
clone_thread:
    /* clone(flags, stack, parent_tid, child_tid, tls) */
    mov %rdx, %r10
    xor %edx, %edx
    xor %r8d, %r8d
    mov $56, %eax
    syscall
    test %rax, %rax
    jz 2f
    ret
2:
    and $-16, %rsp
    call thread_entry
    mov $60, %eax
    mov $5, %edi
    syscall
3:
    jmp 3b

.global signal_restorer
signal_restorer:
    mov $15, %eax
    syscall
    ud2
