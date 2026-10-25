/*
 *
 *       src/ipc/memfd.rs
 *       Anonymous in-memory files with Linux seals and MAP_SHARED
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Pages are owned by the `MemFd` and mapped into address spaces as external
//! mappings, so read/write and shared mappings observe the same physical
//! memory and fork keeps them shared.  Truncation never frees a page because
//! external mappings do not take per-page references; pages are released when
//! the last address-space owner drops.

use crate::{
    mm::pmm,
    sync::SpinLock,
    user::UserAddressSpace,
    vfs::{self, Inode, InodeKind, Stat, VfsError},
};
use alloc::{
    collections::BTreeMap,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};

pub const MFD_CLOEXEC: u32 = 1;
pub const MFD_ALLOW_SEALING: u32 = 2;
pub const MFD_HUGETLB: u32 = 4;

pub const F_SEAL_SEAL: u32 = 1;
pub const F_SEAL_SHRINK: u32 = 2;
pub const F_SEAL_GROW: u32 = 4;
pub const F_SEAL_WRITE: u32 = 8;
pub const F_SEAL_FUTURE_WRITE: u32 = 16;
pub const F_SEAL_EXEC: u32 = 32;

const PAGE_SIZE: u64 = 4096;
const MAX_SIZE: u64 = 256 * 1024 * 1024;

struct MemFdState {
    pages: BTreeMap<u64, u64>,
    size: u64,
    seals: u32,
    writable_maps: u32,
}

pub struct MemFd {
    state: SpinLock<MemFdState>,
    allow_sealing: bool,
    self_reference: SpinLock<Weak<MemFd>>,
    name: String,
    ino: u64,
}

impl MemFd {
    pub fn create(name: String, flags: u32) -> Result<(Arc<Self>, bool), VfsError> {
        if flags & !(MFD_CLOEXEC | MFD_ALLOW_SEALING) != 0 {
            return Err(VfsError::InvalidArgument);
        }
        if name.len() > 249 {
            return Err(VfsError::InvalidArgument);
        }
        let allow_sealing = flags & MFD_ALLOW_SEALING != 0;
        let memfd = Arc::new(Self {
            state: SpinLock::new(MemFdState {
                pages: BTreeMap::new(),
                size: 0,
                seals: if allow_sealing { 0 } else { F_SEAL_SEAL },
                writable_maps: 0,
            }),
            allow_sealing,
            self_reference: SpinLock::new(Weak::new()),
            name,
            ino: vfs::tree::ino(),
        });
        *memfd.self_reference.lock() = Arc::downgrade(&memfd);
        Ok((memfd, flags & MFD_CLOEXEC != 0))
    }

    pub fn seals(&self) -> u32 {
        self.state.lock().seals
    }

    pub fn add_seals(&self, seals: u32) -> Result<(), VfsError> {
        if !self.allow_sealing {
            return Err(VfsError::NotPermitted);
        }
        if seals
            & !(F_SEAL_SEAL
                | F_SEAL_SHRINK
                | F_SEAL_GROW
                | F_SEAL_WRITE
                | F_SEAL_FUTURE_WRITE
                | F_SEAL_EXEC)
            != 0
        {
            return Err(VfsError::InvalidArgument);
        }
        let mut state = self.state.lock();
        if state.seals & F_SEAL_SEAL != 0 {
            return Err(VfsError::NotPermitted);
        }
        // Existing writable shared mappings would bypass F_SEAL_WRITE, so
        // refuse until the address spaces are unmapped.  This kernel cannot
        // observe unmaps, so once mapped the seal stays unavailable.
        if seals & F_SEAL_WRITE != 0 && state.writable_maps != 0 {
            return Err(VfsError::Busy);
        }
        state.seals |= seals;
        Ok(())
    }

    fn ensure_page(state: &mut MemFdState, index: u64) -> Result<u64, VfsError> {
        if let Some(&frame) = state.pages.get(&index) {
            return Ok(frame);
        }
        let frame = pmm::alloc(1).ok_or(VfsError::NoMemory)?;
        // SAFETY: the frame is freshly allocated and unpublished.
        unsafe { core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, 4096) };
        state.pages.insert(index, frame);
        Ok(frame)
    }

    pub fn file_name(&self) -> String {
        alloc::format!("/memfd:{}", self.name)
    }
}

impl Inode for MemFd {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let size = self.state.lock().size;
        Ok(Stat::simple(self.ino, InodeKind::Regular, 0o600, size))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let state = self.state.lock();
        if offset >= state.size {
            return Ok(0);
        }
        let count = (state.size - offset).min(buf.len() as u64) as usize;
        for (index, slot) in buf[..count].iter_mut().enumerate() {
            let position = offset + index as u64;
            let page = position / PAGE_SIZE;
            let within = (position % PAGE_SIZE) as usize;
            *slot = if let Some(&frame) = state.pages.get(&page) {
                // SAFETY: the frame is owned by this inode and the offset is
                // within the page.
                unsafe { (pmm::phys_to_virt(frame) as *const u8).add(within).read() }
            } else {
                0
            };
        }
        Ok(count)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.state.lock();
        if state.seals & (F_SEAL_WRITE | F_SEAL_FUTURE_WRITE) != 0 {
            return Err(VfsError::NotPermitted);
        }
        let end = offset
            .checked_add(buf.len() as u64)
            .filter(|&end| end <= MAX_SIZE)
            .ok_or(VfsError::NoSpace)?;
        if state.seals & F_SEAL_GROW != 0 && end > state.size {
            return Err(VfsError::NotPermitted);
        }
        for (index, &byte) in buf.iter().enumerate() {
            let position = offset + index as u64;
            let page = position / PAGE_SIZE;
            let within = (position % PAGE_SIZE) as usize;
            let frame = Self::ensure_page(&mut state, page)?;
            // SAFETY: the frame is owned by this inode.
            unsafe {
                (pmm::phys_to_virt(frame) as *mut u8)
                    .add(within)
                    .write(byte);
            }
        }
        state.size = state.size.max(end);
        Ok(buf.len())
    }

    fn truncate(&self, size: u64) -> Result<(), VfsError> {
        let mut state = self.state.lock();
        if size > MAX_SIZE {
            return Err(VfsError::NoSpace);
        }
        if size < state.size && state.seals & F_SEAL_SHRINK != 0 {
            return Err(VfsError::NotPermitted);
        }
        if size > state.size && state.seals & F_SEAL_GROW != 0 {
            return Err(VfsError::NotPermitted);
        }
        state.size = size;
        Ok(())
    }

    fn mmap(
        &self,
        space: &mut UserAddressSpace,
        address: u64,
        length: u64,
        offset: u64,
        prot: u32,
    ) -> Result<(), VfsError> {
        if offset & 4095 != 0 {
            return Err(VfsError::InvalidArgument);
        }
        let owner: Arc<dyn Inode> = self
            .self_reference
            .lock()
            .upgrade()
            .ok_or(VfsError::NoDevice)?;
        let writable = prot & crate::user::address_space::PROT_WRITE != 0;
        let mut state = self.state.lock();
        if prot & crate::user::address_space::PROT_EXEC != 0 && state.seals & F_SEAL_EXEC != 0 {
            return Err(VfsError::NotPermitted);
        }
        if writable && state.seals & (F_SEAL_WRITE | F_SEAL_FUTURE_WRITE) != 0 {
            return Err(VfsError::NotPermitted);
        }
        let mut mappings = Vec::new();
        let mut delta = 0u64;
        while delta < length {
            let file_offset = offset.saturating_add(delta);
            if file_offset >= state.size {
                break;
            }
            let page = file_offset / PAGE_SIZE;
            let frame = Self::ensure_page(&mut state, page)?;
            mappings.push((address + delta, frame));
            delta += PAGE_SIZE;
        }
        drop(state);
        for (index, (target, frame)) in mappings.iter().enumerate() {
            if space
                .map_external(*target, *frame, prot, owner.clone())
                .is_err()
            {
                for (mapped, _) in &mappings[..index] {
                    let _ = space.unmap(*mapped, PAGE_SIZE);
                }
                return Err(VfsError::NoMemory);
            }
        }
        if writable {
            let mut state = self.state.lock();
            state.writable_maps = state.writable_maps.saturating_add(1);
        }
        Ok(())
    }
}

impl Drop for MemFd {
    fn drop(&mut self) {
        let state = self.state.lock();
        for frame in state.pages.values() {
            let _ = pmm::free(*frame, 1);
        }
    }
}

pub fn instance(fd: u64) -> Result<Arc<MemFd>, VfsError> {
    if fd > i32::MAX as u64 {
        return Err(VfsError::BadFileDescriptor);
    }
    let inode = vfs::current_fd_table()
        .lock()
        .get(fd as usize)
        .ok_or(VfsError::BadFileDescriptor)?
        .file
        .inode
        .clone();
    let any: Arc<dyn core::any::Any + Send + Sync> = inode;
    any.downcast::<MemFd>()
        .map_err(|_| VfsError::InvalidArgument)
}
