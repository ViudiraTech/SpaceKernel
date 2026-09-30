/*
 *
 *       src/arch/aarch64/gic/mod.rs
 *       Arm Generic Interrupt Controller discovery and common IRQ-domain API
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Arm Generic Interrupt Controller discovery and common IRQ-domain API.
pub mod timer;
mod v2;
mod v3;

use crate::{
    boot,
    hardware::fdt::Fdt,
    irq::{self, Handler, IrqError},
    mm::vmm,
};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

const MAX_INTID: u32 = 1020;
static VERSION: AtomicU8 = AtomicU8::new(0);
static DISTRIBUTOR: AtomicUsize = AtomicUsize::new(0);
static CPU_IF: AtomicUsize = AtomicUsize::new(0);
static REDISTRIBUTOR: AtomicUsize = AtomicUsize::new(0);
static REDISTRIBUTOR_LENGTH: AtomicUsize = AtomicUsize::new(0);
static MAX_SPI: AtomicUsize = AtomicUsize::new(0);

pub(super) fn read32(base: usize, offset: usize) -> u32 {
    // SAFETY: callers use mapped GIC register pages.
    unsafe { ((base + offset) as *const u32).read_volatile() }
}
pub(super) fn write32(base: usize, offset: usize, value: u32) {
    // SAFETY: callers use mapped GIC register pages.
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}

fn discover_from_acpi() -> Option<(u8, u64, u64, u64, u64)> {
    let rsdp = boot::rsdp_address()?;
    let acpi = crate::hardware::acpi::Acpi::from_rsdp(rsdp).ok()?;
    let madt = acpi.madt().ok()??;
    let mut gicd = None;
    let mut gicr = None;
    let mut gicc = None;
    for entry in madt.entries().filter_map(Result::ok) {
        match entry {
            crate::hardware::acpi::MadtEntry::Gicd {
                address, version, ..
            } => {
                gicd = Some((address, version));
            }
            crate::hardware::acpi::MadtEntry::Gicr { address, length } => {
                gicr = Some((address, length));
            }
            crate::hardware::acpi::MadtEntry::Gicc { address, .. }
                if gicc.is_none() && address != 0 =>
            {
                gicc = Some(address);
            }
            _ => {}
        }
    }
    let (dist_addr, version) = gicd?;
    if version == 3 || gicr.is_some() {
        let (redist_addr, redist_len) = gicr.unwrap_or((0x080a_0000, 0x20_0000));
        Some((3, dist_addr, 0x10000, redist_addr, redist_len as u64))
    } else {
        let cpu_addr = gicc.unwrap_or(0x0801_0000);
        Some((2, dist_addr, 0x1000, cpu_addr, 0x2000))
    }
}

fn discover_from_fdt() -> Option<(u8, u64, u64, u64, u64)> {
    let dtb = boot::dtb_address()?;
    let tree = Fdt::from_boot_address(dtb).ok()?;
    let node = tree
        .find_compatible("arm,gic-v3")
        .or_else(|| tree.find_compatible("arm,cortex-a15-gic"))?;
    let version = if node.is_compatible("arm,gic-v3") {
        3
    } else {
        2
    };
    let mut reg = node.reg()?;
    let dist = reg.next()?;
    let second = reg.next()?;
    Some((
        version,
        dist.address,
        dist.size,
        second.address,
        second.size,
    ))
}

pub fn init_bsp() -> Result<(), IrqError> {
    let (version, dist_phys, dist_length, second_phys, second_length) = discover_from_acpi()
        .or_else(discover_from_fdt)
        .ok_or(IrqError::NoController)?;

    if dist_length < 4096 || dist_phys & 0xfff != 0 {
        return Err(IrqError::Invalid);
    }
    let dist = vmm::map_device_range(dist_phys, dist_length as usize)
        .map_err(|_| IrqError::Mapping)? as usize;
    DISTRIBUTOR.store(dist, Ordering::Relaxed);
    if second_phys & 0xfff != 0 || second_length < 4096 {
        return Err(IrqError::Invalid);
    }
    if version == 2 {
        let cpu = vmm::map_device_range(second_phys, second_length as usize)
            .map_err(|_| IrqError::Mapping)? as usize;
        CPU_IF.store(cpu, Ordering::Relaxed);
        v2::init(dist, cpu)?;
    } else {
        REDISTRIBUTOR.store(second_phys as usize, Ordering::Relaxed);
        REDISTRIBUTOR_LENGTH.store(second_length as usize, Ordering::Relaxed);
        v3::init(dist, second_phys, second_length)?;
    }
    let max = (((read32(dist, 4) & 0x1f) + 1) * 32).min(MAX_INTID);
    MAX_SPI.store(max as usize, Ordering::Relaxed);
    VERSION.store(version, Ordering::Release);
    crate::kinfo!("GICv{}: {} interrupt IDs", version, max);
    Ok(())
}

pub fn ready() -> bool {
    VERSION.load(Ordering::Acquire) != 0
}

/// An AP calls this after loading its vector table.
pub fn init_cpu() -> Result<(), IrqError> {
    match VERSION.load(Ordering::Acquire) {
        2 => v2::init_cpu(CPU_IF.load(Ordering::Relaxed)),
        3 => v3::init_cpu(
            REDISTRIBUTOR.load(Ordering::Relaxed) as u64,
            REDISTRIBUTOR_LENGTH.load(Ordering::Relaxed) as u64,
        ),
        _ => Err(IrqError::NoController),
    }
}

pub fn request(intid: u32, handler: Handler, edge: bool) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if !(16..MAX_SPI.load(Ordering::Acquire) as u32).contains(&intid) {
        return Err(IrqError::Invalid);
    }
    irq::register(intid, handler)?;
    let result = match VERSION.load(Ordering::Acquire) {
        2 => v2::configure(DISTRIBUTOR.load(Ordering::Relaxed), intid, edge),
        3 => v3::configure(DISTRIBUTOR.load(Ordering::Relaxed), intid, edge),
        _ => Err(IrqError::NoController),
    };
    if result.is_err() {
        let _ = irq::unregister(intid);
    }
    result
}

pub fn set_mask(intid: u32, masked: bool) -> Result<(), IrqError> {
    if !(16..MAX_SPI.load(Ordering::Acquire) as u32).contains(&intid) {
        return Err(IrqError::Invalid);
    }
    match VERSION.load(Ordering::Acquire) {
        2 => v2::mask(DISTRIBUTOR.load(Ordering::Relaxed), intid, masked),
        3 => v3::mask(DISTRIBUTOR.load(Ordering::Relaxed), intid, masked),
        _ => Err(IrqError::NoController),
    }
}

pub fn release(intid: u32) -> Result<(), IrqError> {
    set_mask(intid, true)?;
    irq::unregister(intid)
}

pub fn handle_irq() {
    if !ready() {
        return;
    }
    let value = if VERSION.load(Ordering::Acquire) == 2 {
        v2::acknowledge(CPU_IF.load(Ordering::Relaxed))
    } else {
        v3::acknowledge()
    };
    let intid = value & 0x3ff;
    if intid >= 1020 {
        return;
    }
    irq::dispatch(intid);
    if VERSION.load(Ordering::Relaxed) == 2 {
        v2::eoi(CPU_IF.load(Ordering::Relaxed), value);
    } else {
        v3::eoi(value);
    }
}

pub fn send_sgi(intid: u8, affinity: u64) -> Result<(), IrqError> {
    if intid >= 16 {
        return Err(IrqError::Invalid);
    }
    match VERSION.load(Ordering::Acquire) {
        2 => v2::send_sgi(DISTRIBUTOR.load(Ordering::Relaxed), intid, affinity),
        3 => v3::send_sgi(intid, affinity),
        _ => Err(IrqError::NoController),
    }
}

/// Program an already registered SGI/PPI on this CPU without global registration.
pub fn configure_local(intid: u32, edge: bool) -> Result<(), IrqError> {
    if intid >= 32 {
        return Err(IrqError::Invalid);
    }
    match VERSION.load(Ordering::Acquire) {
        2 => v2::configure_local(DISTRIBUTOR.load(Ordering::Relaxed), intid, edge),
        3 => v3::configure_local(intid, edge),
        _ => Err(IrqError::NoController),
    }
}
