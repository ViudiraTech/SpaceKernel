//! Platform-Level Interrupt Controller (PLIC) driver for RISC-V.

use crate::{
    boot,
    hardware::{
        acpi::{Acpi, MadtEntry},
        fdt::Fdt,
    },
    irq::{self, Handler, IrqError},
    mm::vmm,
    sync::SpinLock,
};
use core::sync::atomic::{AtomicUsize, Ordering};

const DEFAULT_PLIC_PHYS: u64 = 0x0c00_0000;
const DEFAULT_PLIC_SIZE: usize = 0x0040_0000;
const MAX_SOURCES: usize = 1024;

const PRIORITY_OFFSET: usize = 0x000000;
const PENDING_OFFSET: usize = 0x001000;
const ENABLE_OFFSET: usize = 0x002000;
const THRESHOLD_OFFSET: usize = 0x200000;
const CLAIM_OFFSET: usize = 0x200004;

static PLIC_BASE: AtomicUsize = AtomicUsize::new(0);
static PLIC_LOCK: SpinLock<()> = SpinLock::new(());

fn read32(offset: usize) -> u32 {
    let base = PLIC_BASE.load(Ordering::Relaxed);
    // SAFETY: callers supply mapped PLIC register offsets.
    unsafe { ((base + offset) as *const u32).read_volatile() }
}

fn write32(offset: usize, value: u32) {
    let base = PLIC_BASE.load(Ordering::Relaxed);
    // SAFETY: callers supply mapped PLIC register offsets.
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}

fn discover_from_acpi() -> Option<(u64, usize)> {
    let rsdp = boot::rsdp_address()?;
    let acpi = Acpi::from_rsdp(rsdp).ok()?;
    let madt = acpi.madt().ok()??;
    for entry in madt.entries().filter_map(Result::ok) {
        if let MadtEntry::Plic { address, size, .. } = entry {
            if address != 0 {
                return Some((address, size as usize));
            }
        }
    }
    None
}

fn discover_from_fdt() -> Option<(u64, usize)> {
    let dtb = boot::dtb_address()?;
    let tree = Fdt::from_boot_address(dtb).ok()?;
    let node = tree
        .find_compatible("riscv,plic0")
        .or_else(|| tree.find_compatible("sifive,plic-1.0.0"))?;
    let mut reg = node.reg()?;
    let entry = reg.next()?;
    Some((entry.address, entry.size as usize))
}

fn discover() -> (u64, usize) {
    discover_from_acpi()
        .or_else(discover_from_fdt)
        .unwrap_or((DEFAULT_PLIC_PHYS, DEFAULT_PLIC_SIZE))
}

/// S-mode context index for the given hart ID.
/// In standard RISC-V systems (including QEMU virt), M-mode context is 2*hart,
/// and S-mode context is 2*hart + 1.
#[inline]
pub fn context_for_hart(hart_id: usize) -> usize {
    hart_id * 2 + 1
}

pub fn init_bsp() -> Result<(), IrqError> {
    let (phys, size) = discover();
    let mapped = vmm::map_device_range(phys, size).map_err(|_| IrqError::Mapping)? as usize;
    PLIC_BASE.store(mapped, Ordering::Release);

    let bsp_context = context_for_hart(0);

    // Disable all interrupt sources in BSP context
    for word in 0..(MAX_SOURCES / 32) {
        let enable_offset = ENABLE_OFFSET + 0x80 * bsp_context + word * 4;
        write32(enable_offset, 0);
    }

    // Set priority threshold for BSP context to 0 (accept all priority > 0)
    let threshold_offset = THRESHOLD_OFFSET + 0x1000 * bsp_context;
    write32(threshold_offset, 0);

    // Clear all source priorities
    for source in 1..MAX_SOURCES {
        write32(PRIORITY_OFFSET + source * 4, 0);
    }

    // Enable Supervisor External Interrupts (SEIE = bit 9) in sie CSR
    // SAFETY: modifying supervisor interrupt enable CSR.
    unsafe {
        core::arch::asm!("csrs sie, {mask}", mask = in(reg) (1usize << 9), options(nomem, nostack));
    }

    crate::kinfo!(
        "PLIC: initialized at phys {:#x}, context={}",
        phys,
        bsp_context
    );
    Ok(())
}

pub fn ready() -> bool {
    PLIC_BASE.load(Ordering::Acquire) != 0
}

pub fn init_cpu(hart_id: usize) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    let context = context_for_hart(hart_id);

    // Disable all interrupt sources for this context
    for word in 0..(MAX_SOURCES / 32) {
        let enable_offset = ENABLE_OFFSET + 0x80 * context + word * 4;
        write32(enable_offset, 0);
    }

    // Threshold = 0
    let threshold_offset = THRESHOLD_OFFSET + 0x1000 * context;
    write32(threshold_offset, 0);

    // Enable SEIE on this hart
    // SAFETY: modifying supervisor interrupt enable CSR.
    unsafe {
        core::arch::asm!("csrs sie, {mask}", mask = in(reg) (1usize << 9), options(nomem, nostack));
    }

    Ok(())
}

pub fn set_priority(source: u32, priority: u32) {
    if (1..MAX_SOURCES as u32).contains(&source) {
        write32(PRIORITY_OFFSET + (source as usize) * 4, priority);
    }
}

pub fn set_threshold(context: usize, threshold: u32) {
    write32(THRESHOLD_OFFSET + 0x1000 * context, threshold);
}

pub fn set_mask(source: u32, masked: bool) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if source == 0 || source >= MAX_SOURCES as u32 {
        return Err(IrqError::Invalid);
    }

    let context = context_for_hart(0);
    let offset = ENABLE_OFFSET + 0x80 * context + (source as usize / 32) * 4;
    let bit = 1 << (source % 32);

    let _guard = PLIC_LOCK.lock();
    let mut current = read32(offset);
    if masked {
        current &= !bit;
    } else {
        current |= bit;
    }
    write32(offset, current);

    Ok(())
}

pub fn request(source: u32, handler: Handler) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if source == 0 || source >= MAX_SOURCES as u32 {
        return Err(IrqError::Invalid);
    }

    irq::register(source, handler)?;

    // Set priority = 1 (active)
    set_priority(source, 1);

    // Unmask in PLIC
    if let Err(e) = set_mask(source, false) {
        let _ = irq::unregister(source);
        return Err(e);
    }

    Ok(())
}

pub fn release(source: u32) -> Result<(), IrqError> {
    set_mask(source, true)?;
    set_priority(source, 0);
    irq::unregister(source)
}

pub fn claim(context: usize) -> u32 {
    read32(CLAIM_OFFSET + 0x1000 * context)
}

pub fn complete(context: usize, source: u32) {
    write32(CLAIM_OFFSET + 0x1000 * context, source);
}

/// Dispatches all pending interrupts for the BSP context.
pub fn handle_irq() {
    if !ready() {
        return;
    }
    let context = context_for_hart(0);
    loop {
        let source = claim(context);
        if source == 0 {
            break;
        }
        irq::dispatch(source);
        complete(context, source);
    }
}
