/*
 *
 *       src/arch/riscv64/scheduler.rs
 *       Hart-local trap state, SBI timer and reschedule IPI backend
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */
use crate::irq;
pub fn install_cpu_index(_cpu: usize) {} // sscratch installed with exception state.
pub fn register_scheduler_irqs() -> Result<(), irq::IrqError> {
    irq::register(super::clint::TIMER_IRQ, crate::sched::timer_interrupt)?;
    irq::register(super::clint::IPI_IRQ, crate::sched::reschedule_interrupt)
}
pub fn init_scheduler_cpu(cpu: usize) -> Result<(), irq::IrqError> {
    let hart = crate::boot::cpu_hardware_id(cpu).ok_or(irq::IrqError::Invalid)? as usize;
    if super::plic::ready() {
        super::plic::init_cpu(hart)?;
    }
    super::clint::init_cpu(hart)
}
pub fn arm_scheduler_tick() -> Result<(), irq::IrqError> {
    super::clint::arm_after_micros(1000)
}
pub fn send_reschedule(cpu: usize) -> Result<(), irq::IrqError> {
    super::clint::send_ipi(
        crate::boot::cpu_hardware_id(cpu).ok_or(irq::IrqError::Invalid)? as usize,
    )
}
pub fn idle_wait() {
    // The recurring timer also closes the check-work/WFI race.
    unsafe {
        core::arch::asm!("csrsi sstatus, 2", "wfi", options(nomem, nostack));
    }
}
