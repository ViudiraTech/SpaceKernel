.global _start
.text
_start:
    mov $1, %rax
    mov $1, %rdi
    lea msg(%rip), %rsi
    mov $48, %rdx
    syscall

    mov $39, %rax
    syscall

    mov $24, %rax
    syscall

    mov $60, %rax
    mov $42, %rdi
    syscall

.data
msg:
    .ascii "Hello from SpaceKernel user mode via Linux ABI!\n"
