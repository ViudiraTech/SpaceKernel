//! ARM Generic Timer using the Virtual Timer (CNTV, PPI 27).

use crate::{irq::IrqError, time};

pub const TIMER_INTID: u32 = 27;

#[inline]
unsafe fn read_cntfrq_el0() -> u64 {
    let val: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) val, options(nomem, nostack));
    }
    val
}

#[inline]
unsafe fn write_cntv_tval_el0(val: i64) {
    unsafe {
        core::arch::asm!("msr cntv_tval_el0, {}", in(reg) val, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_cntv_ctl_el0(val: u64) {
    unsafe {
        core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) val, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

pub fn arm_after_micros(micros: u64) -> Result<(), IrqError> {
    let freq = time::counter_frequency().unwrap_or_else(|| unsafe { read_cntfrq_el0() });
    if freq == 0 {
        return Err(IrqError::Unsupported);
    }

    let ticks = ((u128::from(freq) * u128::from(micros)).div_ceil(1_000_000))
        .max(1)
        .min(i64::MAX as u128) as i64;

    // SAFETY: programming architecturally defined ARM generic timer registers.
    unsafe {
        // Enable timer and unmask interrupt (ENABLE=1, IMASK=0)
        write_cntv_tval_el0(ticks);
        write_cntv_ctl_el0(1);
    }

    Ok(())
}

pub fn cancel() {
    // SAFETY: disable timer (ENABLE=0, IMASK=1)
    unsafe {
        write_cntv_ctl_el0(2);
    }
}
