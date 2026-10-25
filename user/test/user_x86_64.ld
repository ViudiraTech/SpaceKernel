/*
 *       user/test/user_x86_64.ld
 *       Fixed-address user test layout: text 0x200000, data 0x201000
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

ENTRY(_start)
PHDRS
{
    text PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
}
SECTIONS
{
    . = 0x200000;
    .text : { *(.text*) *(.rodata*) } :text
    . = 0x210000;
    .data : { *(.data*) *(.bss*) *(COMMON) } :data
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
