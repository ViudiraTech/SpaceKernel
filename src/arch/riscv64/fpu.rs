/*
 *
 *       src/arch/riscv64/fpu.rs
 *       RISC-V F/D state backend. The kernel uses the integer-only LP64 ABI
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! RISC-V F/D state backend. The kernel uses the integer-only LP64 ABI.
use crate::{
    cpuid::{CpuInfo, Feature},
    fpu::{Config, Error, Format},
};
use core::arch::{asm, global_asm};

global_asm!(include_str!("fpu.S"));
unsafe extern "C" {
    fn spacekernel_riscv_f_save(pointer: *mut u8);
    fn spacekernel_riscv_f_restore(pointer: *const u8);
    fn spacekernel_riscv_d_save(pointer: *mut u8);
    fn spacekernel_riscv_d_restore(pointer: *const u8);
}
const FS: u64 = 3 << 13;

#[inline(never)]
pub fn init(cpu: &CpuInfo) -> Result<Config, Error> {
    // SAFETY: turn off FP and unmanaged vector state on this supervisor hart.
    unsafe {
        asm!("csrc sstatus, {}", in(reg) FS | (3 << 9), options(nostack));
    }
    if !cpu.features.contains(Feature::RiscvF) {
        return Ok(Config::unavailable());
    }
    Ok(Config {
        format: if cpu.features.contains(Feature::RiscvD) {
            Format::RiscvD
        } else {
            Format::RiscvF
        },
        size: 264,
        alignment: 64,
        xcr0: 0,
        mxcsr_mask: 0,
    })
}

pub unsafe fn initialize_image(_config: Config, _pointer: *mut u8) {} // +0 registers, RNE, clear flags.

pub fn validate(config: Config, bytes: &[u8]) -> bool {
    if bytes.len() != 264 {
        return false;
    }
    let fcsr = u32::from_le_bytes(bytes[256..260].try_into().unwrap());
    let rounding = (fcsr >> 5) & 7;
    // frm=111 is reserved as stored state: dynamic rounding is an instruction
    // encoding that selects frm, not a valid value to put into frm itself.
    if fcsr & !255 != 0 || rounding >= 5 || bytes[260..].iter().any(|b| *b != 0) {
        return false;
    }
    if config.format == Format::RiscvF
        && bytes[..256]
            .chunks_exact(8)
            .any(|slot| slot[4..].iter().any(|b| *b != 0))
    {
        return false;
    }
    true
}

pub unsafe fn enable() -> u64 {
    let old: u64;
    // SAFETY: FS=Dirty allows register access; no speculative lazy-FPU owner.
    unsafe {
        asm!("csrrs {}, sstatus, {}", out(reg) old, in(reg) FS, options(nostack));
    }
    old & FS
}

pub fn disable() {
    // SAFETY: close FS without changing the FP register contents.
    unsafe {
        asm!("csrc sstatus, {}", in(reg) FS, options(nostack));
    }
}

pub unsafe fn restore_gate(gate: u64) {
    // SAFETY: restore only FS and retain current interrupt/privilege controls.
    // When enabled, keep FS=Dirty because the scope performed register writes.
    unsafe {
        asm!("csrc sstatus, {}", "csrs sstatus, {}", in(reg) FS, in(reg) if gate == 0 { 0 } else { FS }, options(nostack));
    }
}

#[inline(never)]
pub unsafe fn save(config: Config, pointer: *mut u8) {
    // SAFETY: valid aligned area; selected width was discovered from this hart.
    unsafe {
        if config.format == Format::RiscvD {
            spacekernel_riscv_d_save(pointer);
        } else {
            spacekernel_riscv_f_save(pointer);
        }
    }
}
#[inline(never)]
pub unsafe fn restore(config: Config, pointer: *const u8) {
    // SAFETY: validated image, local FS enabled, integer ABI owns no FP registers.
    unsafe {
        if config.format == Format::RiscvD {
            spacekernel_riscv_d_restore(pointer);
        } else {
            spacekernel_riscv_f_restore(pointer);
        }
    }
}

#[cfg(feature = "boot-self-test")]
pub fn is_enabled() -> bool {
    let value: u64;
    // SAFETY: reads the local supervisor FP gate.
    unsafe {
        asm!("csrr {}, sstatus", out(reg) value, options(nomem,nostack));
    }
    value & FS != 0
}
