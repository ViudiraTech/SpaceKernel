/*
 *
 *       src/user/address_space.rs
 *       Isolated user virtual address space management and translation tables
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Architecture-aware user virtual address space management.
//!
//! Kernel higher-half tables are shared across all address spaces, while the
//! canonical lower-half is private to each user process.

use crate::vfs::{Inode, VfsError};
use crate::{
    arch,
    mm::{pmm, vmm},
};
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};

pub const PAGE_SIZE: u64 = 4096;
pub const PROT_READ: u32 = 1;
pub const PROT_WRITE: u32 = 2;
pub const PROT_EXEC: u32 = 4;
const MAX_MAPPING: u64 = 256 * 1024 * 1024;

struct Mapping {
    physical: u64,
    kernel: u64,
    uncached: bool,
    prot: u32,
    max_prot: u32,
    owned: bool,
    owner: Option<Arc<dyn Inode>>,
}

#[derive(Clone)]
pub struct ProcessImage {
    pub executable: String,
    pub argv: Vec<u8>,
    pub environ: Vec<u8>,
    pub auxv: Vec<u8>,
}

pub struct UserAddressSpace {
    root_phys: u64,
    allocated_tables: Vec<u64>,
    mapped_frames: BTreeMap<u64, Mapping>,
    pub brk_base: u64,
    pub brk: u64,
    pub image: Option<ProcessImage>,
}

unsafe impl Send for UserAddressSpace {}
unsafe impl Sync for UserAddressSpace {}

impl UserAddressSpace {
    pub fn new() -> Result<Self, &'static str> {
        let root_phys = pmm::alloc(1).ok_or("failed to allocate user root page table")?;
        let root_virt = pmm::phys_to_virt(root_phys) as *mut u64;

        // Zero the entire 4 KiB root table frame
        unsafe {
            core::ptr::write_bytes(root_virt, 0, 512);
        }

        // On x86_64 and RISC-V, copy the kernel's top-half mappings into entries 256..512
        #[cfg(not(target_arch = "aarch64"))]
        {
            let kernel_root = vmm::root().ok_or("kernel VMM root unavailable")?;
            let kernel_virt = pmm::phys_to_virt(kernel_root) as *const u64;
            unsafe {
                core::ptr::copy_nonoverlapping(kernel_virt.add(256), root_virt.add(256), 256);
            }
        }

        Ok(Self {
            root_phys,
            allocated_tables: Vec::new(),
            mapped_frames: BTreeMap::new(),
            brk_base: 0x2000_0000,
            brk: 0x2000_0000,
            image: None,
        })
    }

    pub fn root_physical(&self) -> u64 {
        self.root_phys
    }

    /// Activate this address space on the current CPU.
    pub fn activate(&self) {
        #[cfg(target_arch = "x86_64")]
        {
            arch::set_page_root(self.root_phys);
        }
        #[cfg(target_arch = "aarch64")]
        {
            // On AArch64, user addresses (lower half) are translated via TTBR0_EL1
            unsafe {
                let mut tcr: u64;
                core::arch::asm!("mrs {}, tcr_el1", out(reg) tcr, options(nomem, nostack));
                let t1sz = (tcr >> 16) & 0x3f;
                tcr &=
                    !(0x3f | (1 << 7) | (0b11 << 8) | (0b11 << 10) | (0b11 << 12) | (0b11 << 14));
                tcr |= t1sz | (0b01 << 8) | (0b01 << 10) | (0b11 << 12);
                core::arch::asm!(
                    "dsb ishst",
                    "msr tcr_el1, {tcr}",
                    "msr ttbr0_el1, {root}",
                    "isb",
                    "tlbi vmalle1is",
                    "dsb ish",
                    "isb",
                    tcr = in(reg) tcr,
                    root = in(reg) self.root_phys,
                    options(nostack)
                );
            }
        }
        #[cfg(target_arch = "riscv64")]
        {
            arch::set_page_root(self.root_phys);
        }
    }

    /// Restore the kernel-only page root on the current CPU.
    pub fn deactivate() {
        if let Some(_kernel_root) = vmm::root() {
            #[cfg(target_arch = "x86_64")]
            {
                arch::set_page_root(_kernel_root);
            }
            #[cfg(target_arch = "aarch64")]
            {
                unsafe {
                    core::arch::asm!(
                        "dsb ishst",
                        "msr ttbr0_el1, {zero}",
                        "isb",
                        "tlbi vmalle1is",
                        "dsb ish",
                        "isb",
                        zero = in(reg) 0u64,
                        options(nostack)
                    );
                }
            }
            #[cfg(target_arch = "riscv64")]
            {
                arch::set_page_root(_kernel_root);
            }
        }
    }

    /// Maximum exclusive user address. Upper-half kernel tables are never walked.
    pub fn user_limit() -> u64 {
        1u64 << (arch::paging_geometry().virtual_bits - 1)
    }

    pub fn valid_range(address: u64, length: usize) -> bool {
        address >= PAGE_SIZE
            && address
                .checked_add(length as u64)
                .is_some_and(|end| end <= Self::user_limit())
    }

    fn leaf(&mut self, address: u64, create: bool) -> Result<*mut u64, &'static str> {
        if address % PAGE_SIZE != 0 || !Self::valid_range(address, PAGE_SIZE as usize) {
            return Err("invalid user page address");
        }
        let mut table = self.root_phys;
        for level in (1..arch::paging_geometry().levels).rev() {
            let index = ((address >> (12 + level * 9)) & 511) as usize;
            // SAFETY: table is an owned page-table frame, index is within 512 entries.
            let ptr = unsafe { (pmm::phys_to_virt(table) as *mut u64).add(index) };
            let value = unsafe { ptr.read_volatile() };
            if arch::pte_present(value) {
                if !arch::pte_is_table(value, level) {
                    return Err("unexpected user huge page");
                }
                table = arch::pte_phys(value);
            } else if create {
                let frame = pmm::alloc(1).ok_or("out of memory for page table")?;
                // SAFETY: newly allocated frame, not yet published to hardware.
                unsafe { core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, 4096) };
                self.allocated_tables.push(frame);
                unsafe { ptr.write_volatile(make_user_table_entry(frame)) };
                table = frame;
            } else {
                return Err("missing user page table");
            }
        }
        // SAFETY: final table has 512 entries; address was validated above.
        Ok(unsafe { (pmm::phys_to_virt(table) as *mut u64).add(((address >> 12) & 511) as usize) })
    }

    /// Takes ownership of `physical` only on success. Never overwrites a mapping.
    pub fn map_page(
        &mut self,
        address: u64,
        physical: u64,
        writable: bool,
        executable: bool,
    ) -> Result<(), &'static str> {
        self.install(
            address,
            physical,
            PROT_READ
                | if writable { PROT_WRITE } else { 0 }
                | if executable { PROT_EXEC } else { 0 },
            true,
            None,
        )
    }

    fn install(
        &mut self,
        address: u64,
        physical: u64,
        prot: u32,
        owned: bool,
        owner: Option<Arc<dyn Inode>>,
    ) -> Result<(), &'static str> {
        if physical % PAGE_SIZE != 0
            || !arch::paging_geometry().valid_physical(physical)
            || prot & !7 != 0
            || self.mapped_frames.contains_key(&address)
        {
            return Err("invalid or overlapping mapping");
        }
        let leaf = self.leaf(address, true)?;
        let value = user_leaf(physical, prot);
        // SAFETY: exclusively owned leaf; frame is retained in mapped_frames.
        unsafe { leaf.write_volatile(value) };
        self.mapped_frames.insert(
            address,
            Mapping {
                physical,
                kernel: pmm::phys_to_virt(physical) as u64,
                uncached: false,
                prot,
                max_prot: if owner.is_some() { 3 } else { 7 },
                owned,
                owner,
            },
        );
        arch::flush_page(address);
        Ok(())
    }

    /// Retain the backing inode until all corresponding user pages are unmapped.
    pub fn map_external(
        &mut self,
        address: u64,
        physical: u64,
        prot: u32,
        owner: Arc<dyn Inode>,
    ) -> Result<(), &'static str> {
        self.install(address, physical, prot, false, Some(owner))
    }

    pub fn map_external_at(
        &mut self,
        address: u64,
        physical: u64,
        kernel: u64,
        prot: u32,
        owner: Arc<dyn Inode>,
        uncached: bool,
    ) -> Result<(), &'static str> {
        self.install(address, physical, prot, false, Some(owner))?;
        let mapping = self.mapped_frames.get_mut(&address).unwrap();
        mapping.kernel = kernel;
        mapping.uncached = uncached;
        let leaf = self.leaf(address, false)?;
        let value = external_leaf(physical, prot, uncached);
        unsafe { leaf.write_volatile(value) };
        arch::flush_page(address);
        Ok(())
    }

    pub fn contains(&self, address: u64) -> bool {
        self.mapped_frames.contains_key(&(address & !4095))
    }

    pub fn find_free(&self, hint: u64, length: u64) -> Result<u64, VfsError> {
        let length = align_up(length).ok_or(VfsError::InvalidArgument)?;
        if length == 0 || length > MAX_MAPPING {
            return Err(VfsError::NoSpace);
        }
        let mut address = align_up(hint.max(0x4000_0000)).ok_or(VfsError::NoSpace)?;
        for (&page, _) in self.mapped_frames.range(address..) {
            if page >= address.checked_add(length).ok_or(VfsError::NoSpace)? {
                break;
            }
            address = page + PAGE_SIZE;
        }
        if !Self::valid_range(address, length as usize) {
            return Err(VfsError::NoSpace);
        }
        Ok(address)
    }

    pub fn map_zeroed(&mut self, address: u64, length: u64, prot: u32) -> Result<(), &'static str> {
        let length = align_up(length).ok_or("mapping size overflow")?;
        if length == 0
            || length > MAX_MAPPING
            || prot & !7 != 0
            || address & 4095 != 0
            || !Self::valid_range(address, length as usize)
            || self
                .mapped_frames
                .range(address..address + length)
                .next()
                .is_some()
        {
            return Err("invalid or overlapping user mapping");
        }
        let mut done = 0;
        while done < length {
            let Some(frame) = pmm::alloc(1) else {
                let _ = self.unmap(address, done);
                return Err("out of memory for user data");
            };
            // SAFETY: unpublished fresh frame, exactly one page.
            unsafe { core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, 4096) };
            if let Err(e) = self.install(address + done, frame, prot, true, None) {
                let _ = pmm::free(frame, 1);
                let _ = self.unmap(address, done);
                return Err(e);
            }
            done += PAGE_SIZE;
        }
        Ok(())
    }

    /// Validate the entire range before copying; the caller holds the space lock.
    pub fn validate(&self, address: u64, length: usize, write: bool) -> Result<(), VfsError> {
        if length == 0 {
            return Ok(());
        }
        if !Self::valid_range(address, length) {
            return Err(VfsError::Fault);
        }
        let end = address + length as u64;
        let mut page = address & !4095;
        while page < end {
            let m = self.mapped_frames.get(&page).ok_or(VfsError::Fault)?;
            if m.prot & if write { PROT_WRITE } else { PROT_READ } == 0 {
                return Err(VfsError::Fault);
            }
            page += PAGE_SIZE;
        }
        Ok(())
    }

    pub fn read_bytes(&self, address: u64, dst: &mut [u8]) -> Result<(), VfsError> {
        self.validate(address, dst.len(), false)?;
        self.copy_pages(address, dst.as_mut_ptr(), dst.len(), false)
    }

    pub fn write_bytes(&self, address: u64, src: &[u8]) -> Result<(), VfsError> {
        self.validate(address, src.len(), true)?;
        self.copy_pages(address, src.as_ptr() as *mut u8, src.len(), true)
    }

    /// Loader-only initialization through HHDM, without making RX mappings writable.
    pub fn initialize(&self, address: u64, src: &[u8]) -> Result<(), VfsError> {
        if !Self::valid_range(address, src.len()) {
            return Err(VfsError::Fault);
        }
        self.copy_pages(address, src.as_ptr() as *mut u8, src.len(), true)
    }

    fn copy_pages(
        &self,
        mut address: u64,
        data: *mut u8,
        length: usize,
        write: bool,
    ) -> Result<(), VfsError> {
        let mut done = 0;
        while done < length {
            let m = self
                .mapped_frames
                .get(&(address & !4095))
                .ok_or(VfsError::Fault)?;
            let offset = address as usize & 4095;
            let count = (4096 - offset).min(length - done);
            // SAFETY: each physical page is retained by this locked address space;
            // source/destination is a kernel slice and page bounds are checked.
            unsafe {
                let page = (m.kernel as *mut u8).add(offset);
                if write {
                    core::ptr::copy_nonoverlapping(data.add(done), page, count);
                } else {
                    core::ptr::copy_nonoverlapping(page, data.add(done), count);
                }
            }
            address += count as u64;
            done += count;
        }
        Ok(())
    }

    pub fn unmap(&mut self, address: u64, length: u64) -> Result<(), VfsError> {
        let length = align_up(length).ok_or(VfsError::InvalidArgument)?;
        if address & 4095 != 0 || !Self::valid_range(address, length as usize) {
            return Err(VfsError::InvalidArgument);
        }
        let keys: Vec<u64> = self
            .mapped_frames
            .range(address..address + length)
            .map(|(&p, _)| p)
            .collect();
        // Clear all PTEs before shooting down, and free backing only after acknowledgements.
        for &page in &keys {
            if let Ok(leaf) = self.leaf(page, false) {
                unsafe { leaf.write_volatile(0) };
            }
        }
        if !keys.is_empty() {
            arch::tlb_shootdown(address, (length / PAGE_SIZE) as usize);
        }
        for page in keys {
            if let Some(m) = self.mapped_frames.remove(&page) {
                if m.owned {
                    let _ = pmm::free(m.physical, 1);
                }
            }
        }
        Ok(())
    }

    pub fn restrict_permissions(
        &mut self,
        address: u64,
        length: u64,
        maximum: u32,
    ) -> Result<(), VfsError> {
        let end = address
            .checked_add(length)
            .ok_or(VfsError::InvalidArgument)?;
        for (&_, mapping) in self.mapped_frames.range_mut(address..end) {
            mapping.max_prot &= maximum;
        }
        Ok(())
    }

    pub fn protect(&mut self, address: u64, length: u64, prot: u32) -> Result<(), VfsError> {
        let length = align_up(length).ok_or(VfsError::InvalidArgument)?;
        if address & 4095 != 0
            || length == 0
            || prot & !7 != 0
            || !Self::valid_range(address, length as usize)
        {
            return Err(VfsError::InvalidArgument);
        }
        if (address..address + length)
            .step_by(4096)
            .any(|p| !self.mapped_frames.contains_key(&p))
        {
            return Err(VfsError::NoMemory);
        }
        if self
            .mapped_frames
            .range(address..address + length)
            .any(|(_, m)| prot & !m.max_prot != 0)
        {
            return Err(VfsError::PermissionDenied);
        }
        for page in (address..address + length).step_by(4096) {
            let m = self.mapped_frames.get_mut(&page).unwrap();
            m.prot = prot;
            let value = external_leaf(m.physical, prot, m.uncached);
            let leaf = self.leaf(page, false).map_err(|_| VfsError::Fault)?;
            unsafe { leaf.write_volatile(value) };
        }
        arch::tlb_shootdown(address, (length / PAGE_SIZE) as usize);
        Ok(())
    }

    pub fn resize_brk(&mut self, requested: u64) -> u64 {
        if requested < self.brk_base || requested >= Self::user_limit() {
            return self.brk;
        }
        let Some(new_end) = align_up(requested) else {
            return self.brk;
        };
        let old_end = align_up(self.brk).unwrap();
        if new_end > old_end {
            if self
                .map_zeroed(old_end, new_end - old_end, PROT_READ | PROT_WRITE)
                .is_err()
            {
                return self.brk;
            }
        } else if new_end < old_end {
            let _ = self.unmap(new_end, old_end - new_end);
        }
        self.brk = requested;
        self.brk
    }

    pub fn maps(&self) -> Vec<(u64, u64, u32)> {
        let mut result: Vec<(u64, u64, u32)> = Vec::new();
        for (&page, m) in &self.mapped_frames {
            if let Some(last) = result.last_mut() {
                if last.1 == page && last.2 == m.prot {
                    last.1 += PAGE_SIZE;
                    continue;
                }
            }
            result.push((page, page + PAGE_SIZE, m.prot));
        }
        result
    }

    pub fn map_entries(&self) -> Vec<(u64, u64, u32, bool, u64, u64)> {
        let mut result: Vec<(u64, u64, u32, bool, u64, u64)> = Vec::new();
        for (&page, m) in &self.mapped_frames {
            let shared = !m.owned;
            let (rdev, ino) = m
                .owner
                .as_ref()
                .and_then(|i| i.metadata().ok())
                .map(|s| (s.rdev, s.ino))
                .unwrap_or((0, 0));
            if let Some(last) = result.last_mut() {
                if last.1 == page
                    && last.2 == m.prot
                    && last.3 == shared
                    && last.4 == rdev
                    && last.5 == ino
                {
                    last.1 += PAGE_SIZE;
                    continue;
                }
            }
            result.push((page, page + PAGE_SIZE, m.prot, shared, rdev, ino));
        }
        result
    }

    pub fn resident_pages(&self) -> usize {
        self.mapped_frames.len()
    }
}

impl Drop for UserAddressSpace {
    fn drop(&mut self) {
        // Free mapped physical frames
        for m in self.mapped_frames.values() {
            if m.owned {
                let _ = pmm::free(m.physical, 1);
            }
        }
        // Free intermediate page table frames
        for &frame in &self.allocated_tables {
            let _ = pmm::free(frame, 1);
        }
        // Free root page table frame
        let _ = pmm::free(self.root_phys, 1);
    }
}

fn make_user_table_entry(physical: u64) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        // Present (bit 0) | Writable (bit 1) | User (bit 2)
        physical | 0x07
    }
    #[cfg(target_arch = "aarch64")]
    {
        // Table descriptor: bit 0 and bit 1 set (0b11)
        physical | 0b11
    }
    #[cfg(target_arch = "riscv64")]
    {
        // Pointer to next-level page table: V=1 (bit 0)
        ((physical >> 12) << 10) | 0x01
    }
}

fn make_user_leaf_entry(physical: u64, writable: bool, executable: bool) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let mut entry = physical | 0x05; // Present (1) | User (4)
        if writable {
            entry |= 0x02;
        }
        if !executable && crate::cpuid::has(crate::cpuid::Feature::Nx) {
            entry |= 1u64 << 63;
        }
        entry
    }
    #[cfg(target_arch = "aarch64")]
    {
        // Page descriptor: 0b11, AF=1 (bit 10), IS=0b11 (bits 9:8), AP[1]=1 for EL0 (bit 6)
        let mut entry = physical | 0b11 | (1 << 10) | (0b11 << 8) | (1 << 6) | (1 << 53);
        if !writable {
            entry |= 1 << 7; // AP[2] = 1 (read-only)
        }
        if !executable {
            entry |= (1 << 53) | (1 << 54); // PXN | UXN
        }
        entry
    }
    #[cfg(target_arch = "riscv64")]
    {
        // V(1), R(2), U(16), A(64), D(128)
        let mut entry = ((physical >> 12) << 10) | 0xc3 | 0x10;
        if writable {
            entry |= 0x04;
        }
        if executable {
            entry |= 0x08;
        }
        entry
    }
}

fn user_leaf(physical: u64, prot: u32) -> u64 {
    if prot == 0 {
        return 0;
    }
    #[allow(unused_mut)]
    let mut value = make_user_leaf_entry(physical, prot & PROT_WRITE != 0, prot & PROT_EXEC != 0);
    #[cfg(target_arch = "riscv64")]
    if prot & (PROT_READ | PROT_WRITE) == 0 {
        value &= !2;
    }
    value
}

pub fn align_up(value: u64) -> Option<u64> {
    value.checked_add(4095).map(|n| n & !4095)
}

fn external_leaf(physical: u64, prot: u32, uncached: bool) -> u64 {
    let value = user_leaf(physical, prot);
    if !uncached || prot == 0 {
        return value;
    }
    #[cfg(target_arch = "x86_64")]
    {
        return value | 0x18;
    }
    #[cfg(target_arch = "aarch64")]
    {
        return (value & !(7 << 2)) | (3 << 2);
    }
    #[cfg(target_arch = "riscv64")]
    {
        value
    }
}
