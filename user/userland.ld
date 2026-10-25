/*
 *       user/userland.ld
 *       Static userland layout: text at 0x400000, data page aligned
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

ENTRY(_start)
SECTIONS
{
    . = 0x400000;
    .text : { *(.text*) *(.rodata*) }
    . = ALIGN(0x1000);
    .data : { *(.data*) *(.bss*) *(COMMON) }
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) *(.eh_frame_hdr) }
}
