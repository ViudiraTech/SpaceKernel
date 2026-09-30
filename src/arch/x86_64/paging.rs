/*
 *
 *       src/arch/x86_64/paging.rs
 *       x86-64 page-table descriptors, root activation and TLB maintenance
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::asm;

const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

pub fn paging_geometry() -> super::super::paging::Geometry {
    super::super::paging::Geometry {
        levels: 4,
        virtual_bits: 48,
        physical_bits: crate::cpuid::info()
            .expect("CPU discovery required")
            .physical_bits
            .unwrap_or(36)
            .min(52) as u32,
    }
}

pub fn page_root() -> u64 {
    let cr3: u64;
    // SAFETY: kernel privilege allows reading CR3.
    unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) }
    cr3 & ADDRESS_MASK
}

pub fn set_page_root(root: u64) {
    // SAFETY: caller supplies an initialized and mapped PML4 frame.
    unsafe { asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags)) }
}

pub fn pte_present(entry: u64) -> bool {
    entry & 1 != 0
}
pub fn pte_is_table(entry: u64, level: usize) -> bool {
    pte_present(entry) && (level == 3 || entry & (1 << 7) == 0)
}
pub fn pte_phys(entry: u64) -> u64 {
    entry & ADDRESS_MASK
}
pub fn pte_table(physical: u64) -> u64 {
    physical | 0x003
}
pub fn pte_leaf(physical: u64, device: bool) -> u64 {
    let nx = crate::cpuid::has(crate::cpuid::Feature::Nx);
    physical | 0x003 | if nx { 1 << 63 } else { 0 } | if device { 0x018 } else { 0 }
}

pub fn setup_memory_protection() {
    if !crate::cpuid::has(crate::cpuid::Feature::Nx) {
        return;
    }
    let (low, high): (u32, u32);
    // SAFETY: EFER exists in long mode; NXE is set only after CPUID validation.
    unsafe {
        asm!("rdmsr", in("ecx") 0xc000_0080u32, out("eax") low, out("edx") high, options(nomem,nostack));
        asm!("wrmsr", in("ecx") 0xc000_0080u32, in("eax") low | (1 << 11), in("edx") high, options(nostack));
    }
}

pub fn flush_page(virtual_address: u64) {
    // SAFETY: the caller has completed its page-table write.
    unsafe { asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags)) }
}

/// INVLPG also invalidates this PCID's paging-structure caches, so it covers
/// publication of a descendant table as well as the selected leaf translation.
pub fn flush_table(virtual_address: u64) {
    flush_page(virtual_address);
}
