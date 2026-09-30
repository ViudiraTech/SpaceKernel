/*
 *
 *       src/tty/console/device.rs
 *       TTY device routing and termios access
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Named TTY I/O and line discipline dispatch.

use super::{CONSOLE, write_serial};
use crate::arch;
use crate::tty::{
    fbcon::VT_COUNT,
    line::{InputEvent, ReadError, Termios},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyDevice {
    Serial0,
    Foreground,
    Virtual(usize),
}

/// Raw write to a named TTY. This path deliberately bypasses printk metadata.
pub fn write(device: TtyDevice, bytes: &[u8]) -> bool {
    let mut state = CONSOLE.lock();
    match device {
        TtyDevice::Serial0 => {
            write_serial(bytes);
            true
        }
        TtyDevice::Foreground => state
            .framebuffer
            .as_mut()
            .is_some_and(|fb| fb.write(0, bytes)),
        TtyDevice::Virtual(number) => state
            .framebuffer
            .as_mut()
            .is_some_and(|fb| fb.write(number, bytes)),
    }
}

pub fn receive(device: TtyDevice, byte: u8) -> InputEvent {
    let mut state = CONSOLE.lock();
    let number = match device {
        TtyDevice::Serial0 => None,
        TtyDevice::Foreground => Some(state.active_vt),
        TtyDevice::Virtual(number) if (1..=VT_COUNT).contains(&number) => Some(number),
        _ => return InputEvent::Overflow,
    };
    let line = match number {
        None => &mut state.serial_line,
        Some(number) => &mut state.virtual_lines[number - 1],
    };
    let echo = line.termios().echo;
    let normalized = if line.termios().icrnl && byte == b'\r' {
        b'\n'
    } else {
        byte
    };
    let event = line.feed(byte);
    let echo_bytes = match event {
        InputEvent::Echo { bytes, length } => Some((bytes, length)),
        InputEvent::LineReady if echo && normalized == b'\n' => Some(([b'\n', 0, 0], 1)),
        _ => None,
    };
    if let Some((bytes, length)) = echo_bytes {
        match number {
            None => write_serial(&bytes[..length]),
            Some(number) => {
                if let Some(framebuffer) = state.framebuffer.as_mut() {
                    framebuffer.write(number, &bytes[..length]);
                }
            }
        }
    }
    event
}

pub fn read(device: TtyDevice, output: &mut [u8]) -> Result<usize, ReadError> {
    let mut state = CONSOLE.lock();
    match device {
        TtyDevice::Serial0 => state.serial_line.read(output),
        TtyDevice::Foreground => {
            let active = state.active_vt;
            state.virtual_lines[active - 1].read(output)
        }
        TtyDevice::Virtual(number) if (1..=VT_COUNT).contains(&number) => {
            state.virtual_lines[number - 1].read(output)
        }
        _ => Err(ReadError::NoDevice),
    }
}

pub fn set_termios(device: TtyDevice, termios: Termios) -> Result<(), ReadError> {
    let mut state = CONSOLE.lock();
    match device {
        TtyDevice::Serial0 => state.serial_line.set_termios(termios),
        TtyDevice::Foreground => {
            let active = state.active_vt;
            state.virtual_lines[active - 1].set_termios(termios)
        }
        TtyDevice::Virtual(number) if (1..=VT_COUNT).contains(&number) => {
            state.virtual_lines[number - 1].set_termios(termios)
        }
        _ => Err(ReadError::NoDevice),
    }
}

pub fn poll_serial() {
    for _ in 0..64 {
        let Some(byte) = arch::serial_try_read() else {
            break;
        };
        receive(TtyDevice::Serial0, byte);
    }
}
