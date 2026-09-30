/*
 *
 *       src/smp.rs
 *       Architecture-independent Limine CPU startup and online publication
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Limine supplies parked APs and boot stacks; reserved MP fields are untouched.
//! Kernel tables, page root, FPU template and runqueues precede the release store
//! in MpInfo::bootstrap. Each AP publishes online only after local setup.

use crate::{arch, boot, cpuid, fpu, mm, sched};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

pub const MAX_CPUS: usize = 256;
static STATES: [AtomicU8; MAX_CPUS] = [const { AtomicU8::new(0) }; MAX_CPUS];
static CPU_INFO: [crate::sync::SpinLock<Option<cpuid::CpuInfo>>; MAX_CPUS] =
    [const { crate::sync::SpinLock::new(None) }; MAX_CPUS];
static RELEASED: AtomicBool = AtomicBool::new(false);

pub fn current_cpu() -> usize {
    arch::current_cpu_index().expect("CPU-local state required")
}
pub fn online(cpu: usize) -> bool {
    STATES[cpu].load(Ordering::Acquire) == 1
}
pub fn online_count() -> usize {
    (0..boot::cpu_count()).filter(|&cpu| online(cpu)).count()
}

pub fn start() {
    let bsp = boot::bsp_cpu_index();
    arch::init_scheduler_cpu(bsp).expect("BSP scheduling timer initialization failed");
    sched::activate_cpu(bsp);
    *CPU_INFO[bsp].lock() = cpuid::info();
    STATES[bsp].store(1, Ordering::Release);
    for cpu in 0..boot::cpu_count() {
        if cpu != bsp {
            boot::start_cpu(cpu, ap_entry);
        }
    }
    let deadline = crate::time::uptime_micros().saturating_add(5_000_000);
    while online_count() != boot::cpu_count() {
        assert!(
            (0..boot::cpu_count()).all(|cpu| STATES[cpu].load(Ordering::Acquire) != 2),
            "AP initialization failed"
        );
        assert!(
            crate::time::uptime_micros() < deadline,
            "AP startup timed out"
        );
        core::hint::spin_loop();
    }
    sched::build_domains();
    crate::kinfo!("SMP: {}/{} CPUs online", online_count(), boot::cpu_count());
}

unsafe extern "C" fn ap_entry(info: &limine::mp::MpInfo) -> ! {
    arch::disable_interrupts();
    let cpu = info.extra_argument() as usize;
    assert!(cpu < boot::cpu_count() && cpu != boot::bsp_cpu_index());
    #[cfg(target_arch = "aarch64")]
    arch::setup_device_memory();
    #[cfg(target_arch = "x86_64")]
    arch::setup_memory_protection();
    arch::set_page_root(mm::vmm::root().expect("kernel page root required"));
    arch::load_exceptions(cpu);
    arch::install_cpu_index(cpu);
    // SAFETY: this AP has exclusive register ownership with IRQs masked.
    let local = unsafe { cpuid::detect_current(cpu) }.expect("AP identity required");
    let required = cpuid::info().expect("BSP capabilities required").features;
    if !cpuid::compatible(&local, required) {
        crate::kerror!("AP {} rejected: CPU capabilities/address widths", cpu);
        park_failed(cpu);
    }
    // SAFETY: local IRQs/preemption are masked; no task owns this AP's registers.
    if let Err(error) = unsafe { fpu::init_current_cpu(&local) } {
        crate::kerror!("AP {} FPU initialization failed: {:?}", cpu, error);
        park_failed(cpu);
    }
    if let Err(error) = arch::init_scheduler_cpu(cpu) {
        crate::kerror!(
            "AP {} interrupt/timer initialization failed: {:?}",
            cpu,
            error
        );
        park_failed(cpu);
    }
    *CPU_INFO[cpu].lock() = Some(local);
    sched::activate_cpu(cpu);
    crate::kinfo!(
        "AP {}: id={:#x} exception/FPU/timer ready",
        cpu,
        local.hardware_id
    );
    STATES[cpu].store(1, Ordering::Release);
    while !RELEASED.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    sched::start_current_cpu();
    sched::idle_loop()
}

/// All CPUs begin task execution after the BSP's pre-SMP boot tests complete.
pub fn release() {
    RELEASED.store(true, Ordering::Release);
}

pub(crate) fn cpu_info(cpu: usize) -> Option<cpuid::CpuInfo> {
    *CPU_INFO[cpu].lock()
}

fn park_failed(cpu: usize) -> ! {
    STATES[cpu].store(2, Ordering::Release);
    loop {
        arch::halt();
    }
}
