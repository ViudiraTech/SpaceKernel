/*
 *
 *       src/tty/fbcon/render.rs
 *       Pixel format conversion, glyph blitting and display scrolling
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Pixel format conversion, glyph blitting and display scrolling.
use super::{Cell, DEFAULT_BACKGROUND, FbConsole, GLYPH_HEIGHT, GLYPH_WIDTH};
use font8x8::{BASIC_FONTS, UnicodeFonts};

impl FbConsole {
    pub(super) fn redraw(&self) {
        self.clear_screen();
        for row in 0..self.rows {
            for column in 0..self.columns {
                let terminal = &self.terminals[self.active];
                let cell = terminal.cells[terminal.displayed_index(row, column, self.columns)];
                if cell.character != ' ' || cell.background != DEFAULT_BACKGROUND {
                    self.draw_cell(column, row, cell);
                }
            }
        }
    }

    pub(super) fn clear_screen(&self) {
        let background = self.pack(DEFAULT_BACKGROUND);
        for y in 0..self.height {
            for x in 0..self.width {
                self.draw_packed_pixel(x, y, background);
            }
        }
    }

    pub(super) fn scroll_pixels(&self) {
        let bytes = (self.rows - 1) * GLYPH_HEIGHT * self.pitch;
        // SAFETY: both ranges are inside the validated framebuffer mapping;
        // `copy` handles overlap, and the TTY lock excludes concurrent draws.
        unsafe {
            core::ptr::copy(
                (self.address + GLYPH_HEIGHT * self.pitch) as *const u8,
                self.address as *mut u8,
                bytes,
            );
        }
    }

    pub(super) fn draw_cell(&self, column: usize, row: usize, cell: Cell) {
        let glyph = BASIC_FONTS
            .get(cell.character)
            .or_else(|| BASIC_FONTS.get('?'))
            .unwrap_or([0; 8]);
        let foreground = self.pack(cell.foreground);
        let background = self.pack(cell.background);
        for y in 0..GLYPH_HEIGHT {
            let bits = glyph[y / 2];
            for x in 0..GLYPH_WIDTH {
                let color = if bits & (1 << x) != 0 {
                    foreground
                } else {
                    background
                };
                self.draw_packed_pixel(column * GLYPH_WIDTH + x, row * GLYPH_HEIGHT + y, color);
            }
        }
    }

    #[inline]
    fn pack(&self, rgb: u32) -> u32 {
        let scaled = |component: u32, bits: u8| component >> (8 - bits);
        (scaled((rgb >> 16) & 0xff, self.red_size) << self.red_shift)
            | (scaled((rgb >> 8) & 0xff, self.green_size) << self.green_shift)
            | (scaled(rgb & 0xff, self.blue_size) << self.blue_shift)
    }

    #[inline]
    fn draw_packed_pixel(&self, x: usize, y: usize, value: u32) {
        let offset = y * self.pitch + x * self.bytes_per_pixel;
        if self.bytes_per_pixel == 4 && (self.address + offset).is_multiple_of(4) {
            // SAFETY: the address and pitch were validated; this branch also
            // proves 32-bit alignment and the framebuffer remains mapped.
            unsafe { ((self.address + offset) as *mut u32).write_volatile(value) }
            return;
        }
        for byte in 0..self.bytes_per_pixel {
            // SAFETY: constructor checked pitch, geometry, format and address;
            // the TTY lock is the sole writer of this Limine framebuffer.
            unsafe {
                (self.address as *mut u8)
                    .add(offset + byte)
                    .write_volatile((value >> (byte * 8)) as u8)
            }
        }
    }
}
