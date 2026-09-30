/*
 *
 *       src/arch/paging.rs
 *       Page-table geometry shared by all supported architectures
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Page-table geometry shared by all supported architectures.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub levels: usize,
    pub virtual_bits: u32,
    pub physical_bits: u32,
}

impl Geometry {
    pub const fn slot_bytes(self) -> u64 {
        1 << (12 + 9 * (self.levels - 1))
    }

    /// Sign extend the upper-half root index into a canonical kernel address.
    pub fn slot_base(self, index: usize) -> u64 {
        let address = index as u64 * self.slot_bytes();
        let shift = 64 - self.virtual_bits;
        (((address << shift) as i64) >> shift) as u64
    }

    pub fn valid_physical(self, address: u64) -> bool {
        address < (1 << self.physical_bits)
    }
}
