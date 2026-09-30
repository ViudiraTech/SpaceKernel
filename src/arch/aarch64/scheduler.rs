/*
 *
 *       src/arch/aarch64/scheduler.rs
 *       GIC CPU interfaces, private timer and reschedule SGI backend
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */
use crate::irq;
const IPI: u32 = 0;
pub fn install_cpu_index(_cpu: usize) {} // TPIDR_EL1 installed with exception state.
pub fn register_scheduler_irqs() -> Result<(), irq::IrqError> {
    irq::register(
        super::gic::timer::TIMER_INTID,
        crate::sched::timer_interrupt,
    )?;
    irq::register(IPI, crate::sched::reschedule_interrupt)
}
pub fn init_scheduler_cpu(_cpu: usize) -> Result<(), irq::IrqError> {
    super::gic::init_cpu()?;
    super::gic::configure_local(super::gic::timer::TIMER_INTID, false)?;
    super::gic::configure_local(IPI, true)
}
pub fn arm_scheduler_tick() -> Result<(), irq::IrqError> {
    super::gic::timer::arm_after_micros(1000)
}
pub fn send_reschedule(cpu: usize) -> Result<(), irq::IrqError> {
    super::gic::send_sgi(
        IPI as u8,
        crate::boot::cpu_hardware_id(cpu).ok_or(irq::IrqError::Invalid)?,
    )
}
pub fn idle_wait() {
    // A periodic private timer bounds the idle wakeup even if an SGI races WFI.
    unsafe {
        core::arch::asm!("msr daifclr, #2", "wfi", options(nomem, nostack));
    }
}
