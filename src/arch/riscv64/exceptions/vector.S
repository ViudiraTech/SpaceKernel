    .section .text.spacekernel_traps, "ax"
    .balign 4
    .global __spacekernel_riscv64_trap
__spacekernel_riscv64_trap:
    addi sp, sp, -256
    sd ra, 0(sp)
    sd t0, 8(sp)
    sd t1, 16(sp)
    sd t2, 24(sp)
    sd a0, 32(sp)
    sd a1, 40(sp)
    sd a2, 48(sp)
    sd a3, 56(sp)
    sd a4, 64(sp)
    sd a5, 72(sp)
    sd a6, 80(sp)
    sd a7, 88(sp)
    sd t3, 96(sp)
    sd t4, 104(sp)
    sd t5, 112(sp)
    sd t6, 120(sp)
    sd s0, 128(sp)
    sd s1, 136(sp)
    sd s2, 144(sp)
    sd s3, 152(sp)
    sd s4, 160(sp)
    sd s5, 168(sp)
    sd s6, 176(sp)
    sd s7, 184(sp)
    sd s8, 192(sp)
    sd s9, 200(sp)
    sd s10, 208(sp)
    sd s11, 216(sp)
    sd gp, 224(sp)
    sd tp, 232(sp)
    csrr t0, sepc
    sd t0, 240(sp)
    csrr t1, sstatus
    sd t1, 248(sp)

    csrr a0, scause
    bltz a0, 1f

    // Exception branch: switch to panic stack
    csrr t0, sscratch
    beqz t0, 2f
    ld t1, 0(t0)
    addi a0, sp, 256
    ld a1, 8(sp)
    mv sp, t1
    call spacekernel_riscv64_exception
2:
    wfi
    j 2b

1:
    // Interrupt branch: call spacekernel_riscv64_irq
    call spacekernel_riscv64_irq

    ld t0, 240(sp)
    csrw sepc, t0
    ld t1, 248(sp)
    csrw sstatus, t1

    ld ra, 0(sp)
    ld t0, 8(sp)
    ld t1, 16(sp)
    ld t2, 24(sp)
    ld a0, 32(sp)
    ld a1, 40(sp)
    ld a2, 48(sp)
    ld a3, 56(sp)
    ld a4, 64(sp)
    ld a5, 72(sp)
    ld a6, 80(sp)
    ld a7, 88(sp)
    ld t3, 96(sp)
    ld t4, 104(sp)
    ld t5, 112(sp)
    ld t6, 120(sp)
    ld s0, 128(sp)
    ld s1, 136(sp)
    ld s2, 144(sp)
    ld s3, 152(sp)
    ld s4, 160(sp)
    ld s5, 168(sp)
    ld s6, 176(sp)
    ld s7, 184(sp)
    ld s8, 192(sp)
    ld s9, 200(sp)
    ld s10, 208(sp)
    ld s11, 216(sp)
    ld gp, 224(sp)
    ld tp, 232(sp)
    addi sp, sp, 256
    sret
