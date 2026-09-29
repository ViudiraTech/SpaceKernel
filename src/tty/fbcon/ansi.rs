//! ANSI CSI cursor movement, erase operations and color attributes.

use super::{DEFAULT_BACKGROUND, DEFAULT_FOREGROUND, FbConsole, PALETTE};

impl FbConsole {
    pub(super) fn execute_csi(&mut self, index: usize, command: u8, values: &[u16]) {
        self.terminals[index].wrap_pending = false;
        let first = usize::from(values[0]);
        match command {
            b'A' => self.terminals[index].y = self.terminals[index].y.saturating_sub(first.max(1)),
            b'B' => {
                self.terminals[index].y =
                    (self.terminals[index].y + first.max(1)).min(self.rows - 1)
            }
            b'C' => {
                self.terminals[index].x =
                    (self.terminals[index].x + first.max(1)).min(self.columns - 1)
            }
            b'D' => self.terminals[index].x = self.terminals[index].x.saturating_sub(first.max(1)),
            b'H' | b'f' => {
                self.terminals[index].y = first.max(1).min(self.rows) - 1;
                self.terminals[index].x = values
                    .get(1)
                    .copied()
                    .map_or(0, usize::from)
                    .max(1)
                    .min(self.columns)
                    - 1;
            }
            b'J' => self.erase_display(index, first),
            b'K' => self.erase_line(index, first),
            b'm' => self.set_graphics(index, values),
            b's' => {
                let terminal = &mut self.terminals[index];
                (terminal.saved_x, terminal.saved_y) = (terminal.x, terminal.y);
            }
            b'u' => {
                let terminal = &mut self.terminals[index];
                (terminal.x, terminal.y) = (terminal.saved_x, terminal.saved_y);
            }
            _ => {}
        }
    }

    fn erase_display(&mut self, index: usize, mode: usize) {
        let terminal = &self.terminals[index];
        let cursor = terminal.y * self.columns + terminal.x;
        let (start, end) = match mode {
            0 => (cursor, self.columns * self.rows),
            1 => (0, cursor + 1),
            2 | 3 => (0, self.columns * self.rows),
            _ => return,
        };
        self.erase_range(index, start, end);
    }

    fn erase_line(&mut self, index: usize, mode: usize) {
        let terminal = &self.terminals[index];
        let row_start = terminal.y * self.columns;
        let (start, end) = match mode {
            0 => (row_start + terminal.x, row_start + self.columns),
            1 => (row_start, row_start + terminal.x + 1),
            2 => (row_start, row_start + self.columns),
            _ => return,
        };
        self.erase_range(index, start, end);
    }

    fn erase_range(&mut self, index: usize, start: usize, end: usize) {
        for cell_index in start..end {
            let row = cell_index / self.columns;
            let column = cell_index % self.columns;
            let physical = self.terminals[index].cell_index(row, column, self.columns);
            self.terminals[index].clear_cell(physical);
            if index == self.active && self.terminals[index].view_offset == 0 {
                self.draw_cell(column, row, self.terminals[index].cells[physical]);
            }
        }
    }

    fn set_graphics(&mut self, index: usize, values: &[u16]) {
        let terminal = &mut self.terminals[index];
        for &value in values {
            match value {
                0 => {
                    terminal.foreground = DEFAULT_FOREGROUND;
                    terminal.background = DEFAULT_BACKGROUND;
                }
                30..=37 => terminal.foreground = PALETTE[usize::from(value - 30)],
                40..=47 => terminal.background = PALETTE[usize::from(value - 40)],
                90..=97 => terminal.foreground = PALETTE[usize::from(value - 90 + 8)],
                100..=107 => terminal.background = PALETTE[usize::from(value - 100 + 8)],
                39 => terminal.foreground = DEFAULT_FOREGROUND,
                49 => terminal.background = DEFAULT_BACKGROUND,
                _ => {}
            }
        }
    }
}
