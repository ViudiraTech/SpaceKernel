/*
 *
 *       src/arch/aarch64/exceptions/vector.S
 *       AArch64 integer-register exception and interrupt vectors
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

    .section .text.spacekernel_vectors, "ax"
    .balign 2048
    .global __spacekernel_aarch64_vectors
__spacekernel_aarch64_vectors:
    .irp vector,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
        .if (\vector % 4) == 1
            b __spacekernel_aarch64_irq_entry
        .else
            mov x16, #\vector
            b __spacekernel_aarch64_exception_entry
        .endif
        .balign 128
    .endr

__spacekernel_aarch64_irq_entry:
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    mrs x0, elr_el1
    str x0, [sp, #248]
    mrs x0, spsr_el1
    str x0, [sp, #256]
    bl spacekernel_aarch64_irq
    ldr x0, [sp, #248]
    msr elr_el1, x0
    ldr x0, [sp, #256]
    msr spsr_el1, x0
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #272
    eret

__spacekernel_aarch64_exception_entry:
    mov x1, sp
    mrs x17, tpidr_el1
    ldr x17, [x17]
    mov sp, x17
    mov x0, x16
    bl spacekernel_aarch64_exception
