.global _start
.text
_start:
    li a7, 64
    li a0, 1
    lla a1, msg
    li a2, 48
    ecall

    li a7, 172
    ecall

    li a7, 124
    ecall

    li a7, 93
    li a0, 42
    ecall

.data
msg:
    .ascii "Hello from SpaceKernel user mode via Linux ABI!\n"
