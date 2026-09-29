//! CPU-local descriptor tables and exception routing.
mod gdt;
mod handlers;
mod idt;

use crate::{boot, sync::SpinLock};
use alloc::{boxed::Box, vec::Vec};
use gdt::CpuTables;

static TABLES: SpinLock<Vec<Box<CpuTables>>> = SpinLock::new(Vec::new());

pub fn init_bsp() -> usize {
    let count = boot::cpu_count();
    assert!((1..=256).contains(&count), "invalid Limine CPU count");
    let mut prepared = Vec::with_capacity(count);
    for _ in 0..count {
        prepared.push(CpuTables::new());
    }
    let mut tables = TABLES.lock();
    assert!(tables.is_empty(), "CPU tables initialized twice");
    *tables = prepared;
    drop(tables);
    load_cpu(boot::bsp_cpu_index());
    count
}

/// Called by each CPU before it can receive interrupts or enter ring 3.
/// The per-CPU boxes are never moved, mutated or freed after publication.
pub fn load_cpu(index: usize) {
    let tables = TABLES.lock();
    tables.get(index).expect("unknown CPU index").load();
}
