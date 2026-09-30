/*
 *
 *       src/fpu/self_test.rs
 *       Boot-time hardware tests for register isolation, nesting and input validation
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Boot-time hardware tests for register isolation, nesting and input validation.

use super::{
    Config, Error, Format, State, config, restore_current, save_current, switch, with_kernel,
};
use crate::{
    arch,
    cpuid::{self, Feature},
};
use alloc::vec::Vec;

fn register_ranges(config: Config) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    match config.format {
        Format::Fxsave | Format::Xsave => {
            ranges.push((0, 5)); // x87 control, status and abridged tag.
            ranges.push((24, 28)); // MXCSR; mask is hardware metadata.
            for register in 0..8 {
                ranges.push((32 + register * 16, 42 + register * 16));
            }
            ranges.push((160, 416)); // XMM0..15, including callee-saved registers.
            #[cfg(target_arch = "x86_64")]
            for component in 2..8 {
                if config.xcr0 & (1 << component) != 0 {
                    let r = cpuid::query_x86(0xd, component).expect("enabled xstate missing");
                    ranges.push((r.ebx as usize, (r.ebx + r.eax) as usize));
                }
            }
        }
        Format::FpSimd => ranges.push((0, 520)),
        Format::RiscvD => ranges.push((0, 260)),
        Format::RiscvF => {
            for register in 0..32 {
                ranges.push((register * 8, register * 8 + 4));
            }
            ranges.push((256, 260));
        }
        Format::Unavailable => {}
    }
    ranges
}

pub(crate) fn pattern(config: Config, salt: u8) -> State {
    let mut state = State::new().expect("FPU pattern allocation failed");
    let mut bytes = state.bytes().to_vec();
    for (start, end) in register_ranges(config) {
        for (i, value) in bytes[start..end].iter_mut().enumerate() {
            *value = salt.wrapping_add(i as u8).wrapping_mul(17);
        }
    }
    match config.format {
        Format::Fxsave | Format::Xsave => {
            bytes[..2].copy_from_slice(&(0x037fu16 | (u16::from(salt & 1) << 10)).to_le_bytes());
            bytes[2..4].fill(0);
            bytes[4] = 0xff;
            bytes[24..28].copy_from_slice(&(0x1f80u32 | (u32::from(salt & 1) << 13)).to_le_bytes());
            for register in 0..8 {
                let offset = 32 + register * 16;
                bytes[offset..offset + 8].copy_from_slice(
                    &(0x8000_0000_0000_0000u64 + u64::from(salt) + register as u64).to_le_bytes(),
                );
                bytes[offset + 8..offset + 10].copy_from_slice(&0x3fffu16.to_le_bytes());
            }
        }
        Format::FpSimd => {
            bytes[512..516].copy_from_slice(&(u32::from(salt & 1) << 22).to_le_bytes());
            bytes[516..520].copy_from_slice(&u32::from(salt & 1).to_le_bytes());
        }
        Format::RiscvF | Format::RiscvD => {
            bytes[256..260].copy_from_slice(&(u32::from(salt & 1) << 5).to_le_bytes())
        }
        Format::Unavailable => unreachable!(),
    }
    state
        .import(&bytes)
        .expect("valid register pattern rejected");
    state
}

pub(crate) fn assert_registers(config: Config, expected: &State, observed: &State) {
    for (start, end) in register_ranges(config) {
        assert_eq!(
            &expected.bytes()[start..end],
            &observed.bytes()[start..end],
            "register mismatch in range {start}..{end}"
        );
    }
}

pub(crate) fn run() {
    let cpu = cpuid::info().expect("CPU discovery required");
    let configured = config().expect("FPU initialization required");
    assert!(cpuid::compatible(&cpu, cpu.features));
    if cpu.physical_bits.is_some() {
        let mut narrow = cpu;
        narrow.physical_bits = Some(1);
        assert!(!cpuid::compatible(&narrow, cpu.features));
        narrow.physical_bits = None;
        assert!(!cpuid::compatible(&narrow, cpu.features));
    }
    assert!(!configured.supports(Feature::Sve, &cpu));
    assert!(!configured.supports(Feature::RiscvV, &cpu));
    #[cfg(target_arch = "x86_64")]
    {
        assert!(cpuid::query_x86(0x7fff_ffff, 0).is_none());
        assert!(cpuid::query_x86(0xffff_ffff, 0).is_none());
        assert!(cpuid::query_x86(0xd, 64).is_none());
        assert_eq!(
            configured.supports(Feature::Avx, &cpu),
            cpu.features.contains(Feature::Avx) && configured.xcr0 & 6 == 6
        );
    }
    #[cfg(target_arch = "riscv64")]
    {
        let flags = arch::cpuid::parse_isa("rv64i2p1_m2p0_zfa_zve64d");
        assert!(flags.contains(Feature::RiscvI));
        assert!(flags.contains(Feature::RiscvM));
        assert!(!arch::cpuid::parse_isa("rv64zve64d").contains(Feature::RiscvD));
        assert!(!flags.contains(Feature::RiscvF));
        assert!(!flags.contains(Feature::RiscvD));
    }
    assert!(!arch::fpu::is_enabled());
    if configured.format == Format::Unavailable {
        assert!(matches!(State::new(), Err(Error::Unsupported)));
        crate::kinfo!("CPU/FPU self-test passed (no managed FP state)");
        return;
    }
    let first = pattern(configured, 1);
    let second = pattern(configured, 2);
    let mut saved = State::new().expect("FPU snapshot allocation failed");
    let mut scratch = State::new().expect("FPU scratch allocation failed");
    let mut nested = State::new().expect("nested FPU scratch allocation failed");
    let clean = State::new().expect("clean FPU state failed");
    // SAFETY: boot runs on BSP with IRQs masked; no task or NMI uses FP.
    unsafe {
        restore_current(&first).unwrap();
        save_current(&mut saved).unwrap();
        assert_registers(configured, &first, &saved);
        switch(&mut saved, &second).unwrap();
        let clone = saved.try_clone().unwrap();
        assert_registers(configured, &first, &clone);
        save_current(&mut saved).unwrap();
        assert_registers(configured, &second, &saved);
        with_kernel(&mut scratch, || {
            assert!(arch::fpu::is_enabled());
            save_current(&mut saved).unwrap();
            assert_registers(configured, &clean, &saved);
            restore_current(&first).unwrap();
            // Test nesting when the outer scope left the FP gate closed too.
            with_kernel(&mut nested, || {
                save_current(&mut saved).unwrap();
                assert_registers(configured, &clean, &saved);
                restore_current(&second).unwrap();
            })
            .unwrap();
            save_current(&mut saved).unwrap();
            assert_registers(configured, &first, &saved);
        })
        .unwrap();
        assert!(!arch::fpu::is_enabled());
        save_current(&mut saved).unwrap();
        assert_registers(configured, &second, &saved);
        restore_current(&clean).unwrap();
    }
    let original = saved.bytes().to_vec();
    let mut invalid = original.clone();
    let control = match configured.format {
        Format::Fxsave | Format::Xsave => 27,
        Format::FpSimd => 515,
        _ => 259,
    };
    invalid[control] |= 0x80;
    assert_eq!(saved.import(&invalid), Err(Error::InvalidState));
    assert_eq!(saved.bytes(), &original);
    assert_eq!(
        saved.import(&original[..original.len() - 1]),
        Err(Error::InvalidState)
    );
    if configured.format == Format::Xsave {
        let mut invalid = original.clone();
        invalid[520] = 0x80; // Compacted format must be rejected.
        assert_eq!(saved.import(&invalid), Err(Error::InvalidState));
        invalid = original.clone();
        invalid[519] = 0x80; // Unmanaged xstate component.
        assert_eq!(saved.import(&invalid), Err(Error::InvalidState));
        invalid = original.clone();
        invalid[575] = 1; // Reserved header metadata.
        assert_eq!(saved.import(&invalid), Err(Error::InvalidState));
    }
    if matches!(configured.format, Format::RiscvF | Format::RiscvD) {
        for rounding in 5u32..=7 {
            let mut invalid = original.clone();
            invalid[256..260].copy_from_slice(&(rounding << 5u32).to_le_bytes());
            assert_eq!(saved.import(&invalid), Err(Error::InvalidState));
        }
        assert_eq!(saved.bytes(), &original);
    }
    saved.reset().unwrap();
    assert_eq!(saved.bytes(), clean.bytes());
    crate::kinfo!("CPU/FPU self-test passed");
}
