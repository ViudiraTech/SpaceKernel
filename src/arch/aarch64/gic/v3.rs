//! ARM Generic Interrupt Controller v3 (GICv3) driver.

use super::{read32, write32};
use crate::{irq::IrqError, mm::vmm};
use core::sync::atomic::{AtomicUsize, Ordering};

const GICD_CTLR: usize = 0x0000;
const GICD_TYPER: usize = 0x0004;
const GICD_IGROUPR: usize = 0x0080;
const GICD_ISENABLER: usize = 0x0100;
const GICD_ICENABLER: usize = 0x0180;
const GICD_ISPENDR: usize = 0x0200;
const GICD_ICPENDR: usize = 0x0280;
const GICD_IPRIORITYR: usize = 0x0400;
const GICD_ICFGR: usize = 0x0c00;
const GICD_IROUTER: usize = 0x6000;

const GICR_TYPER: usize = 0x0008;
const GICR_WAKER: usize = 0x0014;

const GICR_SGI_OFFSET: usize = 0x10000;
const GICR_IGROUPR0: usize = 0x0080;
const GICR_ISENABLER0: usize = 0x0100;
const GICR_ICENABLER0: usize = 0x0180;
const GICR_ISPENDR0: usize = 0x0200;
const GICR_ICPENDR0: usize = 0x0280;
const GICR_IPRIORITYR: usize = 0x0400;

static MAPPED_REDIST: AtomicUsize = AtomicUsize::new(0);

fn read64(base: usize, offset: usize) -> u64 {
    // SAFETY: callers supply mapped GIC MMIO address.
    unsafe { ((base + offset) as *const u64).read_volatile() }
}

fn write64(base: usize, offset: usize, value: u64) {
    // SAFETY: callers supply mapped GIC MMIO address.
    unsafe { ((base + offset) as *mut u64).write_volatile(value) }
}

#[inline]
unsafe fn read_icc_sre_el1() -> u64 {
    let val: u64;
    unsafe {
        core::arch::asm!("mrs {}, ICC_SRE_EL1", out(reg) val, options(nomem, nostack));
    }
    val
}

#[inline]
unsafe fn write_icc_sre_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_SRE_EL1, {}", in(reg) val, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_icc_pmr_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_PMR_EL1, {}", in(reg) val, options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_icc_bpr1_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_BPR1_EL1, {}", in(reg) val, options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_icc_ctlr_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_CTLR_EL1, {}", in(reg) val, options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_icc_igrpen1_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_IGRPEN1_EL1, {}", in(reg) val, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

#[inline]
unsafe fn read_icc_iar1_el1() -> u32 {
    let val: u64;
    unsafe {
        core::arch::asm!("mrs {}, ICC_IAR1_EL1", out(reg) val, options(nomem, nostack));
    }
    val as u32
}

#[inline]
unsafe fn write_icc_eoir1_el1(val: u32) {
    unsafe {
        core::arch::asm!("msr ICC_EOIR1_EL1, {}", in(reg) u64::from(val), options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

#[inline]
unsafe fn write_icc_sgi1r_el1(val: u64) {
    unsafe {
        core::arch::asm!("msr ICC_SGI1R_EL1, {}", in(reg) val, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }
}

fn wait_for_rwp(dist: usize) {
    // Wait until bit 31 (RWP) clears
    while (read32(dist, GICD_CTLR) & (1 << 31)) != 0 {
        core::hint::spin_loop();
    }
}

pub fn init(dist: usize, redist_phys: u64, redist_length: u64) -> Result<(), IrqError> {
    let redist_mapped = vmm::map_device_range(redist_phys, redist_length as usize)
        .map_err(|_| IrqError::Mapping)? as usize;
    MAPPED_REDIST.store(redist_mapped, Ordering::Release);

    // Disable distributor while programming
    write32(dist, GICD_CTLR, 0);
    wait_for_rwp(dist);

    let typer = read32(dist, GICD_TYPER);
    let max_intid = (((typer & 0x1f) + 1) * 32).min(1020);

    // Configure SPIs (>= 32)
    let mut irq = 32;
    while irq < max_intid {
        let reg_offset = (irq as usize / 32) * 4;
        // Group 1 Non-Secure
        write32(dist, GICD_IGROUPR + reg_offset, 0xffff_ffff);
        // Disable
        write32(dist, GICD_ICENABLER + reg_offset, 0xffff_ffff);
        // Clear pending
        write32(dist, GICD_ICPENDR + reg_offset, 0xffff_ffff);
        irq += 32;
    }

    let mut irq = 32;
    while irq < max_intid {
        // Default priority
        write32(dist, GICD_IPRIORITYR + (irq as usize), 0xa0a0_a0a0);
        // Level triggered
        write32(dist, GICD_ICFGR + (irq as usize / 16) * 4, 0);
        // Default route: any PE (bit 31 = 1)
        write64(dist, GICD_IROUTER + (irq as usize) * 8, 1 << 31);
        irq += 4;
    }

    // Enable Group 0, Group 1NS and Affinity Routing (ARE_NS)
    // ARE_NS = bit 4, EnableGrp1NS = bit 1, EnableGrp0 = bit 0
    write32(dist, GICD_CTLR, (1 << 4) | (1 << 1) | (1 << 0));
    wait_for_rwp(dist);

    init_cpu(redist_phys, redist_length)
}

pub fn init_cpu(redist_phys: u64, redist_length: u64) -> Result<(), IrqError> {
    let _ = (redist_phys, redist_length);
    let redist_base = MAPPED_REDIST.load(Ordering::Acquire);
    if redist_base == 0 {
        return Err(IrqError::NoController);
    }

    let mpidr: u64;
    // SAFETY: read current CPU MPIDR affinity.
    unsafe {
        core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack));
    }
    let cpu_aff = (mpidr & 0x00ff_0000_0000) | (mpidr & 0x0000_00ff_ffff);

    // Locate matching redistributor frame
    let mut offset = 0usize;
    let mut found_rd = None;
    loop {
        let rd_addr = redist_base + offset;
        let typer = read64(rd_addr, GICR_TYPER);
        let rd_aff = typer >> 32;
        if rd_aff == cpu_aff || found_rd.is_none() {
            found_rd = Some(rd_addr);
            if rd_aff == cpu_aff {
                break;
            }
        }
        let is_last = (typer & (1 << 4)) != 0;
        let stride = if (typer & (1 << 24)) != 0 {
            0x40000 // 4 * 64KB (with VLPI)
        } else {
            0x20000 // 2 * 64KB
        };
        if is_last {
            break;
        }
        offset += stride;
    }

    let rd_base = found_rd.ok_or(IrqError::NoController)?;

    // Wake up redistributor: clear ProcessorSleep and wait for ChildrenAsleep == 0
    let waker = read32(rd_base, GICR_WAKER);
    write32(rd_base, GICR_WAKER, waker & !(1 << 1));
    while (read32(rd_base, GICR_WAKER) & (1 << 2)) != 0 {
        core::hint::spin_loop();
    }

    // Initialize SGI / PPI frame
    let sgi_base = rd_base + GICR_SGI_OFFSET;
    write32(sgi_base, GICR_IGROUPR0, 0xffff_ffff); // Group 1
    write32(sgi_base, GICR_ICENABLER0, 0xffff_ffff); // Disable SGIs/PPIs
    write32(sgi_base, GICR_ICPENDR0, 0xffff_ffff); // Clear pending

    for i in (0..32).step_by(4) {
        write32(sgi_base, GICR_IPRIORITYR + i, 0xa0a0_a0a0);
    }

    // Program CPU interface system registers
    // SAFETY: operating in EL1 with architected GICv3 system registers.
    unsafe {
        let sre = read_icc_sre_el1();
        if (sre & 1) == 0 {
            write_icc_sre_el1(sre | 1);
        }
        write_icc_pmr_el1(0xff); // allow all priorities
        write_icc_bpr1_el1(0);
        write_icc_ctlr_el1(0);
        write_icc_igrpen1_el1(1); // Enable Group 1
    }

    Ok(())
}

pub fn configure(dist: usize, intid: u32, edge: bool) -> Result<(), IrqError> {
    if intid < 16 {
        return Err(IrqError::Invalid);
    }

    if intid >= 32 {
        // Group 1 Non-Secure
        let group_reg = GICD_IGROUPR + (intid as usize / 32) * 4;
        let mut group = read32(dist, group_reg);
        group |= 1 << (intid % 32);
        write32(dist, group_reg, group);

        // Edge/level configuration
        let icfgr_reg = GICD_ICFGR + (intid as usize / 16) * 4;
        let shift = (intid % 16) * 2;
        let mut icfgr = read32(dist, icfgr_reg);
        if edge {
            icfgr |= 2 << shift;
        } else {
            icfgr &= !(2 << shift);
        }
        write32(dist, icfgr_reg, icfgr);

        // Priority
        let prio_reg = GICD_IPRIORITYR + (intid as usize & !3);
        let byte_shift = (intid % 4) * 8;
        let mut prio = read32(dist, prio_reg);
        prio = (prio & !(0xff << byte_shift)) | (0xa0 << byte_shift);
        write32(dist, prio_reg, prio);

        // Routing: Any PE (bit 31 = 1)
        write64(dist, GICD_IROUTER + (intid as usize) * 8, 1 << 31);
    }

    mask(dist, intid, false)
}

pub fn mask(dist: usize, intid: u32, masked: bool) -> Result<(), IrqError> {
    if intid >= 32 {
        let reg_offset = (intid as usize / 32) * 4;
        let bit = 1 << (intid % 32);
        if masked {
            write32(dist, GICD_ICENABLER + reg_offset, bit);
        } else {
            write32(dist, GICD_ISENABLER + reg_offset, bit);
        }
        wait_for_rwp(dist);
    }
    Ok(())
}

pub fn acknowledge() -> u32 {
    // SAFETY: read IAR1 register
    unsafe { read_icc_iar1_el1() }
}

pub fn eoi(value: u32) {
    // SAFETY: write EOIR1 register
    unsafe { write_icc_eoir1_el1(value) }
}

pub fn send_sgi(intid: u8, affinity: u64) -> Result<(), IrqError> {
    if intid >= 16 {
        return Err(IrqError::Invalid);
    }
    let sgi_val = if affinity == u64::MAX {
        // IRM = 1 (all other PEs)
        (1u64 << 40) | u64::from(intid)
    } else {
        // Specific target affinity
        let aff3 = (affinity >> 24) & 0xff;
        let aff2 = (affinity >> 16) & 0xff;
        let aff1 = (affinity >> 8) & 0xff;
        let target_list = 1u64 << (affinity & 0x0f);
        (aff3 << 48) | (aff2 << 32) | (aff1 << 16) | target_list | u64::from(intid)
    };
    // SAFETY: write SGI1R register
    unsafe { write_icc_sgi1r_el1(sgi_val) }
    Ok(())
}
