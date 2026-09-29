ENTRY(kmain)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    dynamic PT_DYNAMIC FLAGS(6);
}

SECTIONS {
    . = 0;
    .text : { *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : { *(.rodata .rodata.*) } :rodata
    . = ALIGN(4096);
    .limine_requests : {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :data
    .data : { *(.data .data.*) } :data
    .dynamic : { *(.dynamic) } :data :dynamic
    .got : { *(.got .got.*) } :data
    .rela.dyn : { *(.rela.dyn) } :data
    .bss : { *(.bss .bss.* COMMON) } :data
    /DISCARD/ : { *(.eh_frame*) *(.note*) }
}
