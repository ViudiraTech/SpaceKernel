/*
 *
 *       src/arch/x86_64/mod.rs
 *       x86-64 architecture backend interfaces
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

pub mod apic;
mod cpu;
pub mod cpuid;
pub mod fpu;
mod interrupts;
mod paging;
mod serial;

pub use cpu::{counter, disable_interrupts, enable_interrupts, halt, irq_restore, irq_save};
pub use interrupts::init_bsp as init_exceptions;
pub use paging::setup_memory_protection;
pub use paging::{
    flush_page, flush_table, page_root, paging_geometry, pte_is_table, pte_leaf, pte_phys,
    pte_present, pte_table, set_page_root,
};
pub use serial::{serial_init, serial_try_read, serial_write};
