/*
 *
 *       src/arch/riscv64/exceptions/mod.rs
 *       Supervisor trap vector installation and hart-local state
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Supervisor trap vector installation and hart-local state.

mod per_cpu;
mod vector;

pub(crate) use per_cpu::current_cpu_index;
pub use per_cpu::{init_bsp, load_cpu};
