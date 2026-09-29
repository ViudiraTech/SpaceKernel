    .section .text.spacekernel_vectors, "ax"
    .balign 2048
    .global __spacekernel_aarch64_vectors
__spacekernel_aarch64_vectors:
    .irp vector,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
        mov x16, #\vector
        b __spacekernel_aarch64_exception_entry
        .balign 128
    .endr

__spacekernel_aarch64_exception_entry:
    mov x1, sp
    mrs x17, tpidr_el1
    ldr x17, [x17]
    mov sp, x17
    mov x0, x16
    bl spacekernel_aarch64_exception
