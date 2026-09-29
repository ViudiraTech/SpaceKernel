//! Kernel console selection and synchronized output routing.

mod device;

pub use device::{TtyDevice, poll_serial, read, receive, set_termios, write};

use super::{
    fbcon::{FbConsole, VT_COUNT},
    line::LineDiscipline,
};
use crate::{arch, boot, sync::SpinLock};

static CONSOLE: SpinLock<State> = SpinLock::new(State::new());

struct State {
    initialized: bool,
    serial: bool,
    framebuffer: Option<FbConsole>,
    active_vt: usize,
    invalid_argument: bool,
    framebuffer_unavailable: bool,
    serial_line: LineDiscipline,
    virtual_lines: [LineDiscipline; VT_COUNT],
}

impl State {
    const fn new() -> Self {
        Self {
            initialized: false,
            serial: false,
            framebuffer: None,
            active_vt: 1,
            invalid_argument: false,
            framebuffer_unavailable: false,
            serial_line: LineDiscipline::new(),
            virtual_lines: [const { LineDiscipline::new() }; VT_COUNT],
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
