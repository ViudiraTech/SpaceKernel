//! Quiesce the legacy 8259 before enabling LAPIC-delivered interrupts.
use core::arch::asm;

pub fn mask_all() {
    // SAFETY: these ports are the architectural dual-8259 data registers.
    unsafe {
        asm!("out dx, al", in("dx") 0x21u16, in("al") 0xffu8, options(nomem, nostack));
        asm!("out dx, al", in("dx") 0xa1u16, in("al") 0xffu8, options(nomem, nostack));
    }
}
