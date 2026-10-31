/*
 *
 *       src/tty/fbcon/mod.rs
 *       Limine framebuffer console, split by terminal state and rendering
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Limine framebuffer console, split by terminal state and rendering.
mod ansi;
mod render;
mod simd;
mod state;
mod terminal;

use alloc::vec::Vec;
use limine::framebuffer::{FRAMEBUFFER_RGB, Framebuffer};
use state::{Cell, Escape, Terminal};

pub const VT_COUNT: usize = 8;
const GLYPH_WIDTH: usize = 8;
const GLYPH_HEIGHT: usize = 16;
const MAX_COLUMNS: usize = 256;
const MAX_ROWS: usize = 128;
const HISTORY_ROWS: usize = 256;
const DEFAULT_FOREGROUND: u32 = 0xd8dee9;
const DEFAULT_BACKGROUND: u32 = 0x111827;
const PALETTE: [u32; 16] = [
    0x1b1f2a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xd8dee9, 0x4c566a,
    0xd08770, 0xb7d99a, 0xf0d68a, 0x8fb9dd, 0xc5a0d2, 0x9bd6dc, 0xeceff4,
];

/// The framebuffer is owned by Limine and remains mapped for the lifetime of
/// this kernel. All access is serialized by the enclosing TTY lock.
pub struct FbConsole {
    address: usize,
    pitch: usize,
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
    red_size: u8,
    green_size: u8,
    blue_size: u8,
    columns: usize,
    rows: usize,
    active: usize,
    terminals: Vec<Terminal>,
    fpu_scratch: Option<crate::fpu::State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbError {
    UnsupportedFormat,
    InvalidGeometry,
}

struct FramebufferSpec {
    address: usize,
    width: u64,
    height: u64,
    pitch: u64,
    bpp: u16,
    memory_model: u8,
    red_mask_size: u8,
    red_mask_shift: u8,
    green_mask_size: u8,
    green_mask_shift: u8,
    blue_mask_size: u8,
    blue_mask_shift: u8,
}

impl FbConsole {
    pub fn new(framebuffer: &Framebuffer) -> Result<Self, FbError> {
        Self::from_spec(FramebufferSpec {
            address: framebuffer.address() as usize,
            width: framebuffer.width,
            height: framebuffer.height,
            pitch: framebuffer.pitch,
            bpp: framebuffer.bpp,
            memory_model: framebuffer.memory_model,
            red_mask_size: framebuffer.red_mask_size,
            red_mask_shift: framebuffer.red_mask_shift,
            green_mask_size: framebuffer.green_mask_size,
            green_mask_shift: framebuffer.green_mask_shift,
            blue_mask_size: framebuffer.blue_mask_size,
            blue_mask_shift: framebuffer.blue_mask_shift,
        })
    }
    /// # Safety
    /// The owner must pin at least width * height * 4 writable bytes until this console is replaced.
    pub unsafe fn linear(address: usize, width: u32, height: u32) -> Result<Self, FbError> {
        Self::from_spec(FramebufferSpec {
            address,
            width: width as u64,
            height: height as u64,
            pitch: width as u64 * 4,
            bpp: 32,
            memory_model: FRAMEBUFFER_RGB,
            red_mask_size: 8,
            red_mask_shift: 16,
            green_mask_size: 8,
            green_mask_shift: 8,
            blue_mask_size: 8,
            blue_mask_shift: 0,
        })
    }
    fn from_spec(spec: FramebufferSpec) -> Result<Self, FbError> {
        let width = usize::try_from(spec.width).map_err(|_| FbError::InvalidGeometry)?;
        let height = usize::try_from(spec.height).map_err(|_| FbError::InvalidGeometry)?;
        let pitch = usize::try_from(spec.pitch).map_err(|_| FbError::InvalidGeometry)?;
        let bytes_per_pixel = match spec.bpp {
            24 => 3,
            32 => 4,
            _ => return Err(FbError::UnsupportedFormat),
        };
        if spec.memory_model != FRAMEBUFFER_RGB
            || spec.red_mask_size == 0
            || spec.green_mask_size == 0
            || spec.blue_mask_size == 0
            || spec.red_mask_size > 8
            || spec.green_mask_size > 8
            || spec.blue_mask_size > 8
        {
            return Err(FbError::UnsupportedFormat);
        }
        for (size, shift) in [
            (spec.red_mask_size, spec.red_mask_shift),
            (spec.green_mask_size, spec.green_mask_shift),
            (spec.blue_mask_size, spec.blue_mask_shift),
        ] {
            if u16::from(size) + u16::from(shift) > spec.bpp {
                return Err(FbError::UnsupportedFormat);
            }
        }
        let columns = width / GLYPH_WIDTH;
        let rows = height / GLYPH_HEIGHT;
        if columns == 0
            || rows == 0
            || columns > MAX_COLUMNS
            || rows > MAX_ROWS
            || width
                .checked_mul(bytes_per_pixel)
                .is_none_or(|minimum| pitch < minimum)
            || pitch.checked_mul(height).is_none()
            || spec.address == 0
        {
            return Err(FbError::InvalidGeometry);
        }
        let mut terminals = Vec::with_capacity(VT_COUNT);
        for _ in 0..VT_COUNT {
            terminals.push(Terminal::new(columns, rows));
        }
        let fpu_scratch = crate::fpu::State::new().ok();
        let mut result = Self {
            address: spec.address,
            pitch,
            width,
            height,
            bytes_per_pixel,
            red_shift: spec.red_mask_shift,
            green_shift: spec.green_mask_shift,
            blue_shift: spec.blue_mask_shift,
            red_size: spec.red_mask_size,
            green_size: spec.green_mask_size,
            blue_size: spec.blue_mask_size,
            columns,
            rows,
            active: 0,
            terminals,
            fpu_scratch,
        };
        result.clear_screen();
        Ok(result)
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }

    pub fn switch_to(&mut self, tty_number: usize) -> bool {
        if !(1..=VT_COUNT).contains(&tty_number) {
            return false;
        }
        if self.active != tty_number - 1 {
            self.active = tty_number - 1;
            self.redraw();
        }
        true
    }

    /// Move the visible window through retained terminal history. A positive
    /// delta moves toward older lines; a negative delta follows live output.
    pub fn scrollback(&mut self, tty_number: usize, delta: isize) -> bool {
        if !(1..=VT_COUNT).contains(&tty_number) {
            return false;
        }
        let terminal = &mut self.terminals[tty_number - 1];
        let previous = terminal.view_offset;
        terminal.view_offset = terminal
            .view_offset
            .saturating_add_signed(delta)
            .min(terminal.history_count);
        if previous != terminal.view_offset && self.active == tty_number - 1 {
            self.redraw();
        }
        true
    }

    /// `tty0` addresses the current foreground terminal, as on Linux.
    pub fn write(&mut self, tty_number: usize, bytes: &[u8]) -> bool {
        // A display server owns the scanout; drawing text would corrupt it.
        if crate::tty::vt::graphics_active() {
            return true;
        }
        let index = if tty_number == 0 {
            self.active
        } else {
            tty_number - 1
        };
        if index >= VT_COUNT {
            return false;
        }
        for &byte in bytes {
            self.feed(index, byte);
        }
        true
    }
}
