/*
 *
 *       src/tty/console/mod.rs
 *       Kernel console selection and device ownership
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Kernel console selection and synchronized output routing.

mod device;

pub use device::{TtyDevice, poll_serial, read, receive, set_termios, write};

use super::{
    fbcon::{FbConsole, VT_COUNT},
    line::LineDiscipline,
};
use crate::{arch, boot, sync::SpinLock};

pub static DIRTY: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static CONSOLE: SpinLock<State> = SpinLock::new(State::new());

struct State {
    initialized: bool,
    serial: bool,
    framebuffer: Option<FbConsole>,
    framebuffer_owner: Option<alloc::sync::Arc<dyn crate::vfs::Inode>>,
    active_vt: usize,
    invalid_argument: bool,
    framebuffer_unavailable: bool,
    serial_line: LineDiscipline,
    virtual_lines: [LineDiscipline; VT_COUNT],
    session: u64,
    foreground_pgrp: u64,
}

impl State {
    const fn new() -> Self {
        Self {
            initialized: false,
            serial: false,
            framebuffer: None,
            framebuffer_owner: None,
            active_vt: 1,
            invalid_argument: false,
            framebuffer_unavailable: false,
            serial_line: LineDiscipline::new(),
            virtual_lines: [const { LineDiscipline::new() }; VT_COUNT],
            session: 0,
            foreground_pgrp: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConsoleStatus {
    pub serial: bool,
    pub framebuffer: bool,
    pub active_vt: usize,
    pub invalid_argument: bool,
    pub framebuffer_unavailable: bool,
    pub columns: usize,
    pub rows: usize,
}

pub fn init() {
    let mut serial = false;
    let mut vt = None;
    let mut found = false;
    let mut invalid = false;
    for word in boot::cmdline().split_ascii_whitespace() {
        let Some(value) = word.strip_prefix("console=") else {
            continue;
        };
        found = true;
        match value {
            "ttyS0" => serial = true,
            "tty0" => vt = Some(1),
            _ => {
                if let Some(number) = value
                    .strip_prefix("tty")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if (1..=VT_COUNT).contains(&number) {
                        vt = Some(number);
                    } else {
                        invalid = true;
                    }
                } else {
                    invalid = true;
                }
            }
        }
    }
    if !found || (vt.is_none() && !serial) {
        serial = true;
    }

    // Allocate terminal buffers before taking the console lock. Nothing in
    // the allocator may call printk while a terminal lock is held.
    let framebuffer = vt
        .and_then(|_| boot::framebuffer())
        .and_then(|framebuffer| FbConsole::new(framebuffer).ok());
    let unavailable = vt.is_some() && framebuffer.is_none();
    if unavailable {
        serial = true;
    }
    let mut state = CONSOLE.lock();
    assert!(!state.initialized, "TTY initialized twice");
    state.initialized = true;
    state.serial = serial;
    state.active_vt = vt.unwrap_or(1);
    state.invalid_argument = invalid;
    state.framebuffer_unavailable = unavailable;
    state.framebuffer = framebuffer;
    let active = state.active_vt;
    if let Some(framebuffer) = state.framebuffer.as_mut() {
        framebuffer.switch_to(active);
    }
}

/// Session that owns this console, or 0 when unclaimed.
pub fn controlling_session() -> u64 {
    CONSOLE.lock().session
}

pub fn foreground_pgrp() -> u64 {
    CONSOLE.lock().foreground_pgrp
}

/// Claim the console for `sid` if it has no owner yet.
pub fn assign_session(sid: u64) -> bool {
    let mut state = CONSOLE.lock();
    if state.session == 0 {
        state.session = sid;
    }
    state.session == sid
}

/// Set the foreground process group. The caller has already checked that the
/// group belongs to this session.
pub fn set_foreground_pgrp(sid: u64, pgrp: u64) -> bool {
    let mut state = CONSOLE.lock();
    if state.session == 0 {
        state.session = sid;
    }
    if state.session != sid {
        return false;
    }
    state.foreground_pgrp = pgrp;
    true
}

pub fn status() -> ConsoleStatus {
    let state = CONSOLE.lock();
    let (columns, rows) = state
        .framebuffer
        .as_ref()
        .map_or((0, 0), FbConsole::dimensions);
    ConsoleStatus {
        serial: state.serial,
        framebuffer: state.framebuffer.is_some(),
        active_vt: state.active_vt,
        invalid_argument: state.invalid_argument,
        framebuffer_unavailable: state.framebuffer_unavailable,
        columns,
        rows,
    }
}

/// Console writes serialize each complete log record with terminal output.
/// Serial and framebuffer are never written under the printk ring lock.
pub fn write_kernel(bytes: &[u8]) {
    DIRTY.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let mut state = CONSOLE.lock();
    if !state.initialized || state.serial {
        write_serial(bytes);
    }
    let active = state.active_vt;
    if let Some(framebuffer) = state.framebuffer.as_mut() {
        framebuffer.write(active, bytes);
    }
}

pub fn switch_to(tty_number: usize) -> bool {
    DIRTY.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let mut state = CONSOLE.lock();
    if state
        .framebuffer
        .as_mut()
        .is_some_and(|fb| fb.switch_to(tty_number))
    {
        state.active_vt = tty_number;
        true
    } else {
        false
    }
}

pub fn scrollback(tty_number: usize, delta: isize) -> bool {
    DIRTY.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    CONSOLE
        .lock()
        .framebuffer
        .as_mut()
        .is_some_and(|fb| fb.scrollback(tty_number, delta))
}

fn write_serial(bytes: &[u8]) {
    for &byte in bytes {
        if byte == b'\n' {
            arch::serial_write(b'\r');
        }
        arch::serial_write(byte);
    }
}

/// # Safety
/// `owner` must retain the contiguous virtual buffer until its final reference is dropped.
pub unsafe fn install_gpu_console(
    address: usize,
    width: u32,
    height: u32,
    owner: alloc::sync::Arc<dyn crate::vfs::Inode>,
) -> Result<(), super::fbcon::FbError> {
    let framebuffer = unsafe { FbConsole::linear(address, width, height)? };
    let (old, old_owner) = {
        let mut state = CONSOLE.lock();
        state.framebuffer_unavailable = false;
        (
            state.framebuffer.replace(framebuffer),
            state.framebuffer_owner.replace(owner),
        )
    };
    drop(old);
    drop(old_owner);
    DIRTY.fetch_add(1, core::sync::atomic::Ordering::Release);
    Ok(())
}
