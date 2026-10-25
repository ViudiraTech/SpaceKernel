/*
 *       user/splibc/crt0.S
 *       Freestanding C runtime entry and signal restorer
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

.section .text
.global _start
_start:
    xor %ebp, %ebp
    mov (%rsp), %rdi          /* argc */
    lea 8(%rsp), %rsi         /* argv */
    lea 16(%rsp,%rdi,8), %rdx /* envp */
    mov %rdx, environ(%rip)
    and $-16, %rsp
    call main
    mov %eax, %edi
    call exit
1:
    jmp 1b

.global splibc_sigreturn
splibc_sigreturn:
    mov $15, %eax
    syscall
    ud2
