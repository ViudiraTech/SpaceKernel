/*
 *
 *       src/arch/riscv64/clint.rs
 *       Supervisor timer and IPI control through the standard and legacy SBI ABIs
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Supervisor timer and IPI control through the standard and legacy SBI ABIs.
//! MMIO CLINT and direct Sstc access require platform permission and are not
//! selected speculatively by this driver.

use crate::{
    arch,
    irq::{self, IrqError},
    time,
};
use core::sync::atomic::{AtomicU64, Ordering};

pub const TIMER_IRQ: u32 = 0;
pub const IPI_IRQ: u32 = 1;

const DEFAULT_FREQ: u64 = 10_000_000; // 10 MHz default on RISC-V virt

static FREQUENCY: AtomicU64 = AtomicU64::new(DEFAULT_FREQ);

#[inline]
pub fn sbi_call(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize) -> (usize, usize) {
    let error: usize;
    let value: usize;
    // SAFETY: executes SBI environment call according to RISC-V SBI spec.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") arg0 => error,
            inlateout("a1") arg1 => value,
            in("a2") arg2,
            in("a6") fid,
            in("a7") eid,
            options(nostack)
        );
    }
    (error, value)
}

fn sbi_set_timer(stime: u64) {
    // Try standard SBI Time extension (EID 0x54494D45)
    let (err, _) = sbi_call(0x5449_4D45, 0, stime as usize, 0, 0);
    if err != 0 {
        // Fallback to legacy SBI extension (EID 0x00)
        let _ = sbi_call(0x00, 0, stime as usize, 0, 0);
    }
}

fn sbi_send_ipi_mask(hart_mask: usize) -> Result<(), IrqError> {
    let mask = hart_mask;
    // Standard SBI sPI extension (EID 0x735049)
    // SBI v0.2 passes the mask by value; only the legacy ABI takes a pointer.
    let (err, _) = sbi_call(0x0073_5049, 0, mask, 0, 0);
    if err != 0 {
        // Fallback to legacy SBI IPI (EID 0x04)
        let (legacy_error, _) = sbi_call(0x04, 0, core::ptr::addr_of!(mask) as usize, 0, 0);
        if legacy_error != 0 {
            return Err(IrqError::Unsupported);
        }
    }
    Ok(())
}

pub fn init_bsp() -> Result<(), IrqError> {
    let freq = time::counter_frequency().unwrap_or(DEFAULT_FREQ);
    FREQUENCY.store(freq, Ordering::Release);

    // Cancel any active timer
    sbi_set_timer(u64::MAX);

    // Enable Supervisor Timer Interrupt (STIE = bit 5) and
    // Supervisor Software Interrupt (SSIE = bit 1) in sie CSR
    // SAFETY: atomic bitwise set on sie register.
    unsafe {
        core::arch::asm!("csrs sie, {mask}", mask = in(reg) 0x22usize, options(nomem, nostack));
        // Clear pending software interrupt if any
        core::arch::asm!("csrci sip, 0x02", options(nomem, nostack));
    }

    crate::kinfo!(
        "CLINT/Timer: initialized (STIE/SSIE enabled, freq={} Hz)",
        freq
    );
    Ok(())
}

pub fn init_cpu(_hart_id: usize) -> Result<(), IrqError> {
    sbi_set_timer(u64::MAX);
    // SAFETY: enable STIE and SSIE on secondary hart.
    unsafe {
        core::arch::asm!("csrs sie, {mask}", mask = in(reg) 0x22usize, options(nomem, nostack));
        core::arch::asm!("csrci sip, 0x02", options(nomem, nostack));
    }
    Ok(())
}

pub fn arm_after_micros(micros: u64) -> Result<(), IrqError> {
    let freq = FREQUENCY.load(Ordering::Acquire);
    if freq == 0 {
        return Err(IrqError::Unsupported);
    }

    let delta = ((u128::from(freq) * u128::from(micros)).div_ceil(1_000_000))
        .max(1)
        .min(u128::from(u64::MAX)) as u64;

    let deadline = arch::counter().saturating_add(delta);

    sbi_set_timer(deadline);
    Ok(())
}

pub fn cancel() {
    sbi_set_timer(u64::MAX);
}

pub fn send_ipi(hart_id: usize) -> Result<(), IrqError> {
    if hart_id >= usize::BITS as usize {
        return Err(IrqError::Invalid);
    }
    sbi_send_ipi_mask(1 << hart_id)
}

pub fn handle_timer_irq() {
    // Reset timer to prevent flood until re-armed
    cancel();
    irq::dispatch(TIMER_IRQ);
}

pub fn handle_software_irq() {
    // Clear SSIP (bit 1) in sip CSR
    // SAFETY: clearing software interrupt pending bit in supervisor mode.
    unsafe {
        core::arch::asm!("csrci sip, 0x02", options(nomem, nostack));
    }
    irq::dispatch(IPI_IRQ);
}
