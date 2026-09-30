/*
 *
 *       src/arch/riscv64/fpu.S
 *       RISC-V explicit extended-register save and restore routines
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

    .option push
    .option arch, +f, +d
    .section .text.spacekernel_fpu, "ax"
    .balign 4
    .global spacekernel_riscv_f_save
spacekernel_riscv_f_save:
    fsw f0, 0(a0)
    fsw f1, 8(a0)
    fsw f2, 16(a0)
    fsw f3, 24(a0)
    fsw f4, 32(a0)
    fsw f5, 40(a0)
    fsw f6, 48(a0)
    fsw f7, 56(a0)
    fsw f8, 64(a0)
    fsw f9, 72(a0)
    fsw f10, 80(a0)
    fsw f11, 88(a0)
    fsw f12, 96(a0)
    fsw f13, 104(a0)
    fsw f14, 112(a0)
    fsw f15, 120(a0)
    fsw f16, 128(a0)
    fsw f17, 136(a0)
    fsw f18, 144(a0)
    fsw f19, 152(a0)
    fsw f20, 160(a0)
    fsw f21, 168(a0)
    fsw f22, 176(a0)
    fsw f23, 184(a0)
    fsw f24, 192(a0)
    fsw f25, 200(a0)
    fsw f26, 208(a0)
    fsw f27, 216(a0)
    fsw f28, 224(a0)
    fsw f29, 232(a0)
    fsw f30, 240(a0)
    fsw f31, 248(a0)
    frcsr t0
    sw t0, 256(a0)
    ret

    .global spacekernel_riscv_f_restore
spacekernel_riscv_f_restore:
    flw f0, 0(a0)
    flw f1, 8(a0)
    flw f2, 16(a0)
    flw f3, 24(a0)
    flw f4, 32(a0)
    flw f5, 40(a0)
    flw f6, 48(a0)
    flw f7, 56(a0)
    flw f8, 64(a0)
    flw f9, 72(a0)
    flw f10, 80(a0)
    flw f11, 88(a0)
    flw f12, 96(a0)
    flw f13, 104(a0)
    flw f14, 112(a0)
    flw f15, 120(a0)
    flw f16, 128(a0)
    flw f17, 136(a0)
    flw f18, 144(a0)
    flw f19, 152(a0)
    flw f20, 160(a0)
    flw f21, 168(a0)
    flw f22, 176(a0)
    flw f23, 184(a0)
    flw f24, 192(a0)
    flw f25, 200(a0)
    flw f26, 208(a0)
    flw f27, 216(a0)
    flw f28, 224(a0)
    flw f29, 232(a0)
    flw f30, 240(a0)
    flw f31, 248(a0)
    lw t0, 256(a0)
    fscsr t0
    ret

    .global spacekernel_riscv_d_save
spacekernel_riscv_d_save:
    fsd f0, 0(a0)
    fsd f1, 8(a0)
    fsd f2, 16(a0)
    fsd f3, 24(a0)
    fsd f4, 32(a0)
    fsd f5, 40(a0)
    fsd f6, 48(a0)
    fsd f7, 56(a0)
    fsd f8, 64(a0)
    fsd f9, 72(a0)
    fsd f10, 80(a0)
    fsd f11, 88(a0)
    fsd f12, 96(a0)
    fsd f13, 104(a0)
    fsd f14, 112(a0)
    fsd f15, 120(a0)
    fsd f16, 128(a0)
    fsd f17, 136(a0)
    fsd f18, 144(a0)
    fsd f19, 152(a0)
    fsd f20, 160(a0)
    fsd f21, 168(a0)
    fsd f22, 176(a0)
    fsd f23, 184(a0)
    fsd f24, 192(a0)
    fsd f25, 200(a0)
    fsd f26, 208(a0)
    fsd f27, 216(a0)
    fsd f28, 224(a0)
    fsd f29, 232(a0)
    fsd f30, 240(a0)
    fsd f31, 248(a0)
    frcsr t0
    sw t0, 256(a0)
    ret

    .global spacekernel_riscv_d_restore
spacekernel_riscv_d_restore:
    fld f0, 0(a0)
    fld f1, 8(a0)
    fld f2, 16(a0)
    fld f3, 24(a0)
    fld f4, 32(a0)
    fld f5, 40(a0)
    fld f6, 48(a0)
    fld f7, 56(a0)
    fld f8, 64(a0)
    fld f9, 72(a0)
    fld f10, 80(a0)
    fld f11, 88(a0)
    fld f12, 96(a0)
    fld f13, 104(a0)
    fld f14, 112(a0)
    fld f15, 120(a0)
    fld f16, 128(a0)
    fld f17, 136(a0)
    fld f18, 144(a0)
    fld f19, 152(a0)
    fld f20, 160(a0)
    fld f21, 168(a0)
    fld f22, 176(a0)
    fld f23, 184(a0)
    fld f24, 192(a0)
    fld f25, 200(a0)
    fld f26, 208(a0)
    fld f27, 216(a0)
    fld f28, 224(a0)
    fld f29, 232(a0)
    fld f30, 240(a0)
    fld f31, 248(a0)
    lw t0, 256(a0)
    fscsr t0
    ret

    .option pop
