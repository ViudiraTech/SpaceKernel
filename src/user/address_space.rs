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

use crate::{
    arch,
    mm::{pmm, vmm},
};
use alloc::vec::Vec;

pub struct UserAddressSpace {
    root_phys: u64,
    allocated_tables: Vec<u64>,
    mapped_frames: Vec<(u64, u64)>, // (virtual_addr, phys_frame)
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
            mapped_frames: Vec::new(),
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

    /// Map a 4 KiB user page with specified permissions.
    pub fn map_page(
        &mut self,
        virtual_address: u64,
        physical: u64,
        writable: bool,
        executable: bool,
    ) -> Result<(), &'static str> {
        let geometry = arch::paging_geometry();
        let mut table = self.root_phys;

        // Traverse page table levels from root down to leaf table (level 0)
        for level in (1..geometry.levels).rev() {
            let shift = 12 + level * 9;
            let index = ((virtual_address >> shift) & 0x1ff) as usize;
            let entry_ptr = (pmm::phys_to_virt(table) as *mut u64).wrapping_add(index);
            let value = unsafe { entry_ptr.read_volatile() };

            if arch::pte_present(value) {
                table = arch::pte_phys(value);
            } else {
                let frame = pmm::alloc(1).ok_or("out of memory for user page table")?;
                unsafe {
                    core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, 4096);
                }
                self.allocated_tables.push(frame);

                // Publish parent table entry with User accessibility
                let parent_entry = make_user_table_entry(frame);
                unsafe {
                    entry_ptr.write_volatile(parent_entry);
                }
                table = frame;
            }
        }

        // Write leaf entry at level 0
        let index = ((virtual_address >> 12) & 0x1ff) as usize;
        let leaf_ptr = (pmm::phys_to_virt(table) as *mut u64).wrapping_add(index);
        let existing = unsafe { leaf_ptr.read_volatile() };
        if arch::pte_present(existing) {
            return Err("user page already mapped");
        }

        let leaf_entry = make_user_leaf_entry(physical, writable, executable);
        unsafe {
            leaf_ptr.write_volatile(leaf_entry);
        }
        self.mapped_frames.push((virtual_address, physical));
        arch::flush_page(virtual_address);
        Ok(())
    }
}

impl Drop for UserAddressSpace {
    fn drop(&mut self) {
        // Free mapped physical frames
        for &(_va, phys) in &self.mapped_frames {
            let _ = pmm::free(phys, 1);
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
        let mut entry = physical | 0b11 | (1 << 10) | (0b11 << 8) | (1 << 6);
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
