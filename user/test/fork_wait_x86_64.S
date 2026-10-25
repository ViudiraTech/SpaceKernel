.global _start
.text
_start:
    mov $57, %rax          /* fork */
    syscall
    test %rax, %rax
    js fail
    jz child

    /* parent: wait4(child, &status, 0, 0) */
    mov %rax, %rdi
    lea status(%rip), %rsi
    xor %rdx, %rdx
    xor %r10, %r10
    mov $61, %rax
    syscall
    test %rax, %rax
    js fail
    cmpq $0x700, status(%rip)
    jne fail

    mov $1, %rax
    mov $1, %rdi
    lea parent_msg(%rip), %rsi
    mov $10, %rdx
    syscall
    mov $60, %rax
    mov $42, %rdi
    syscall

child:
    /* Write a private data page to force a COW fault. */
    lea cow_page(%rip), %rax
    movb $0x43, (%rax)
    mov $1, %rax
    mov $1, %rdi
    lea child_msg(%rip), %rsi
    mov $9, %rdx
    syscall
    mov $60, %rax
    mov $7, %rdi
    syscall

fail:
    mov $1, %rax
    mov $1, %rdi
    lea fail_msg(%rip), %rsi
    mov $5, %rdx
    syscall
    mov $60, %rax
    mov $1, %rdi
    syscall

.data
.align 8
status:     .quad 0
cow_page:   .byte 0x50
parent_msg: .ascii "parent ok\n"
child_msg:  .ascii "child ok\n"
fail_msg:   .ascii "fail\n"
