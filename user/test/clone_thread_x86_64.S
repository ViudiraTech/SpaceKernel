/*
 *       user/test/clone_thread_x86_64.S
 *       Raw clone wrapper that starts the child on a fresh stack
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

.section .text
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

