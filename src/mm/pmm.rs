use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::SpinLock;
use limine::memmap::{Entry, MEMMAP_USABLE};

pub const PAGE_SIZE: u64 = 4096;
static HHDM: AtomicU64 = AtomicU64::new(0);
static PMM: SpinLock<State> = SpinLock::new(State::empty());

struct State {
    used_addr: usize,
    usable_addr: usize,
    frames: usize,
    free: usize,
    hint: usize,
}

impl State {
    const fn empty() -> Self {
        Self {
            used_addr: 0,
            usable_addr: 0,
            frames: 0,
            free: 0,
            hint: 0,
        }
    }

    fn bit(&self, base: usize, frame: usize) -> bool {
        // SAFETY: the bitmap storage is permanently reserved, and the lock
        // serializes every access to it.
        unsafe { (*(base as *const u8).add(frame / 8) & (1 << (frame % 8))) != 0 }
    }

    fn set_bit(&self, base: usize, frame: usize, value: bool) {
        // SAFETY: as above; frame is always bounded by self.frames.
        unsafe {
            let byte = (base as *mut u8).add(frame / 8);
            let mask = 1 << (frame % 8);
            let old = byte.read();
            byte.write(if value { old | mask } else { old & !mask });
        }
    }

    fn free_frame(&self, frame: usize) -> bool {
        self.bit(self.usable_addr, frame) && !self.bit(self.used_addr, frame)
    }
}

pub struct Stats {
    pub frames: usize,
    pub free: usize,
}

pub fn init(entries: &[&Entry], hhdm: u64) {
    let mut state = PMM.lock();
    assert_eq!(state.frames, 0, "PMM initialized twice");
    let max_end = entries
        .iter()
        .filter(|e| e.type_ == MEMMAP_USABLE)
        .filter_map(|e| e.base.checked_add(e.length))
        .max()
        .expect("no usable RAM");
    let frames = max_end.div_ceil(PAGE_SIZE) as usize;
    let bitmap_bytes = frames.div_ceil(8);
    let bitmap_pages = ((bitmap_bytes * 2) as u64).div_ceil(PAGE_SIZE);
    let bitmap_phys = entries
        .iter()
        .filter(|e| e.type_ == MEMMAP_USABLE)
        .find_map(|e| {
            let first = e.base.max(PAGE_SIZE).div_ceil(PAGE_SIZE) * PAGE_SIZE;
            let end = e.base.checked_add(e.length)?;
            (first.checked_add(bitmap_pages * PAGE_SIZE)? <= end).then_some(first)
        })
        .expect("no room for PMM bitmaps");
    let bitmap_virt = hhdm.checked_add(bitmap_phys).expect("HHDM overflow") as usize;
    // SAFETY: the chosen pages are usable and hence present in Limine's HHDM;
    // they are reserved before the allocator is published to other CPUs.
    unsafe {
        core::ptr::write_bytes(bitmap_virt as *mut u8, 0xff, bitmap_bytes);
        core::ptr::write_bytes((bitmap_virt + bitmap_bytes) as *mut u8, 0, bitmap_bytes);
    }
    state.used_addr = bitmap_virt;
    state.usable_addr = bitmap_virt + bitmap_bytes;
    state.frames = frames;
    for entry in entries.iter().filter(|e| e.type_ == MEMMAP_USABLE) {
        let start = entry.base.max(PAGE_SIZE).div_ceil(PAGE_SIZE) as usize;
        let end = ((entry.base + entry.length) / PAGE_SIZE) as usize;
        for frame in start..end.min(frames) {
            state.set_bit(state.usable_addr, frame, true);
            state.set_bit(state.used_addr, frame, false);
            state.free += 1;
        }
    }
    for frame in
        (bitmap_phys / PAGE_SIZE) as usize..((bitmap_phys / PAGE_SIZE) + bitmap_pages) as usize
    {
        state.set_bit(state.used_addr, frame, true);
        state.free -= 1;
    }
    state.hint = (bitmap_phys / PAGE_SIZE + bitmap_pages) as usize;
    HHDM.store(hhdm, Ordering::Release);
}

pub fn hhdm() -> u64 {
    HHDM.load(Ordering::Acquire)
}

pub fn phys_to_virt(physical: u64) -> usize {
    hhdm().checked_add(physical).expect("HHDM overflow") as usize
}

pub fn stats() -> Stats {
    let state = PMM.lock();
    Stats {
        frames: state.frames,
        free: state.free,
    }
}

pub fn alloc(pages: usize) -> Option<u64> {
    if pages == 0 {
        return None;
    }
    let mut state = PMM.lock();
    if state.frames == 0 || pages > state.free {
        return None;
    }
    for pass in 0..2 {
        let (start, end) = if pass == 0 {
            (state.hint, state.frames)
        } else {
            (1, state.hint)
        };
        let mut run = 0;
        for frame in start..end {
            if state.free_frame(frame) {
                run += 1;
            } else {
                run = 0;
            }
            if run == pages {
                let first = frame + 1 - pages;
                for index in first..=frame {
                    state.set_bit(state.used_addr, index, true);
                }
                state.free -= pages;
                state.hint = (frame + 1).min(state.frames);
                return Some(first as u64 * PAGE_SIZE);
            }
        }
    }
    None
}

pub fn free(physical: u64, pages: usize) -> Result<(), &'static str> {
    if physical % PAGE_SIZE != 0 || pages == 0 {
        return Err("invalid frame span");
    }
    let first = (physical / PAGE_SIZE) as usize;
    let mut state = PMM.lock();
    if first == 0
        || first
            .checked_add(pages)
            .is_none_or(|end| end > state.frames)
    {
        return Err("frame span out of bounds");
    }
    for frame in first..first + pages {
        if !state.bit(state.usable_addr, frame) || !state.bit(state.used_addr, frame) {
            return Err("frame not allocated");
        }
    }
    for frame in first..first + pages {
        state.set_bit(state.used_addr, frame, false);
    }
    state.free += pages;
    state.hint = state.hint.min(first);
    Ok(())
}
