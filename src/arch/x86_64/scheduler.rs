/*
 *
 *       src/arch/x86_64/scheduler.rs
 *       CPU-local scheduler identity and calibrated LAPIC scheduling ticks
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use crate::{irq, time};
use core::{
    arch::asm,
    sync::atomic::{AtomicU32, Ordering},
};
const TIMER: u32 = 0xf0;
const IPI: u32 = 0xf1;
#[repr(C)]
struct CpuLocal {
    index: usize,
}
static LOCALS: [CpuLocal; 256] = make_locals();
const fn make_locals() -> [CpuLocal; 256] {
    let mut locals = [const { CpuLocal { index: 0 } }; 256];
    let mut i = 0;
    while i < 256 {
        locals[i].index = i;
        i += 1;
    }
    locals
}
static TICKS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

pub fn install_cpu_index(index: usize) {
    let address = &LOCALS[index] as *const CpuLocal as u64;
    // SAFETY: permanent CPU-local object; FS/GS are never task-owned in this kernel.
    unsafe {
        asm!("wrmsr",in("ecx") 0xc0000101u32,in("eax") address as u32,in("edx") (address>>32) as u32,options(nostack));
    }
}
pub fn current_cpu_index() -> Option<usize> {
    let index: usize;
    // SAFETY: BSP installs GS before scheduler initialization; APs before online.
    unsafe {
        asm!("mov {}, gs:[0]",out(reg) index,options(nomem,nostack,preserves_flags));
    }
    Some(index)
}
pub fn register_scheduler_irqs() -> Result<(), irq::IrqError> {
    irq::register(TIMER, crate::sched::timer_interrupt)?;
    irq::register(IPI, crate::sched::reschedule_interrupt)
}
pub fn init_scheduler_cpu(cpu: usize) -> Result<(), irq::IrqError> {
    super::apic::init_cpu()?;
    if !crate::cpuid::has(crate::cpuid::Feature::TscDeadline) {
        time::counter_frequency().ok_or(irq::IrqError::Unsupported)?;
        super::apic::timer::arm_apic_ticks(u32::MAX)?;
        let start = time::uptime_micros();
        let before = super::apic::timer::current_ticks();
        while time::uptime_micros().saturating_sub(start) < 5000 {
            core::hint::spin_loop();
        }
        let elapsed = time::uptime_micros().saturating_sub(start).max(1);
        let ticks = u64::from(before - super::apic::timer::current_ticks()) * 1000 / elapsed;
        super::apic::timer::cancel();
        if ticks == 0 {
            return Err(irq::IrqError::Unsupported);
        }
        TICKS[cpu].store(ticks.min(u64::from(u32::MAX)) as u32, Ordering::Release);
    }
    Ok(())
}
pub fn arm_scheduler_tick() -> Result<(), irq::IrqError> {
    if crate::cpuid::has(crate::cpuid::Feature::TscDeadline) {
        super::apic::timer::arm_after_micros(1000)
    } else {
        super::apic::timer::arm_apic_ticks(TICKS[crate::smp::current_cpu()].load(Ordering::Acquire))
    }
}
pub fn send_reschedule(cpu: usize) -> Result<(), irq::IrqError> {
    super::apic::send_ipi(crate::boot::cpu_hardware_id(cpu).ok_or(irq::IrqError::Invalid)? as u32)
}
/// Atomically unmask and halt so an IPI cannot be lost between checking work and sleep.
pub fn idle_wait() {
    unsafe {
        asm!("sti", "hlt", options(nomem, nostack));
    }
}
