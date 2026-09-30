/*
 *
 *       src/arch/riscv64/plic.rs
 *       Platform-Level Interrupt Controller (PLIC) driver for RISC-V
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Platform-Level Interrupt Controller (PLIC) driver for RISC-V.

use crate::{
    boot,
    hardware::fdt::Fdt,
    irq::{self, Handler, IrqError},
    mm::vmm,
    sync::SpinLock,
};
use core::sync::atomic::{AtomicUsize, Ordering};

const MAX_SOURCES: usize = 1024;
const ENABLE_OFFSET: usize = 0x002000;
const THRESHOLD_OFFSET: usize = 0x200000;
const CLAIM_OFFSET: usize = 0x200004;
const NO_CONTEXT: usize = usize::MAX;

static PLIC_BASE: AtomicUsize = AtomicUsize::new(0);
static PLIC_SIZE: AtomicUsize = AtomicUsize::new(0);
static SOURCE_COUNT: AtomicUsize = AtomicUsize::new(0);
static CONTEXTS: [AtomicUsize; 256] = [const { AtomicUsize::new(NO_CONTEXT) }; 256];
static PLIC_LOCK: SpinLock<()> = SpinLock::new(());

fn read32(offset: usize) -> u32 {
    // SAFETY: all call sites validate the source/context within the mapped bank.
    unsafe { ((PLIC_BASE.load(Ordering::Acquire) + offset) as *const u32).read_volatile() }
}
fn write32(offset: usize, value: u32) {
    // SAFETY: all call sites validate the source/context within the mapped bank.
    unsafe { ((PLIC_BASE.load(Ordering::Acquire) + offset) as *mut u32).write_volatile(value) }
}

fn valid_context(context: usize, length: usize) -> bool {
    context
        .checked_mul(0x1000)
        .and_then(|n| CLAIM_OFFSET.checked_add(n))
        .and_then(|n| n.checked_add(4))
        .is_some_and(|end| end <= length)
        && context
            .checked_mul(0x80)
            .and_then(|n| ENABLE_OFFSET.checked_add(n))
            .and_then(|n| n.checked_add(0x80))
            .is_some_and(|end| end <= length)
}

/// Discover context indices from interrupts-extended, which need not follow
/// 2*hart+1 and may contain noncontiguous hart IDs or reordered CPU nodes.
pub fn init_bsp() -> Result<(), IrqError> {
    if ready() {
        return Err(IrqError::InUse);
    }
    let tree = boot::dtb_address()
        .and_then(|addr| Fdt::from_boot_address(addr).ok())
        .ok_or(IrqError::NoController)?;
    let node = tree
        .find_compatible("riscv,plic0")
        .or_else(|| tree.find_compatible("sifive,plic-1.0.0"))
        .ok_or(IrqError::NoController)?;
    let reg = node
        .reg()
        .and_then(|mut regs| regs.next())
        .ok_or(IrqError::Invalid)?;
    let sources = node.cell("riscv,ndev").ok_or(IrqError::Invalid)? as usize;
    if sources == 0 || sources >= MAX_SOURCES || reg.address & 0xfff != 0 {
        return Err(IrqError::Invalid);
    }
    let extended = node
        .property("interrupts-extended")
        .ok_or(IrqError::Unsupported)?;
    if extended.value.len() % 8 != 0 {
        return Err(IrqError::Invalid);
    }
    let mut contexts = [NO_CONTEXT; 256];
    for (index, context) in contexts.iter_mut().enumerate().take(boot::cpu_count()) {
        let hart = boot::cpu_hardware_id(index).ok_or(IrqError::Invalid)?;
        let cpu = tree
            .cpus()
            .find(|cpu| {
                cpu.reg()
                    .and_then(|mut regs| regs.next())
                    .is_some_and(|reg| reg.address == hart)
            })
            .ok_or(IrqError::Unsupported)?;
        let intc = cpu
            .children()
            .find(|child| child.is_compatible("riscv,cpu-intc"))
            .ok_or(IrqError::Unsupported)?;
        if intc.cell("#interrupt-cells") != Some(1) {
            return Err(IrqError::Unsupported);
        }
        let phandle = intc.phandle().ok_or(IrqError::Invalid)?;
        for (position, entry) in extended.value.chunks_exact(8).enumerate() {
            if u32::from_be_bytes(entry[..4].try_into().unwrap()) == phandle
                && u32::from_be_bytes(entry[4..].try_into().unwrap()) == 9
            {
                if *context != NO_CONTEXT || !valid_context(position, reg.size as usize) {
                    return Err(IrqError::Invalid);
                }
                *context = position;
            }
        }
    }
    if contexts[boot::bsp_cpu_index()] == NO_CONTEXT {
        return Err(IrqError::Unsupported);
    }
    let mapped = vmm::map_device_range(reg.address, reg.size as usize)
        .map_err(|_| IrqError::Mapping)? as usize;
    PLIC_SIZE.store(reg.size as usize, Ordering::Relaxed);
    SOURCE_COUNT.store(sources + 1, Ordering::Relaxed);
    for (to, value) in CONTEXTS.iter().zip(contexts) {
        to.store(value, Ordering::Relaxed);
    }
    PLIC_BASE.store(mapped, Ordering::Release);
    for source in 1..=sources {
        write32(source * 4, 0);
    }
    init_cpu(boot::cpu_hardware_id(boot::bsp_cpu_index()).ok_or(IrqError::Invalid)? as usize)?;
    crate::kinfo!(
        "PLIC: initialized at phys {:#x}, context={}",
        reg.address,
        current_context()?
    );
    Ok(())
}

pub fn ready() -> bool {
    PLIC_BASE.load(Ordering::Acquire) != 0
}

pub fn context_for_hart(hart_id: usize) -> Option<usize> {
    if !ready() {
        return None;
    }
    let index =
        (0..boot::cpu_count()).find(|&i| boot::cpu_hardware_id(i) == Some(hart_id as u64))?;
    let context = CONTEXTS[index].load(Ordering::Relaxed);
    (context != NO_CONTEXT).then_some(context)
}

fn current_context() -> Result<usize, IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    let context = CONTEXTS[super::current_cpu_index().ok_or(IrqError::NotRegistered)?]
        .load(Ordering::Relaxed);
    if context == NO_CONTEXT {
        return Err(IrqError::Unsupported);
    }
    Ok(context)
}

pub fn init_cpu(hart_id: usize) -> Result<(), IrqError> {
    let context = context_for_hart(hart_id).ok_or(IrqError::Unsupported)?;
    for word in 0..SOURCE_COUNT.load(Ordering::Relaxed).div_ceil(32) {
        write32(ENABLE_OFFSET + 0x80 * context + word * 4, 0);
    }
    set_threshold(context, 0)?;
    // SAFETY: configure supervisor external interrupt delivery on this hart.
    unsafe {
        core::arch::asm!("csrs sie, {}", in(reg) 1usize << 9, options(nomem,nostack));
    }
    Ok(())
}

pub fn set_priority(source: u32, priority: u32) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if source == 0 || source as usize >= SOURCE_COUNT.load(Ordering::Relaxed) {
        return Err(IrqError::Invalid);
    }
    write32(source as usize * 4, priority);
    Ok(())
}

pub fn set_threshold(context: usize, threshold: u32) -> Result<(), IrqError> {
    if !ready() {
        return Err(IrqError::NoController);
    }
    if !valid_context(context, PLIC_SIZE.load(Ordering::Relaxed)) {
        return Err(IrqError::Invalid);
    }
    write32(THRESHOLD_OFFSET + 0x1000 * context, threshold);
    Ok(())
}

pub fn set_mask(source: u32, masked: bool) -> Result<(), IrqError> {
    let context = current_context()?;
    if source == 0 || source as usize >= SOURCE_COUNT.load(Ordering::Relaxed) {
        return Err(IrqError::Invalid);
    }
    let offset = ENABLE_OFFSET + 0x80 * context + source as usize / 32 * 4;
    let bit = 1 << (source % 32);
    let _guard = PLIC_LOCK.lock();
    let old = read32(offset);
    write32(offset, if masked { old & !bit } else { old | bit });
    Ok(())
}

pub fn request(source: u32, handler: Handler) -> Result<(), IrqError> {
    let _ = current_context()?;
    if source == 0 || source as usize >= SOURCE_COUNT.load(Ordering::Relaxed) {
        return Err(IrqError::Invalid);
    }
    irq::register(source, handler)?;
    set_priority(source, 1)?;
    if let Err(error) = set_mask(source, false) {
        let _ = irq::unregister(source);
        return Err(error);
    }
    Ok(())
}

pub fn release(source: u32) -> Result<(), IrqError> {
    set_mask(source, true)?;
    set_priority(source, 0)?;
    irq::unregister(source)
}

pub fn claim(context: usize) -> u32 {
    if !ready() || !valid_context(context, PLIC_SIZE.load(Ordering::Relaxed)) {
        return 0;
    }
    read32(CLAIM_OFFSET + 0x1000 * context)
}
pub fn complete(context: usize, source: u32) {
    if ready()
        && valid_context(context, PLIC_SIZE.load(Ordering::Relaxed))
        && source != 0
        && (source as usize) < SOURCE_COUNT.load(Ordering::Relaxed)
    {
        write32(CLAIM_OFFSET + 0x1000 * context, source);
    }
}

pub fn handle_irq() {
    let Ok(context) = current_context() else {
        return;
    };
    // Bound draining so a permanently asserted device cannot starve the CPU.
    for _ in 0..MAX_SOURCES {
        let source = claim(context);
        if source == 0 {
            break;
        }
        if source as usize >= SOURCE_COUNT.load(Ordering::Relaxed) {
            break;
        }
        irq::dispatch(source);
        complete(context, source);
    }
}
