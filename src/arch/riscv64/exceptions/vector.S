    .section .text.spacekernel_traps, "ax"
    .balign 4
    .global __spacekernel_riscv64_trap
__spacekernel_riscv64_trap:
    csrrw t0, sscratch, t0
    beqz t0, 1f
    ld t1, 0(t0)
    mv a0, sp
    csrr a1, sscratch
    mv sp, t1
    call spacekernel_riscv64_exception
1:
    wfi
    j 1b
