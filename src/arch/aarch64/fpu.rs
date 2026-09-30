/*
 *
 *       src/arch/aarch64/fpu.rs
 *       AArch64 FP/AdvSIMD state: all 32 Q registers plus FPCR and FPSR
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! AArch64 FP/AdvSIMD state: all 32 Q registers plus FPCR and FPSR.
use crate::{
    cpuid::{CpuInfo, Feature},
    fpu::{Config, Error, Format},
};
use core::arch::{asm, global_asm};

global_asm!(include_str!("fpu.S"));
unsafe extern "C" {
    fn spacekernel_fpsimd_save(pointer: *mut u8);
    fn spacekernel_fpsimd_restore(pointer: *const u8);
}

#[inline(never)]
pub fn init(cpu: &CpuInfo) -> Result<Config, Error> {
    let mut cpacr: u64;
    // SAFETY: disable FP/SIMD and all unmanaged SVE/SME state before use.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) cpacr, options(nomem,nostack));
        cpacr &= !((3 << 20) | (3 << 16) | (3 << 24));
        asm!("msr cpacr_el1, {}", "isb", in(reg) cpacr, options(nostack));
    }
    if !cpu.features.contains(Feature::Fpu) || !cpu.features.contains(Feature::AdvSimd) {
        return Ok(Config::unavailable());
    }
    Ok(Config {
        format: Format::FpSimd,
        size: 528,
        alignment: 64,
        xcr0: 0,
        mxcsr_mask: 0,
    })
}

pub unsafe fn initialize_image(_config: Config, _pointer: *mut u8) {} // Zero is the architectural default.

pub fn validate(config: Config, bytes: &[u8]) -> bool {
    if config.format != Format::FpSimd || bytes.len() != 528 {
        return false;
    }
    let fpcr = u32::from_le_bytes(bytes[512..516].try_into().unwrap());
    let fpsr = u32::from_le_bytes(bytes[516..520].try_into().unwrap());
    fpcr & !0x07f7_9f00 == 0 && fpsr & !0xf800_009f == 0 && bytes[520..].iter().all(|b| *b == 0)
}

pub unsafe fn enable() -> u64 {
    let old: u64;
    // SAFETY: caller owns local FP registers; ISB completes the access change.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) old, options(nomem,nostack));
        asm!("msr cpacr_el1, {}", "isb", in(reg) old | (3 << 20), options(nostack));
    }
    old & (3 << 20)
}

pub fn disable() {
    let old: u64;
    // SAFETY: disable local access without changing the FP register file.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) old, options(nomem,nostack));
        asm!("msr cpacr_el1, {}", "isb", in(reg) old & !(3 << 20), options(nostack));
    }
}

pub unsafe fn restore_gate(gate: u64) {
    let old: u64;
    // SAFETY: restore only the saved FPEN field; retain unrelated controls.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) old, options(nomem,nostack));
        asm!("msr cpacr_el1, {}", "isb", in(reg) (old & !(3 << 20)) | gate, options(nostack));
    }
}

#[inline(never)]
pub unsafe fn save(_config: Config, pointer: *mut u8) {
    // SAFETY: caller enabled FP access and owns an aligned 528-byte area.
    unsafe {
        spacekernel_fpsimd_save(pointer);
    }
}
#[inline(never)]
pub unsafe fn restore(_config: Config, pointer: *const u8) {
    // SAFETY: image/control registers were validated; soft-float Rust owns no Q registers.
    unsafe {
        spacekernel_fpsimd_restore(pointer);
    }
}

#[cfg(feature = "boot-self-test")]
pub fn is_enabled() -> bool {
    let value: u64;
    // SAFETY: reads the local FP access control.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) value, options(nomem,nostack));
    }
    value & (3 << 20) == 3 << 20
}
