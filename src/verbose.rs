/*
 *
 *       src/verbose.rs
 *       Verbose boot diagnostics switch
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Always-built diagnostics used by both test and production kernels.

pub fn enabled() -> bool {
    crate::boot::cmdline()
        .split_whitespace()
        .any(|arg| arg == "verbose" || arg == "selftest=verbose")
}

pub fn log(args: core::fmt::Arguments) {
    if enabled() {
        crate::printk::record(crate::printk::Level::Info, args);
    }
}
