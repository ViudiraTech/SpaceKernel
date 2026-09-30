/*
 *
 *       src/tty/fbcon/state.rs
 *       Ring-backed virtual terminal cells and parser state
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Ring-backed virtual terminal cells and parser state.
use super::{DEFAULT_BACKGROUND, DEFAULT_FOREGROUND, HISTORY_ROWS};
use alloc::{vec, vec::Vec};

#[derive(Clone, Copy)]
pub(super) struct Cell {
    pub(super) character: char,
    pub(super) foreground: u32,
    pub(super) background: u32,
}

impl Cell {
    const fn blank() -> Self {
        Self {
            character: ' ',
            foreground: DEFAULT_FOREGROUND,
            background: DEFAULT_BACKGROUND,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Escape {
    Ground,
    Esc,
    Csi { values: [u16; 8], index: usize },
}

pub(super) struct Terminal {
    pub(super) cells: Vec<Cell>,
    pub(super) total_rows: usize,
    pub(super) top_row: usize,
    pub(super) history_count: usize,
    pub(super) view_offset: usize,
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) saved_x: usize,
    pub(super) saved_y: usize,
    pub(super) wrap_pending: bool,
    pub(super) foreground: u32,
    pub(super) background: u32,
    pub(super) escape: Escape,
    pub(super) utf8_codepoint: u32,
    pub(super) utf8_remaining: u8,
}

impl Terminal {
    pub(super) fn new(columns: usize, rows: usize) -> Self {
        Self {
            cells: vec![Cell::blank(); columns * (rows + HISTORY_ROWS)],
            total_rows: rows + HISTORY_ROWS,
            top_row: 0,
            history_count: 0,
            view_offset: 0,
            x: 0,
            y: 0,
            saved_x: 0,
            saved_y: 0,
            wrap_pending: false,
            foreground: DEFAULT_FOREGROUND,
            background: DEFAULT_BACKGROUND,
            escape: Escape::Ground,
            utf8_codepoint: 0,
            utf8_remaining: 0,
        }
    }

    pub(super) fn clear_cell(&mut self, index: usize) {
        self.cells[index] = Cell {
            character: ' ',
            foreground: self.foreground,
            background: self.background,
        };
    }

    pub(super) fn cell_index(&self, row: usize, column: usize, columns: usize) -> usize {
        ((self.top_row + row) % self.total_rows) * columns + column
    }

    pub(super) fn displayed_index(&self, row: usize, column: usize, columns: usize) -> usize {
        ((self.top_row + self.total_rows - self.view_offset + row) % self.total_rows) * columns
            + column
    }
}
