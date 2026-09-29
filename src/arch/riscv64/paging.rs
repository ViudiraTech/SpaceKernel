use core::arch::asm;

pub fn page_root() -> u64 {
    let satp: u64;
    // SAFETY: satp is readable in S mode.
    unsafe { asm!("csrr {}, satp", out(reg) satp, options(nomem, nostack)) }
    (satp & ((1u64 << 44) - 1)) << 12
}

pub fn set_page_root(root: u64) {
    let satp = (9u64 << 60) | (root >> 12); // Sv48, ASID 0.
    // SAFETY: caller supplies a valid Sv48 root; sfence removes stale entries.
    unsafe { asm!("csrw satp, {}", "sfence.vma", in(reg) satp, options(nostack)) }
}

pub fn pte_present(entry: u64) -> bool {
    entry & 1 != 0
}
pub fn pte_phys(entry: u64) -> u64 {
    ((entry >> 10) & ((1u64 << 44) - 1)) << 12
}
pub fn pte_table(physical: u64) -> u64 {
    ((physical >> 12) << 10) | 1
}
pub fn pte_leaf(physical: u64, _device: bool) -> u64 {
    ((physical >> 12) << 10) | 0xc7 // V,R,W,A,D; platform PMA controls MMIO.
}

pub fn flush_page(virtual_address: u64) {
    // SAFETY: local hart translation invalidation after a new PTE write.
    unsafe { asm!("sfence.vma {}, x0", in(reg) virtual_address, options(nostack)) }
}
