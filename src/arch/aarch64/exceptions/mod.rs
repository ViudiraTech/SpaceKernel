/*
 *
 *       src/arch/aarch64/exceptions/mod.rs
 *       EL1 vector installation and CPU-local exception state
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! EL1 vector installation and CPU-local exception state.

mod per_cpu;
mod vector;

pub use per_cpu::init_bsp;
