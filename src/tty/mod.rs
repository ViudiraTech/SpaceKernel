/*
 *
 *       src/tty/mod.rs
 *       Terminal subsystem public interfaces
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Kernel terminal devices and console routing.

mod console;
mod fbcon;
mod line;

pub use console::{
    TtyDevice, init, poll_serial, read, receive, scrollback, set_termios, status, switch_to, write,
    write_kernel,
};
pub use line::{InputEvent, LineDiscipline, ReadError, Termios};
