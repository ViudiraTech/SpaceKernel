/*
 *
 *       src/tty/fbcon/terminal.rs
 *       Character decoding, ANSI CSI processing and ring scrolling
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Character decoding, ANSI CSI processing and ring scrolling.
use super::{Cell, Escape, FbConsole, HISTORY_ROWS};

impl FbConsole {
    pub(super) fn feed(&mut self, index: usize, byte: u8) {
        let escape = self.terminals[index].escape;
        match escape {
            Escape::Ground if byte == 0x1b => self.terminals[index].escape = Escape::Esc,
            Escape::Ground => self.feed_plain(index, byte),
            Escape::Esc if byte == b'[' => {
                self.terminals[index].escape = Escape::Csi {
                    values: [0; 8],
                    index: 0,
                };
            }
            Escape::Esc if byte == b'7' => {
                let terminal = &mut self.terminals[index];
                (terminal.saved_x, terminal.saved_y) = (terminal.x, terminal.y);
                terminal.escape = Escape::Ground;
            }
            Escape::Esc if byte == b'8' => {
                let terminal = &mut self.terminals[index];
                (terminal.x, terminal.y) = (terminal.saved_x, terminal.saved_y);
                terminal.escape = Escape::Ground;
            }
            Escape::Esc => self.terminals[index].escape = Escape::Ground,
            Escape::Csi {
                mut values,
                index: mut parameter,
            } => {
                if byte.is_ascii_digit() {
                    values[parameter] = values[parameter]
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    self.terminals[index].escape = Escape::Csi {
                        values,
                        index: parameter,
                    };
                } else if byte == b';' && parameter + 1 < values.len() {
                    parameter += 1;
                    self.terminals[index].escape = Escape::Csi {
                        values,
                        index: parameter,
                    };
                } else {
                    self.terminals[index].escape = Escape::Ground;
                    self.execute_csi(index, byte, &values[..=parameter]);
                }
            }
        }
    }

    fn feed_plain(&mut self, index: usize, byte: u8) {
        let terminal = &mut self.terminals[index];
        if terminal.utf8_remaining != 0 {
            if byte & 0xc0 == 0x80 {
                terminal.utf8_codepoint = (terminal.utf8_codepoint << 6) | u32::from(byte & 0x3f);
                terminal.utf8_remaining -= 1;
                if terminal.utf8_remaining == 0 {
                    let character = char::from_u32(terminal.utf8_codepoint).unwrap_or('�');
                    self.put_char(index, character);
                }
                return;
            }
            terminal.utf8_remaining = 0;
            self.put_char(index, '�');
        }
        match byte {
            b'\n' => self.newline(index),
            b'\r' => {
                self.terminals[index].x = 0;
                self.terminals[index].wrap_pending = false;
            }
            b'\t' => {
                let spaces = 8 - self.terminals[index].x % 8;
                for _ in 0..spaces {
                    self.put_char(index, ' ');
                }
            }
            0x08 => {
                let terminal = &mut self.terminals[index];
                if terminal.wrap_pending {
                    terminal.wrap_pending = false;
                } else {
                    terminal.x = terminal.x.saturating_sub(1);
                }
            }
            0x20..=0x7e => self.put_char(index, byte as char),
            0xc2..=0xdf => self.start_utf8(index, byte, 1, 0x1f),
            0xe0..=0xef => self.start_utf8(index, byte, 2, 0x0f),
            0xf0..=0xf4 => self.start_utf8(index, byte, 3, 0x07),
            _ => {}
        }
    }

    fn start_utf8(&mut self, index: usize, byte: u8, remaining: u8, mask: u8) {
        let terminal = &mut self.terminals[index];
        terminal.utf8_codepoint = u32::from(byte & mask);
        terminal.utf8_remaining = remaining;
    }

    fn put_char(&mut self, index: usize, character: char) {
        if self.terminals[index].wrap_pending {
            self.newline(index);
        }
        let terminal = &mut self.terminals[index];
        let column = terminal.x;
        let row = terminal.y;
        let cell = Cell {
            character,
            foreground: terminal.foreground,
            background: terminal.background,
        };
        let cell_index = terminal.cell_index(row, column, self.columns);
        terminal.cells[cell_index] = cell;
        if index == self.active && terminal.view_offset == 0 {
            self.draw_cell(column, row, cell);
        }
        let terminal = &mut self.terminals[index];
        if terminal.x + 1 == self.columns {
            terminal.wrap_pending = true;
        } else {
            terminal.x += 1;
        }
    }

    fn newline(&mut self, index: usize) {
        let terminal = &mut self.terminals[index];
        terminal.x = 0;
        terminal.wrap_pending = false;
        if terminal.y + 1 < self.rows {
            terminal.y += 1;
            return;
        }
        terminal.top_row = (terminal.top_row + 1) % terminal.total_rows;
        terminal.history_count = (terminal.history_count + 1).min(HISTORY_ROWS);
        if terminal.view_offset != 0 {
            terminal.view_offset = (terminal.view_offset + 1).min(terminal.history_count);
        }
        for column in 0..self.columns {
            let cell_index = terminal.cell_index(self.rows - 1, column, self.columns);
            terminal.clear_cell(cell_index);
        }
        if index == self.active && self.terminals[index].view_offset == 0 {
            self.scroll_pixels();
            for column in 0..self.columns {
                let terminal = &self.terminals[index];
                let cell = terminal.cells[terminal.cell_index(self.rows - 1, column, self.columns)];
                self.draw_cell(column, self.rows - 1, cell);
            }
        }
    }
}
