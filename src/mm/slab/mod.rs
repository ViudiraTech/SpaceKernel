/*
 *
 *       src/mm/slab/mod.rs
 *       Kernel global allocator and slab class dispatch
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Size class slabs and contiguous large allocations.

mod class;
mod large;

use core::alloc::{GlobalAlloc, Layout};

pub struct KernelHeap;

#[global_allocator]
pub static HEAP: KernelHeap = KernelHeap;

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
}

// SAFETY: each size class has its own IRQ-safe lock; the PMM lock protects
// physical page ownership. Large allocations have disjoint PMM-owned spans.
unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match class::class_for(layout) {
            Some(index) => class::alloc_small(index),
            None => large::alloc_large(layout),
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        match class::class_for(layout) {
            Some(index) => class::free_small(pointer, index),
            None => large::free_large(pointer),
        }
    }
}
