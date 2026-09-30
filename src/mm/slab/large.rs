/*
 *
 *       src/mm/slab/large.rs
 *       Direct PMM backing for allocations larger than a slab slot
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Direct PMM backing for allocations larger than a slab slot.

use super::align_up;
use crate::mm::pmm::{self, PAGE_SIZE};
use core::{alloc::Layout, mem::size_of, ptr};

const LARGE_MAGIC: u64 = 0x4c41_5247_4550_4147;

#[repr(C)]
struct LargeAllocation {
    magic: u64,
    physical: u64,
    pages: usize,
}

pub(super) fn alloc_large(layout: Layout) -> *mut u8 {
    let bytes = match layout
        .size()
        .max(1)
        .checked_add(size_of::<LargeAllocation>())
        .and_then(|n| n.checked_add(layout.align() - 1))
    {
        Some(n) => n,
        None => return ptr::null_mut(),
    };
    let pages = bytes.div_ceil(PAGE_SIZE as usize);
    let physical = match pmm::alloc(pages) {
        Some(p) => p,
        None => return ptr::null_mut(),
    };
    let base = pmm::phys_to_virt(physical);
    let address = match align_up(base + size_of::<LargeAllocation>(), layout.align()) {
        Some(n) => n,
        None => {
            pmm::free(physical, pages).ok();
            return ptr::null_mut();
        }
    };
    // SAFETY: the metadata lies before the aligned user region, within the
    // exclusively allocated contiguous page span.
    unsafe {
        ((address - size_of::<LargeAllocation>()) as *mut LargeAllocation).write(LargeAllocation {
            magic: LARGE_MAGIC,
            physical,
            pages,
        });
    }
    address as *mut u8
}

pub(super) fn free_large(pointer: *mut u8) {
    // SAFETY: GlobalAlloc requires the matching layout and pointer, so this
    // metadata is exactly the header written by alloc_large.
    let header = unsafe {
        &mut *((pointer as usize - size_of::<LargeAllocation>()) as *mut LargeAllocation)
    };
    assert_eq!(header.magic, LARGE_MAGIC, "invalid large allocation");
    let physical = header.physical;
    let pages = header.pages;
    header.magic = 0;
    pmm::free(physical, pages).expect("large PMM free failed");
}
