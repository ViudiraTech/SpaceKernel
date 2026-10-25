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

.global signal_restorer
signal_restorer:
    mov $15, %eax
    syscall
    ud2
