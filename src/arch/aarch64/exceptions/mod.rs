//! EL1 vector installation and CPU-local exception state.

mod per_cpu;
mod vector;

pub use per_cpu::init_bsp;
