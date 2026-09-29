//! Supervisor trap vector installation and hart-local state.

mod per_cpu;
mod vector;

pub use per_cpu::init_bsp;
