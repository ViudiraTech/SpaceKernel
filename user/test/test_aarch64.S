.global _start
.text
_start:
    mov x8, #64
    mov x0, #1
    adr x1, msg
    mov x2, #48
    svc #0

    mov x8, #172
    svc #0

    mov x8, #124
    svc #0

    mov x8, #93
    mov x0, #42
    svc #0

.data
msg:
    .ascii "Hello from SpaceKernel user mode via Linux ABI!\n"
