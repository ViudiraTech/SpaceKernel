/*
 *
 *       src/arch/aarch64/paging.rs
 *       AArch64 page-table descriptors, root activation and TLB maintenance
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use core::arch::asm;

const ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;

pub fn paging_geometry() -> super::super::paging::Geometry {
    super::super::paging::Geometry {
        levels: 4,
        virtual_bits: 48,
        physical_bits: crate::cpuid::info()
            .expect("CPU discovery required")
            .physical_bits
            .unwrap_or(32)
            .min(48) as u32,
    }
}

pub fn page_root() -> u64 {
    let root: u64;
    // SAFETY: Limine enters with TTBR1_EL1 configured and readable.
    unsafe { asm!("mrs {}, ttbr1_el1", out(reg) root, options(nomem, nostack)) }
    root & ADDRESS_MASK
}

pub fn set_page_root(root: u64) {
    // SAFETY: caller supplies a valid root copied from the active table.
    unsafe {
        asm!("dsb ishst", "msr ttbr1_el1, {}", "isb", "tlbi vmalle1is", "dsb ish", "isb", in(reg) root, options(nostack))
    }
}

pub fn pte_present(entry: u64) -> bool {
    entry & 1 != 0
}
pub fn pte_is_table(entry: u64, _level: usize) -> bool {
    entry & 3 == 3
}
pub fn pte_phys(entry: u64) -> u64 {
    entry & ADDRESS_MASK
}
pub fn pte_table(physical: u64) -> u64 {
    physical | 0b11
}
pub fn pte_leaf(physical: u64, device: bool) -> u64 {
    // Limine rev 6 fixes Attr0 to Normal WB; Attr2 is configured as Device.
    physical
        | 0b11
        | (1 << 10)
        | (0b11 << 8)
        | (1 << 53)
        | (1 << 54)
        | if device { 2 << 2 } else { 0 }
}

pub fn setup_device_memory() {
    let mut mair: u64;
    // SAFETY: Limine rev 6 leaves Attr2 unused, and the caller runs before
    // any kernel device mapping uses that index.
    unsafe {
        asm!("mrs {}, mair_el1", out(reg) mair, options(nomem, nostack));
        mair &= !(0xff << 16);
        asm!("msr mair_el1, {}", "isb", in(reg) mair, options(nostack));
    }
}

pub fn flush_page(virtual_address: u64) {
    let page = virtual_address >> 12;
    // SAFETY: required barriers surround invalidation of the new mapping.
    unsafe {
        asm!("dsb ishst", "tlbi vaae1is, {}", "dsb ish", "isb", in(reg) page, options(nostack))
    }
}

/// Complete publication of a new descendant table at every cached walk level.
pub fn flush_table(_virtual_address: u64) {
    // SAFETY: EL1 context invalidation, ordered after the parent descriptor.
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack)
        )
    }
}
