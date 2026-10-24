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
    pub(super) fn redraw(&mut self) {
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

    pub(super) fn clear_screen(&mut self) {
        let background = self.pack(DEFAULT_BACKGROUND);
        let dst = self.address as *mut u8;
        let is_stride_contiguous = self.pitch == self.width * self.bytes_per_pixel;

        #[cfg(target_arch = "x86_64")]
        if self.bytes_per_pixel == 4 {
            if let Some(scratch) = self.fpu_scratch.as_mut() {
                // Must be executed inside the explicit `with_kernel` section
                let success = unsafe {
                    crate::fpu::with_kernel(scratch, || {
                        if is_stride_contiguous {
                            super::simd::spacekernel_fpsimd_fb_clear(
                                dst,
                                self.height * self.pitch,
                                background,
                            );
                        } else {
                            let line_bytes = self.width * self.bytes_per_pixel;
                            for y in 0..self.height {
                                super::simd::spacekernel_fpsimd_fb_clear(
                                    dst.add(y * self.pitch),
                                    line_bytes,
                                    background,
                                );
                            }
                        }
                    })
                };
                if success.is_ok() {
                    return;
                }
            }
        }

        // Fast 64-bit integer path (fallback for non-x86_64 or when FPU scratch borrow is inactive)
        if self.bytes_per_pixel == 4 {
            unsafe {
                if is_stride_contiguous {
                    super::simd::fast_clear(dst, self.height * self.pitch, background);
                } else {
                    let line_bytes = self.width * self.bytes_per_pixel;
                    for y in 0..self.height {
                        super::simd::fast_clear(dst.add(y * self.pitch), line_bytes, background);
                    }
                }
            }
            return;
        }

        // Byte-by-byte fallback for 24bpp
        for y in 0..self.height {
            for x in 0..self.width {
                self.draw_packed_pixel(x, y, background);
            }
        }
    }

    pub(super) fn scroll_pixels(&mut self) {
        let bytes = (self.rows - 1) * GLYPH_HEIGHT * self.pitch;
        let dst = self.address as *mut u8;
        let src = (self.address + GLYPH_HEIGHT * self.pitch) as *const u8;

        #[cfg(target_arch = "x86_64")]
        if let Some(scratch) = self.fpu_scratch.as_mut() {
            // Must be executed inside the explicit `with_kernel` section
            let success = unsafe {
                crate::fpu::with_kernel(scratch, || {
                    super::simd::spacekernel_fpsimd_fb_copy(dst, src, bytes);
                })
            };
            if success.is_ok() {
                return;
            }
        }

        // Fast 64-bit integer copy (fallback for non-x86_64 or when FPU scratch borrow is inactive)
        unsafe {
            super::simd::fast_copy(dst, src, bytes);
        }
    }

    pub(super) fn draw_cell(&self, column: usize, row: usize, cell: Cell) {
        let glyph = BASIC_FONTS
            .get(cell.character)
            .or_else(|| BASIC_FONTS.get('?'))
            .unwrap_or([0; 8]);
        let foreground = self.pack(cell.foreground);
        let background = self.pack(cell.background);
        if self.bytes_per_pixel == 4 {
            let base_offset = row * GLYPH_HEIGHT * self.pitch + column * GLYPH_WIDTH * 4;
            for y in 0..GLYPH_HEIGHT {
                let bits = glyph[y / 2];
                let line_ptr = (self.address + base_offset + y * self.pitch) as *mut u32;
                for x in 0..GLYPH_WIDTH {
                    let color = if bits & (1 << x) != 0 {
                        foreground
                    } else {
                        background
                    };
                    unsafe { line_ptr.add(x).write_volatile(color) };
                }
            }
            return;
        }
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
