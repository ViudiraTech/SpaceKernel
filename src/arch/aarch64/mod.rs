/*
 *
 *       src/arch/aarch64/mod.rs
 *       AArch64 architecture backend interfaces
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

mod cpu;
pub mod cpuid;
mod exceptions;
pub mod fpu;
pub mod gic;
mod paging;
mod serial;

pub use cpu::{counter, disable_interrupts, enable_interrupts, halt, irq_restore, irq_save};
pub use exceptions::init_bsp as init_exceptions;
pub use paging::{
    flush_page, flush_table, page_root, paging_geometry, pte_is_table, pte_leaf, pte_phys,
    pte_present, pte_table, set_page_root, setup_device_memory,
};
pub use serial::{SERIAL_PHYS, serial_init, serial_try_read, serial_write};
