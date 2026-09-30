/*
 *
 *       src/arch/x86_64/apic/timer.rs
 *       Local APIC one-shot timer, using TSC deadline when available
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Local APIC one-shot timer, using TSC deadline when available.
use super::local;
use crate::{irq::IrqError, time};

pub fn arm_after_micros(micros: u64) -> Result<(), IrqError> {
    if !local::tsc_deadline_supported() {
        return Err(IrqError::Unsupported);
    }
    let frequency = time::counter_frequency().ok_or(IrqError::Unsupported)?;
    let delta = ((u128::from(frequency) * u128::from(micros)).div_ceil(1_000_000))
        .max(1)
        .min(u128::from(u64::MAX)) as u64;
    local::timer_set_lvt(u32::from(local::TIMER_VECTOR) | (2 << 17));
    // Intel requires serialization between LVTT programming and the deadline MSR.
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
    local::set_tsc_deadline(crate::arch::counter().saturating_add(delta));
    Ok(())
}

/// Fallback when TSC deadline is unavailable. `ticks` are divided APIC clock ticks.
pub fn arm_apic_ticks(ticks: u32) -> Result<(), IrqError> {
    if ticks == 0 {
        return Err(IrqError::Invalid);
    }
    local::timer_set_lvt(u32::from(local::TIMER_VECTOR));
    local::timer_set_divide(0b0011); // divide by 16
    local::timer_set_initial(ticks);
    Ok(())
}

pub fn cancel() {
    local::timer_set_lvt(u32::from(local::TIMER_VECTOR) | (1 << 16));
    if local::tsc_deadline_supported() {
        local::set_tsc_deadline(0);
    }
    local::timer_set_initial(0);
}

pub fn current_ticks() -> u32 {
    local::timer_current()
}
