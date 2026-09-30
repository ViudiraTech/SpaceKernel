/*
 *
 *       src/mm/vmm.rs
 *       Geometry-aware kernel virtual reservations and page mappings
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
    arch,
    mm::pmm::{self, PAGE_SIZE},
    sync::SpinLock,
};

const ENTRIES_PER_TABLE: usize = 512;
static READY: AtomicBool = AtomicBool::new(false);
static VMM: SpinLock<State> = SpinLock::new(State::empty());

struct State {
    root: u64,
    base: u64,
    next: u64,
    limit: u64,
    geometry: arch::paging::Geometry,
}

impl State {
    const fn empty() -> Self {
        Self {
            root: 0,
            base: 0,
            next: 0,
            limit: 0,
            geometry: arch::paging::Geometry {
                levels: 4,
                virtual_bits: 48,
                physical_bits: 48,
            },
        }
    }
}

/// An exclusively reserved subrange of an architecture-sized kernel slot.
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
    InvalidPhysicalAddress,
    MalformedTable,
}

pub fn init() {
    let mut state = VMM.lock();
    assert_eq!(state.root, 0, "VMM initialized twice");
    let geometry = arch::paging_geometry();
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
    let base = geometry.slot_base(slot);
    state.geometry = geometry;
    state.root = root;
    state.base = base;
    state.next = base;
    state.limit = base
        .checked_add(geometry.slot_bytes())
        .expect("virtual slot overflow");
    #[cfg(target_arch = "aarch64")]
    arch::setup_device_memory();
    #[cfg(target_arch = "x86_64")]
    arch::setup_memory_protection();
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
    if !physical.is_multiple_of(PAGE_SIZE) {
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
    if !state.geometry.valid_physical(physical) {
        return Err(MapError::InvalidPhysicalAddress);
    }
    let mut table = state.root;
    // Build missing descendants privately, then publish a single link only
    // after every allocation succeeds. OOM leaves no visible partial mapping.
    for level in (1..state.geometry.levels).rev() {
        let entry = entry_ptr(table, virtual_address, level);
        // SAFETY: VMM owns this hierarchy and its lock excludes PTE writers.
        let value = unsafe { entry.read_volatile() };
        if arch::pte_present(value) {
            if !arch::pte_is_table(value, level) {
                return Err(MapError::MalformedTable);
            }
            table = arch::pte_phys(value);
            continue;
        }
        let mut frames = [0u64; 3];
        for i in 0..level {
            let Some(frame) = pmm::alloc(1) else {
                for &allocated in &frames[..i] {
                    pmm::free(allocated, 1).expect("page table rollback failed");
                }
                return Err(MapError::OutOfMemory);
            };
            frames[i] = frame;
            // SAFETY: the frame is private until the parent link is published.
            unsafe {
                core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, PAGE_SIZE as usize)
            };
        }
        for i in 0..level - 1 {
            // SAFETY: link private, initialized descendant tables.
            unsafe {
                entry_ptr(frames[i], virtual_address, level - i - 1)
                    .write_volatile(arch::pte_table(frames[i + 1]))
            };
        }
        // SAFETY: the leaf is in a fresh, exclusively owned table.
        unsafe {
            entry_ptr(frames[level - 1], virtual_address, 0)
                .write_volatile(arch::pte_leaf(physical, device))
        };
        core::sync::atomic::fence(Ordering::Release);
        // SAFETY: publish the completed hierarchy while holding the VMM lock.
        unsafe { entry.write_volatile(arch::pte_table(frames[0])) };
        arch::flush_table(virtual_address);
        return Ok(());
    }
    let leaf = entry_ptr(table, virtual_address, 0);
    // SAFETY: the existing leaf table is protected by the VMM lock.
    if arch::pte_present(unsafe { leaf.read_volatile() }) {
        return Err(MapError::AlreadyMapped);
    }
    // SAFETY: exclusively publish this newly populated page.
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
    for level in (1..state.geometry.levels).rev() {
        let value = unsafe { entry_ptr(table, virtual_address, level).read_volatile() };
        if !arch::pte_is_table(value, level) {
            return None;
        }
        table = arch::pte_phys(value);
    }
    let leaf = unsafe { entry_ptr(table, virtual_address, 0).read_volatile() };
    arch::pte_present(leaf).then(|| arch::pte_phys(leaf) + virtual_address % PAGE_SIZE)
}

pub fn map_device_page(physical: u64) -> Result<u64, MapError> {
    if !arch::paging_geometry().valid_physical(physical) {
        return Err(MapError::InvalidPhysicalAddress);
    }
    let region = reserve(1)?;
    map(region, 0, physical & !(PAGE_SIZE - 1), true)?;
    Ok(region.base + physical % PAGE_SIZE)
}

pub fn map_device_range(physical: u64, length: usize) -> Result<u64, MapError> {
    if length == 0 {
        return Err(MapError::OutOfRange);
    }
    let offset = physical % PAGE_SIZE;
    let base_phys = physical & !(PAGE_SIZE - 1);
    let end = physical
        .checked_add(length as u64 - 1)
        .ok_or(MapError::OutOfRange)?;
    if !arch::paging_geometry().valid_physical(end) {
        return Err(MapError::InvalidPhysicalAddress);
    }
    let span = (length as u64)
        .checked_add(offset)
        .ok_or(MapError::OutOfRange)?;
    let pages = span.div_ceil(PAGE_SIZE) as usize;
    let region = reserve(pages)?;
    for i in 0..pages {
        map(region, i, base_phys + (i as u64 * PAGE_SIZE), true)?;
    }
    Ok(region.base + offset)
}

fn entry_ptr(table: u64, virtual_address: u64, level: usize) -> *mut u64 {
    let shift = 12 + level * 9;
    let index = ((virtual_address >> shift) & 0x1ff) as usize;
    (pmm::phys_to_virt(table) as *mut u64).wrapping_add(index)
}
