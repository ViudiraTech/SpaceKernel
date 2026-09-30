/*
 *
 *       src/arch/x86_64/fpu.rs
 *       x87/MMX/SSE and XSAVE standard-format AVX/AVX-512 state backend
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! x87/MMX/SSE and XSAVE standard-format AVX/AVX-512 state backend.

use super::cpuid::query;
use crate::{
    cpuid::{CpuInfo, Feature},
    fpu::{Config, Error, Format},
};
use core::arch::asm;

const TS: u64 = 1 << 3;
const OSXSAVE: u64 = 1 << 18;

#[inline(never)]
pub fn init(cpu: &CpuInfo) -> Result<Config, Error> {
    if ![Feature::Fpu, Feature::Fxsave, Feature::Sse, Feature::Sse2]
        .iter()
        .all(|f| cpu.features.contains(*f))
    {
        disable();
        return Ok(Config::unavailable());
    }
    let (mut cr0, mut cr4): (u64, u64);
    // SAFETY: privileged initialization, before this CPU owns any task state.
    unsafe {
        asm!("mov {}, cr0", out(reg) cr0, options(nomem,nostack,preserves_flags));
        asm!("mov {}, cr4", out(reg) cr4, options(nomem,nostack,preserves_flags));
        cr0 = (cr0 | (1 << 1) | (1 << 5)) & !((1 << 2) | TS);
        cr4 |= (1 << 9) | (1 << 10);
        if cpu.features.contains(Feature::Xsave) {
            cr4 |= OSXSAVE;
        } else {
            cr4 &= !OSXSAVE;
        }
        asm!("mov cr0, {}", in(reg) cr0, options(nostack,preserves_flags));
        asm!("mov cr4, {}", in(reg) cr4, options(nostack,preserves_flags));
    }
    let mut config = Config {
        format: Format::Fxsave,
        size: 512,
        alignment: 64,
        xcr0: 0,
        mxcsr_mask: 0,
    };
    if cpu.features.contains(Feature::Xsave) {
        let r = query(0xd, 0).ok_or(Error::Unsupported)?;
        let supported = u64::from(r.eax) | u64::from(r.edx) << 32;
        if supported & 3 != 3 {
            disable();
            return Err(Error::Unsupported);
        }
        let mut mask = 3u64;
        if cpu.features.contains(Feature::Avx) && supported & 4 != 0 {
            mask |= 4;
        }
        if mask & 4 != 0 && cpu.features.contains(Feature::Avx512f) && supported & 0xe0 == 0xe0 {
            mask |= 0xe0;
        }
        // Do not enable unmanaged components such as PKRU, AMX or supervisor
        // state. CPUID.0D:EBX is re-read after selecting the exact XCR0 mask.
        unsafe {
            asm!("xsetbv", in("ecx") 0u32, in("eax") mask as u32, in("edx") (mask >> 32) as u32, options(nostack));
        }
        let size = query(0xd, 0).ok_or(Error::Unsupported)?.ebx as usize;
        if !(576..=65536).contains(&size) {
            disable();
            return Err(Error::Unsupported);
        }
        for component in 2..8 {
            if mask & (1 << component) != 0 {
                let r = query(0xd, component).ok_or(Error::Unsupported)?;
                if r.ecx & 1 != 0
                    || r.ebx < 576
                    || r.eax == 0
                    || (r.ebx as usize)
                        .checked_add(r.eax as usize)
                        .is_none_or(|end| end > size)
                {
                    disable();
                    return Err(Error::Unsupported);
                }
            }
        }
        config.format = Format::Xsave;
        config.size = size;
        config.xcr0 = mask;
    }
    #[repr(align(64))]
    struct Probe([u8; 512]);
    let mut probe = Probe([0; 512]);
    // SAFETY: CR0/CR4 allow FXSAVE and this stack buffer is aligned and sized.
    unsafe {
        asm!("fxsave64 [{}]", in(reg) probe.0.as_mut_ptr(), options(nostack));
    }
    config.mxcsr_mask = u32::from_le_bytes(probe.0[28..32].try_into().unwrap());
    if config.mxcsr_mask == 0 {
        config.mxcsr_mask = 0xffbf;
    }
    disable();
    Ok(config)
}

pub unsafe fn initialize_image(config: Config, pointer: *mut u8) {
    // SAFETY: called only with a fresh zeroed Config-sized aligned allocation.
    unsafe {
        pointer.cast::<u16>().write(0x037f);
        pointer.add(24).cast::<u32>().write(0x1f80);
        pointer.add(28).cast::<u32>().write(config.mxcsr_mask);
        if config.format == Format::Xsave {
            pointer.add(512).cast::<u64>().write(config.xcr0);
        }
    }
}

pub fn validate(config: Config, bytes: &[u8]) -> bool {
    if bytes.len() != config.size || bytes.len() < 512 {
        return false;
    }
    let mxcsr = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    if mxcsr & !config.mxcsr_mask != 0 {
        return false;
    }
    if config.format == Format::Xsave {
        if bytes.len() < 576 {
            return false;
        }
        let bitmap = u64::from_le_bytes(bytes[512..520].try_into().unwrap());
        if bitmap & !config.xcr0 != 0 || bytes[520..576].iter().any(|b| *b != 0) {
            return false;
        }
    }
    true
}

pub unsafe fn enable() -> u64 {
    let old: u64;
    // SAFETY: caller owns this CPU's register file and preserves IRQ state.
    unsafe {
        asm!("mov {}, cr0", out(reg) old, options(nomem,nostack,preserves_flags));
        asm!("clts", options(nostack, preserves_flags));
    }
    old & TS
}

pub fn disable() {
    let value: u64;
    // SAFETY: sets the local hardware gate without changing any FP registers.
    unsafe {
        asm!("mov {}, cr0", out(reg) value, options(nomem,nostack,preserves_flags));
        asm!("mov cr0, {}", in(reg) value | TS, options(nostack,preserves_flags));
    }
}

pub unsafe fn restore_gate(gate: u64) {
    if gate & TS != 0 {
        disable();
    } else {
        unsafe {
            let _ = enable();
        }
    }
}

#[inline(never)]
pub unsafe fn save(config: Config, pointer: *mut u8) {
    // SAFETY: caller enabled the gate and owns this checked save area. XSAVE,
    // rather than XSAVEOPT, also works when saving to a different task buffer.
    unsafe {
        if config.format == Format::Xsave {
            asm!("xsave64 [{}]", in(reg) pointer, in("eax") config.xcr0 as u32, in("edx") (config.xcr0 >> 32) as u32, options(nostack));
        } else {
            asm!("fxsave64 [{}]", in(reg) pointer, options(nostack));
        }
    }
}

#[inline(never)]
pub unsafe fn restore(config: Config, pointer: *const u8) {
    // SAFETY: aligned image was initialized internally or validated on import;
    // soft-float codegen guarantees no compiler-managed SIMD register values.
    unsafe {
        if config.format == Format::Xsave {
            asm!("xrstor64 [{}]", in(reg) pointer, in("eax") config.xcr0 as u32, in("edx") (config.xcr0 >> 32) as u32, options(nostack));
        } else {
            asm!("fxrstor64 [{}]", in(reg) pointer, options(nostack));
        }
    }
}

#[cfg(feature = "boot-self-test")]
pub fn is_enabled() -> bool {
    let value: u64;
    // SAFETY: read the local hardware gate without accessing FP registers.
    unsafe {
        asm!("mov {}, cr0", out(reg) value, options(nomem,nostack,preserves_flags));
    }
    value & TS == 0
}
