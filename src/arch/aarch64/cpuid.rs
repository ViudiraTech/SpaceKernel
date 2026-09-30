/*
 *
 *       src/arch/aarch64/cpuid.rs
 *       CPU identity and capability discovery from EL1 architectural ID registers
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! CPU identity and capability discovery from EL1 architectural ID registers.

use crate::cpuid::{Architecture, CpuInfo, Feature};
use core::arch::asm;

pub fn detect(_index: usize) -> CpuInfo {
    let (midr, mpidr, pfr0, isar0, mmfr0, frequency): (u64, u64, u64, u64, u64, u64);
    // SAFETY: these architectural identification registers are readable at EL1.
    unsafe {
        asm!("mrs {midr}, midr_el1", "mrs {mpidr}, mpidr_el1", "mrs {pfr0}, id_aa64pfr0_el1",
            "mrs {isar0}, id_aa64isar0_el1", "mrs {mmfr0}, id_aa64mmfr0_el1", "mrs {freq}, cntfrq_el0",
            midr=out(reg) midr, mpidr=out(reg) mpidr, pfr0=out(reg) pfr0,
            isar0=out(reg) isar0, mmfr0=out(reg) mmfr0, freq=out(reg) frequency, options(nomem,nostack));
    }
    let mut info = CpuInfo::empty(Architecture::Aarch64);
    info.hardware_id = mpidr & 0xff00_ffffff;
    info.signature = midr;
    info.family = ((midr >> 24) & 255) as u16;
    info.model_id = ((midr >> 4) & 0xfff) as u16;
    info.stepping = (midr & 15) as u8;
    info.vendor_id = Some(u64::from(info.family));
    // MIDR's variant and revision form rNpM; retain both, not only M.
    info.revision_id = Some(((midr >> 16) & 0xf0) | (midr & 15));
    info.vendor.set(match info.family {
        0x41 => b"Arm",
        0x43 => b"Cavium",
        0x51 => b"Qualcomm",
        0x61 => b"Apple",
        _ => b"Unknown",
    });
    info.model.set(b"AArch64 processor (MIDR identifies part)");
    info.physical_bits = [32, 36, 40, 42, 44, 48, 52, 56]
        .get((mmfr0 & 15) as usize)
        .copied();
    info.virtual_bits = Some(48);
    if (pfr0 >> 16) & 15 != 15 {
        info.features.insert(Feature::Fpu);
        info.features.insert(Feature::DoublePrecision);
    }
    if (pfr0 >> 20) & 15 != 15 {
        info.features.insert(Feature::Simd);
        info.features.insert(Feature::AdvSimd);
    }
    if (pfr0 >> 32) & 15 != 0 {
        info.features.insert(Feature::Sve);
    }
    for (shift, feature) in [
        (4, Feature::Aes),
        (8, Feature::Sha),
        (12, Feature::Sha),
        (16, Feature::Crc32),
    ] {
        if (isar0 >> shift) & 15 != 0 {
            info.features.insert(feature);
        }
    }
    if (isar0 >> 20) & 15 >= 2 {
        info.features.insert(Feature::Atomics);
    }
    info.features.insert(Feature::Nx);
    info.counter_hz = (frequency != 0).then_some(frequency);
    info
}
