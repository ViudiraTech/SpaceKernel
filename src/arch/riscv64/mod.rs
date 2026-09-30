/*
 *
 *       src/arch/riscv64/mod.rs
 *       RISC-V architecture backend interfaces
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

pub mod clint;
mod cpu;
pub mod cpuid;
mod exceptions;
pub mod fpu;
mod paging;
pub mod plic;
mod serial;

pub use cpu::{counter, disable_interrupts, enable_interrupts, halt, irq_restore, irq_save};
pub(crate) use exceptions::current_cpu_index;
pub use exceptions::init_bsp as init_exceptions;
pub use paging::{
    flush_page, flush_table, page_root, paging_geometry, pte_is_table, pte_leaf, pte_phys,
    pte_present, pte_table, set_page_root,
};
pub use serial::{SERIAL_PHYS, serial_init, serial_try_read, serial_write};
