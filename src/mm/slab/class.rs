//! Per-class page lists and slot accounting.

use super::align_up;
use crate::{
    mm::pmm::{self, PAGE_SIZE},
    sync::SpinLock,
};
use core::{alloc::Layout, mem::size_of, ptr};

const CLASS_SIZES: [usize; 7] = [32, 64, 128, 256, 512, 1024, 2048];
const EMPTY: u16 = u16::MAX;
const SLAB_MAGIC: u64 = 0x534c_4142_5041_4745;

#[repr(C)]
struct SlabPage {
    magic: u64,
    next: u64,
    allocated: u128,
    class: u16,
    head: u16,
    live: u16,
    capacity: u16,
}

struct Class {
    head: u64,
}

static CLASSES: [SpinLock<Class>; CLASS_SIZES.len()] =
    [const { SpinLock::new(Class { head: 0 }) }; CLASS_SIZES.len()];

pub(super) fn class_for(layout: Layout) -> Option<usize> {
    let needed = layout.size().max(layout.align()).max(CLASS_SIZES[0]);
    CLASS_SIZES.iter().position(|&size| size >= needed)
}

fn object_offset(class_size: usize) -> usize {
    align_up(size_of::<SlabPage>(), class_size).unwrap()
}

fn page_header(physical: u64) -> *mut SlabPage {
    pmm::phys_to_virt(physical) as *mut SlabPage
}

fn slot(physical: u64, class_size: usize, index: u16) -> *mut u8 {
    (pmm::phys_to_virt(physical) + object_offset(class_size) + index as usize * class_size)
        as *mut u8
}

fn new_slab(class_index: usize, next: u64) -> Option<u64> {
    let physical = pmm::alloc(1)?;
    let size = CLASS_SIZES[class_index];
    let capacity = ((PAGE_SIZE as usize - object_offset(size)) / size) as u16;
    assert!(capacity <= 128);
    // SAFETY: PMM transferred exclusive ownership of this page. The class
    // lock is held, so no other CPU can publish or touch it yet.
    unsafe {
        page_header(physical).write(SlabPage {
            magic: SLAB_MAGIC,
            next,
            allocated: 0,
            class: class_index as u16,
            head: 0,
            live: 0,
            capacity,
        });
        for index in 0..capacity {
            let following = if index + 1 == capacity {
                EMPTY
            } else {
                index + 1
            };
            (slot(physical, size, index) as *mut u16).write_unaligned(following);
        }
    }
    Some(physical)
}

pub(super) fn alloc_small(class_index: usize) -> *mut u8 {
    let mut class = CLASSES[class_index].lock();
    let mut physical = class.head;
    while physical != 0 {
        // SAFETY: class lock protects all headers and free lists in this class.
        let page = unsafe { &mut *page_header(physical) };
        assert_eq!(page.magic, SLAB_MAGIC);
        if page.head != EMPTY {
            break;
        }
        physical = page.next;
    }
    if physical == 0 {
        physical = match new_slab(class_index, class.head) {
            Some(p) => p,
            None => return ptr::null_mut(),
        };
        class.head = physical;
    }
    // SAFETY: the selected page belongs to this locked class; head names a
    // free slot whose first two bytes contain the following slot index.
    unsafe {
        let page = &mut *page_header(physical);
        let index = page.head;
        assert!(index < page.capacity);
        let object = slot(physical, CLASS_SIZES[class_index], index);
        page.head = (object as *const u16).read_unaligned();
        page.allocated |= 1u128 << index;
        page.live += 1;
        object
    }
}

pub(super) fn free_small(pointer: *mut u8, class_index: usize) {
    let hhdm = pmm::hhdm() as usize;
    let physical = (pointer as usize)
        .checked_sub(hhdm)
        .expect("slab pointer outside HHDM") as u64;
    let page_physical = physical & !(PAGE_SIZE - 1);
    let mut class = CLASSES[class_index].lock();
    // SAFETY: valid GlobalAlloc deallocation gives an owned pointer and its
    // original layout. The class lock excludes concurrent slot-list edits.
    unsafe {
        let page = &mut *page_header(page_physical);
        assert_eq!(page.magic, SLAB_MAGIC, "invalid slab page");
        assert_eq!(page.class as usize, class_index, "wrong slab class");
        let start = object_offset(CLASS_SIZES[class_index]) as u64;
        let offset = physical - page_physical;
        assert!(offset >= start);
        let relative = (offset - start) as usize;
        assert_eq!(
            relative % CLASS_SIZES[class_index],
            0,
            "unaligned slab pointer"
        );
        let index = (relative / CLASS_SIZES[class_index]) as u16;
        assert!(index < page.capacity);
        assert!(page.allocated & (1u128 << index) != 0, "double free");
        page.allocated &= !(1u128 << index);
        (pointer as *mut u16).write_unaligned(page.head);
        page.head = index;
        page.live -= 1;
        if page.live == 0 {
            // Remove the empty page before returning it to PMM. Otherwise a
            // concurrent allocation could follow a dangling page link.
            let next = page.next;
            if class.head == page_physical {
                class.head = next;
            } else {
                let mut predecessor = class.head;
                while predecessor != 0 && (*page_header(predecessor)).next != page_physical {
                    predecessor = (*page_header(predecessor)).next;
                }
                assert_ne!(predecessor, 0, "slab list corruption");
                (*page_header(predecessor)).next = next;
            }
            page.magic = 0;
            pmm::free(page_physical, 1).expect("slab PMM free failed");
        }
    }
}
