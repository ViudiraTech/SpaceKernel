use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
    arch,
    mm::pmm::{self, PAGE_SIZE},
    sync::SpinLock,
};

const ENTRIES_PER_TABLE: usize = 512;
const SLOT_BYTES: u64 = 1 << 39;
static READY: AtomicBool = AtomicBool::new(false);
static VMM: SpinLock<State> = SpinLock::new(State::empty());

struct State {
    root: u64,
    base: u64,
    next: u64,
    limit: u64,
}

impl State {
    const fn empty() -> Self {
        Self {
            root: 0,
            base: 0,
            next: 0,
            limit: 0,
        }
    }
}

/// An exclusively reserved subrange of the kernel's 512 GiB mapping slot.
/// Each page can be populated once. Keeping the mapping monotonic avoids
/// cross-CPU TLB invalidation until an IPI shootdown implementation exists.
#[derive(Clone, Copy)]
pub struct Region {
    base: u64,
    pages: usize,
}

impl Region {
    pub fn base(&self) -> u64 {
        self.base
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    NotReady,
    OutOfMemory,
    OutOfRange,
    AlreadyMapped,
    Unaligned,
}

pub fn init() {
    let mut state = VMM.lock();
    assert_eq!(state.root, 0, "VMM initialized twice");
    let boot_root = arch::page_root();
    let root = pmm::alloc(1).expect("root page table allocation failed");
    let source = pmm::phys_to_virt(boot_root) as *const u64;
    let target = pmm::phys_to_virt(root) as *mut u64;
    // SAFETY: source is the active Limine root and target is a new page frame;
    // their 4096-byte ranges cannot overlap.
    unsafe { core::ptr::copy_nonoverlapping(source, target, ENTRIES_PER_TABLE) }
    let slot = (256..511)
        .find(|&index| {
            // SAFETY: the copied root frame contains exactly 512 entries.
            unsafe { target.add(index).read_volatile() == 0 }
        })
        .expect("no free kernel virtual slot");
    let base = 0xffff_0000_0000_0000u64 | ((slot as u64) << 39);
    state.root = root;
    state.base = base;
    state.next = base;
    state.limit = base + SLOT_BYTES;
    #[cfg(target_arch = "aarch64")]
    arch::setup_device_memory();
    arch::set_page_root(root);
    READY.store(true, Ordering::Release);
}

pub fn root() -> Option<u64> {
    READY.load(Ordering::Acquire).then(|| VMM.lock().root)
}

pub fn reserve(pages: usize) -> Result<Region, MapError> {
    if pages == 0 {
        return Err(MapError::OutOfRange);
    }
    let mut state = VMM.lock();
    if state.root == 0 {
        return Err(MapError::NotReady);
    }
    let length = (pages as u64)
        .checked_mul(PAGE_SIZE)
        .ok_or(MapError::OutOfRange)?;
    let end = state.next.checked_add(length).ok_or(MapError::OutOfRange)?;
    if end > state.limit {
        return Err(MapError::OutOfRange);
    }
    let region = Region {
        base: state.next,
        pages,
    };
    state.next = end;
    Ok(region)
}

pub fn map(region: Region, index: usize, physical: u64, device: bool) -> Result<(), MapError> {
    if index >= region.pages {
        return Err(MapError::OutOfRange);
    }
    if physical % PAGE_SIZE != 0 {
        return Err(MapError::Unaligned);
    }
    let virtual_address = region.base + index as u64 * PAGE_SIZE;
    let state = VMM.lock();
    if state.root == 0 {
        return Err(MapError::NotReady);
    }
    if virtual_address < state.base || virtual_address >= state.next {
        return Err(MapError::OutOfRange);
    }
    let mut table = state.root;
    for level in (1..=3).rev() {
        let entry = entry_ptr(table, virtual_address, level);
        // SAFETY: lock serializes writes; table is mapped through the HHDM.
        let value = unsafe { entry.read_volatile() };
        table = if arch::pte_present(value) {
            arch::pte_phys(value)
        } else {
            let new_table = pmm::alloc(1).ok_or(MapError::OutOfMemory)?;
            // SAFETY: the new frame is exclusively owned by the page table.
            unsafe {
                core::ptr::write_bytes(
                    pmm::phys_to_virt(new_table) as *mut u8,
                    0,
                    PAGE_SIZE as usize,
                )
            }
            unsafe { entry.write_volatile(arch::pte_table(new_table)) }
            new_table
        };
    }
    let leaf = entry_ptr(table, virtual_address, 0);
    if arch::pte_present(unsafe { leaf.read_volatile() }) {
        return Err(MapError::AlreadyMapped);
    }
    unsafe { leaf.write_volatile(arch::pte_leaf(physical, device)) }
    arch::flush_page(virtual_address);
    Ok(())
}

pub fn translate(virtual_address: u64) -> Option<u64> {
    let state = VMM.lock();
    if state.root == 0 || virtual_address < state.base || virtual_address >= state.next {
        return None;
    }
    let mut table = state.root;
    for level in (1..=3).rev() {
        let value = unsafe { entry_ptr(table, virtual_address, level).read_volatile() };
        if !arch::pte_present(value) {
            return None;
        }
        table = arch::pte_phys(value);
    }
    let leaf = unsafe { entry_ptr(table, virtual_address, 0).read_volatile() };
    arch::pte_present(leaf).then(|| arch::pte_phys(leaf) + virtual_address % PAGE_SIZE)
}

#[cfg(not(target_arch = "x86_64"))]
pub fn map_device_page(physical: u64) -> Result<u64, MapError> {
    let region = reserve(1)?;
    map(region, 0, physical & !(PAGE_SIZE - 1), true)?;
    Ok(region.base + physical % PAGE_SIZE)
}

fn entry_ptr(table: u64, virtual_address: u64, level: usize) -> *mut u64 {
    let shift = 12 + level * 9;
    let index = ((virtual_address >> shift) & 0x1ff) as usize;
    (pmm::phys_to_virt(table) as *mut u64).wrapping_add(index)
}
