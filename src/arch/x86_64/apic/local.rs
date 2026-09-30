/*
 *
 *       src/arch/x86_64/apic/local.rs
 *       Local APIC register access, initialization and interprocessor interrupts
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Local APIC register access, initialization and interprocessor interrupts.

use crate::{irq::IrqError, mm::vmm};
use core::{
    arch::asm,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};

const APIC_BASE_MSR: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;
const APIC_BASE_MASK: u64 = 0x000f_ffff_ffff_f000;
const ID: u32 = 0x20;
const VERSION: u32 = 0x30;
const TPR: u32 = 0x80;
const EOI: u32 = 0xb0;
const SPURIOUS: u32 = 0xf0;
const ESR: u32 = 0x280;
const ICR_LOW: u32 = 0x300;
const ICR_HIGH: u32 = 0x310;
const LVT_TIMER: u32 = 0x320;
const LVT_THERMAL: u32 = 0x330;
const LVT_PERF: u32 = 0x340;
const LVT_LINT0: u32 = 0x350;
const LVT_LINT1: u32 = 0x360;
const LVT_ERROR: u32 = 0x370;

pub const TIMER_VECTOR: u8 = 0xf0;
pub const IPI_VECTOR: u8 = 0xf1;
pub const ERROR_VECTOR: u8 = 0xfe;
pub const SPURIOUS_VECTOR: u8 = 0xff;

// 0=not initialized, 1=xAPIC MMIO, 2=x2APIC MSRs.
static MODE: AtomicU8 = AtomicU8::new(0);
static MMIO: AtomicUsize = AtomicUsize::new(0);

fn rdmsr(msr: u32) -> u64 {
    let (lo, hi): (u32, u32);
    // SAFETY: callers use architectural MSRs after CPUID capability checks.
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack));
    }
    u64::from(lo) | (u64::from(hi) << 32)
}

fn wrmsr(msr: u32, value: u64) {
    // SAFETY: the caller supplies a valid architectural MSR and value.
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") value as u32,
        in("edx") (value >> 32) as u32, options(nomem, nostack));
    }
}

pub fn read(reg: u32) -> u32 {
    match MODE.load(Ordering::Acquire) {
        1 => unsafe {
            ((MMIO.load(Ordering::Relaxed) + reg as usize) as *const u32).read_volatile()
        },
        2 => rdmsr(0x800 + reg / 16) as u32,
        _ => 0,
    }
}

pub fn write(reg: u32, value: u32) {
    match MODE.load(Ordering::Acquire) {
        1 => unsafe {
            ((MMIO.load(Ordering::Relaxed) + reg as usize) as *mut u32).write_volatile(value)
        },
        2 => wrmsr(0x800 + reg / 16, u64::from(value)),
        _ => {}
    }
}

pub fn ready() -> bool {
    MODE.load(Ordering::Acquire) != 0
}
pub fn is_x2apic() -> bool {
    MODE.load(Ordering::Acquire) == 2
}

pub fn init_bsp(address: u64) -> Result<u32, IrqError> {
    if !crate::cpuid::has(crate::cpuid::Feature::Apic) {
        return Err(IrqError::Unsupported);
    }
    let x2 = crate::cpuid::has(crate::cpuid::Feature::X2apic);
    let mut base = rdmsr(APIC_BASE_MSR);
    if base & X2APIC_ENABLE != 0 && !x2 {
        return Err(IrqError::Unsupported);
    }
    let mode = if x2 { 2 } else { 1 };
    if mode == 1 {
        if address == 0 || address & 0xfff != 0 || address & !APIC_BASE_MASK != 0 {
            return Err(IrqError::Mapping);
        }
        let mapped = vmm::map_device_page(address).map_err(|_| IrqError::Mapping)?;
        MMIO.store(mapped as usize, Ordering::Relaxed);
        base = (base & !APIC_BASE_MASK) | address;
    }
    base |= APIC_ENABLE;
    if mode == 2 {
        base |= X2APIC_ENABLE;
    }
    wrmsr(APIC_BASE_MSR, base);
    MODE.store(mode, Ordering::Release);
    configure_cpu();
    Ok(id())
}

/// Called on every AP after its IDT is loaded and before interrupts are enabled.
pub fn configure_cpu() {
    if !ready() {
        return;
    }
    let base = rdmsr(APIC_BASE_MSR);
    wrmsr(
        APIC_BASE_MSR,
        base | APIC_ENABLE | if is_x2apic() { X2APIC_ENABLE } else { 0 },
    );
    write(TPR, 0);
    write(SPURIOUS, 0x100 | u32::from(SPURIOUS_VECTOR));
    let max_lvt = (read(VERSION) >> 16) & 0xff;
    write(LVT_TIMER, 1 << 16 | u32::from(TIMER_VECTOR));
    write(LVT_LINT0, 1 << 16);
    write(LVT_LINT1, 1 << 16);
    if max_lvt >= 4 {
        write(LVT_PERF, 1 << 16);
    }
    if max_lvt >= 5 {
        write(LVT_THERMAL, 1 << 16);
    }
    if max_lvt >= 3 {
        write(ESR, 0);
        let _ = read(ESR);
        write(LVT_ERROR, u32::from(ERROR_VECTOR));
    }
}

pub fn id() -> u32 {
    if is_x2apic() {
        read(ID)
    } else {
        read(ID) >> 24
    }
}

pub fn eoi() {
    write(EOI, 0);
}

pub fn error_status() -> u32 {
    write(ESR, 0);
    read(ESR)
}

pub fn set_lint_nmi(lint: u8, flags: u16) -> Result<(), IrqError> {
    let reg = match lint {
        0 => LVT_LINT0,
        1 => LVT_LINT1,
        _ => return Err(IrqError::Invalid),
    };
    let polarity = match flags & 3 {
        0 | 1 => 0,
        3 => 1 << 13,
        _ => return Err(IrqError::Invalid),
    };
    let trigger = match (flags >> 2) & 3 {
        0 | 1 => 0,
        3 => 1 << 15,
        _ => return Err(IrqError::Invalid),
    };
    write(reg, 4 << 8 | polarity | trigger);
    Ok(())
}

pub fn send_fixed(destination: u32, vector: u8) -> Result<(), IrqError> {
    if vector < 32 || vector == SPURIOUS_VECTOR {
        return Err(IrqError::Invalid);
    }
    send(destination, u32::from(vector))
}

pub fn send_nmi(destination: u32) -> Result<(), IrqError> {
    send(destination, 4 << 8)
}

fn send(destination: u32, low: u32) -> Result<(), IrqError> {
    // xAPIC has separate destination/high and vector/low writes. A local IRQ
    // sending another IPI between them would redirect the interrupted send.
    let flags = crate::arch::irq_save();
    let result = send_masked(destination, low);
    crate::arch::irq_restore(flags);
    result
}

fn send_masked(destination: u32, low: u32) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if is_x2apic() {
        wrmsr(0x830, (u64::from(destination) << 32) | u64::from(low));
        return Ok(());
    }
    if destination > 255 {
        return Err(IrqError::Unsupported);
    }
    wait_icr()?;
    write(ICR_HIGH, destination << 24);
    write(ICR_LOW, low);
    wait_icr()
}

fn wait_icr() -> Result<(), IrqError> {
    for _ in 0..1_000_000 {
        if read(ICR_LOW) & (1 << 12) == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(IrqError::Timeout)
}

pub fn self_ipi(vector: u8) -> Result<(), IrqError> {
    if vector < 32 || vector == SPURIOUS_VECTOR {
        return Err(IrqError::Invalid);
    }
    if is_x2apic() {
        wrmsr(0x83f, u64::from(vector));
        Ok(())
    } else {
        send(id(), u32::from(vector))
    }
}

pub fn timer_set_lvt(value: u32) {
    write(LVT_TIMER, value);
}
pub fn timer_set_divide(value: u32) {
    write(0x3e0, value);
}
pub fn timer_set_initial(value: u32) {
    write(0x380, value);
}
pub fn timer_current() -> u32 {
    read(0x390)
}
pub fn tsc_deadline_supported() -> bool {
    crate::cpuid::has(crate::cpuid::Feature::TscDeadline)
}
pub fn set_tsc_deadline(value: u64) {
    wrmsr(0x6e0, value);
}
