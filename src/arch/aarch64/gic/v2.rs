//! ARM Generic Interrupt Controller v2 (GICv2) driver.

use super::{read32, write32};
use crate::irq::IrqError;

const GICD_CTLR: usize = 0x000;
const GICD_TYPER: usize = 0x004;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_ISPENDR: usize = 0x200;
const GICD_ICPENDR: usize = 0x280;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_ICFGR: usize = 0xc00;
const GICD_SGIR: usize = 0xf00;

const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_BPR: usize = 0x008;
const GICC_IAR: usize = 0x00c;
const GICC_EOIR: usize = 0x010;

pub fn init(dist: usize, cpu: usize) -> Result<(), IrqError> {
    // Disable distributor while programming routes
    write32(dist, GICD_CTLR, 0);

    let typer = read32(dist, GICD_TYPER);
    let max_intid = (((typer & 0x1f) + 1) * 32).min(1020);

    // Disable and clear pending state for all SPIs (>= 32)
    let mut irq = 32;
    while irq < max_intid {
        let reg_offset = (irq as usize / 32) * 4;
        write32(dist, GICD_ICENABLER + reg_offset, 0xffff_ffff);
        write32(dist, GICD_ICPENDR + reg_offset, 0xffff_ffff);
        irq += 32;
    }

    // Default priority (0xa0) and route to CPU0 (0x01) for all SPIs
    let mut irq = 32;
    while irq < max_intid {
        write32(dist, GICD_IPRIORITYR + (irq as usize), 0xa0a0_a0a0);
        write32(dist, GICD_ITARGETSR + (irq as usize), 0x0101_0101);
        irq += 4;
    }

    // Set level-sensitive (bit 1 = 0) by default for SPIs
    let mut irq = 32;
    while irq < max_intid {
        write32(dist, GICD_ICFGR + (irq as usize / 16) * 4, 0);
        irq += 16;
    }

    // Enable distributor
    write32(dist, GICD_CTLR, 1);

    // Initialize BSP CPU interface
    init_cpu(cpu)
}

pub fn init_cpu(cpu: usize) -> Result<(), IrqError> {
    if cpu == 0 {
        return Err(IrqError::NoController);
    }
    // Allow all priority levels through PMR
    write32(cpu, GICC_PMR, 0xff);
    // Flat priority without sub-priority grouping
    write32(cpu, GICC_BPR, 0x00);
    // Enable CPU interface signaling
    write32(cpu, GICC_CTLR, 0x01);
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    Ok(())
}

pub fn configure(dist: usize, intid: u32, edge: bool) -> Result<(), IrqError> {
    if intid < 16 {
        return Err(IrqError::Invalid);
    }

    // Trigger configuration in ICFGR (2 bits per intid; bit 1 is edge/level)
    let icfgr_reg = GICD_ICFGR + (intid as usize / 16) * 4;
    let shift = (intid % 16) * 2;
    let mut icfgr = read32(dist, icfgr_reg);
    if edge {
        icfgr |= 2 << shift;
    } else {
        icfgr &= !(2 << shift);
    }
    write32(dist, icfgr_reg, icfgr);

    // For SPIs (>= 32), set CPU0 target and priority
    if intid >= 32 {
        let target_reg = GICD_ITARGETSR + (intid as usize & !3);
        let byte_shift = (intid % 4) * 8;
        let mut target = read32(dist, target_reg);
        target = (target & !(0xff << byte_shift)) | (0x01 << byte_shift);
        write32(dist, target_reg, target);

        let prio_reg = GICD_IPRIORITYR + (intid as usize & !3);
        let mut prio = read32(dist, prio_reg);
        prio = (prio & !(0xff << byte_shift)) | (0xa0 << byte_shift);
        write32(dist, prio_reg, prio);
    }

    mask(dist, intid, false)
}

pub fn mask(dist: usize, intid: u32, masked: bool) -> Result<(), IrqError> {
    let reg_offset = (intid as usize / 32) * 4;
    let bit = 1 << (intid % 32);
    if masked {
        write32(dist, GICD_ICENABLER + reg_offset, bit);
    } else {
        write32(dist, GICD_ISENABLER + reg_offset, bit);
    }
    Ok(())
}

pub fn acknowledge(cpu: usize) -> u32 {
    read32(cpu, GICC_IAR)
}

pub fn eoi(cpu: usize, value: u32) {
    write32(cpu, GICC_EOIR, value);
}

pub fn send_sgi(dist: usize, intid: u8, affinity: u64) -> Result<(), IrqError> {
    if intid >= 16 {
        return Err(IrqError::Invalid);
    }
    let (filter, target_list) = if affinity == u64::MAX {
        (0b01u32, 0u32) // all other CPUs
    } else if affinity == 0 {
        (0b00u32, 0x01u32) // CPU0
    } else {
        (0b00u32, (1 << (affinity & 7)) as u32)
    };
    let value = (filter << 24) | (target_list << 16) | (u32::from(intid) & 0x0f);
    write32(dist, GICD_SGIR, value);
    Ok(())
}
