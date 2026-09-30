/*
 *
 *       src/arch/x86_64/cpuid.rs
 *       Bounded CPUID queries, feature decoding, deterministic caches and topology
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Bounded CPUID queries, feature decoding, deterministic caches and topology.

use crate::cpuid::{Architecture, Cache, CpuInfo, Feature, Topology};
use core::arch::x86_64::__cpuid_count;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

fn raw(leaf: u32, subleaf: u32) -> Registers {
    let value = __cpuid_count(leaf, subleaf);
    Registers {
        eax: value.eax,
        ebx: value.ebx,
        ecx: value.ecx,
        edx: value.edx,
    }
}

/// Reject absent leaf namespaces instead of accepting CPUID's last-leaf echo.
/// Subleafs with architectural termination rules are checked as well. Other
/// leaf-specific semantics are left to the caller of this raw escape hatch.
pub fn query(leaf: u32, subleaf: u32) -> Option<Registers> {
    let maximum = match leaf {
        0..=0x3fff_ffff => raw(0, 0).eax,
        0x4000_0000..=0x4fff_ffff => {
            if raw(1, 0).ecx & (1 << 31) == 0 {
                return None;
            }
            let max = raw(0x4000_0000, 0).eax;
            if !(0x4000_0000..=0x4fff_ffff).contains(&max) {
                return None;
            }
            max
        }
        0x8000_0000..=0xbfff_ffff => raw(0x8000_0000, 0).eax,
        _ => return None,
    };
    if leaf > maximum {
        return None;
    }
    match leaf {
        7 if subleaf > raw(7, 0).eax => return None,
        0xd => {
            if raw(1, 0).ecx & (1 << 26) == 0 || subleaf >= 64 {
                return None;
            }
            if subleaf >= 2 {
                let user = raw(0xd, 0);
                let supervisor = raw(0xd, 1);
                let supported = (u64::from(user.edx) << 32 | u64::from(user.eax))
                    | (u64::from(supervisor.edx) << 32 | u64::from(supervisor.ecx));
                if supported & (1 << subleaf) == 0 {
                    return None;
                }
            }
        }
        _ => {}
    }
    let value = raw(leaf, subleaf);
    if matches!(leaf, 4 | 0x8000_001d) && value.eax & 31 == 0 {
        return None;
    }
    if matches!(leaf, 0xb | 0x1f) && (value.ebx & 0xffff == 0 || value.ecx & 0xff00 == 0) {
        return None;
    }
    Some(value)
}

pub fn detect(index: usize) -> CpuInfo {
    let mut info = CpuInfo::empty(Architecture::X86_64);
    // Leaf 1 carries only an 8-bit legacy APIC ID. Keep Limine's full ID when
    // extended topology leaves are absent (e.g. older AMD implementations).
    info.hardware_id = crate::boot::cpu_hardware_id(index).expect("unknown CPU");
    let vendor = raw(0, 0);
    let mut name = [0u8; 12];
    name[..4].copy_from_slice(&vendor.ebx.to_le_bytes());
    name[4..8].copy_from_slice(&vendor.edx.to_le_bytes());
    name[8..].copy_from_slice(&vendor.ecx.to_le_bytes());
    info.vendor.set(&name);
    if query(0x8000_0004, 0).is_some() {
        let mut brand = [0u8; 48];
        for i in 0..3 {
            let r = raw(0x8000_0002 + i as u32, 0);
            for (j, register) in [r.eax, r.ebx, r.ecx, r.edx].iter().enumerate() {
                brand[i * 16 + j * 4..i * 16 + j * 4 + 4].copy_from_slice(&register.to_le_bytes());
            }
        }
        info.model.set(&brand);
    }
    if let Some(r) = query(1, 0) {
        info.signature = u64::from(r.eax);
        let base_family = (r.eax >> 8) & 15;
        info.family = (base_family
            + if base_family == 15 {
                (r.eax >> 20) & 255
            } else {
                0
            }) as u16;
        info.model_id = (((r.eax >> 4) & 15)
            | if base_family == 6 || base_family == 15 {
                (r.eax >> 12) & 0xf0
            } else {
                0
            }) as u16;
        info.stepping = (r.eax & 15) as u8;
        for (bit, feature) in [
            (0, Feature::Fpu),
            (4, Feature::Tsc),
            (9, Feature::Apic),
            (23, Feature::Mmx),
            (24, Feature::Fxsave),
            (25, Feature::Sse),
            (26, Feature::Sse2),
        ] {
            if r.edx & (1 << bit) != 0 {
                info.features.insert(feature);
            }
        }
        for (bit, feature) in [
            (0, Feature::Sse3),
            (1, Feature::Pclmul),
            (9, Feature::Ssse3),
            (12, Feature::Fma),
            (17, Feature::Pcid),
            (19, Feature::Sse41),
            (20, Feature::Sse42),
            (21, Feature::X2apic),
            (23, Feature::Popcnt),
            (24, Feature::TscDeadline),
            (25, Feature::Aes),
            (26, Feature::Xsave),
            (28, Feature::Avx),
            (29, Feature::F16c),
            (30, Feature::Rdrand),
            (31, Feature::Hypervisor),
        ] {
            if r.ecx & (1 << bit) != 0 {
                info.features.insert(feature);
            }
        }
    }
    if let Some(r) = query(7, 0) {
        for (bit, feature) in [
            (3, Feature::Bmi1),
            (5, Feature::Avx2),
            (7, Feature::Smep),
            (8, Feature::Bmi2),
            (10, Feature::Invpcid),
            (16, Feature::Avx512f),
            (17, Feature::Avx512dq),
            (18, Feature::Rdseed),
            (20, Feature::Smap),
            (28, Feature::Avx512cd),
            (29, Feature::Sha),
            (30, Feature::Avx512bw),
            (31, Feature::Avx512vl),
        ] {
            if r.ebx & (1 << bit) != 0 {
                info.features.insert(feature);
            }
        }
        if r.ecx & (1 << 16) != 0 {
            info.features.insert(Feature::La57);
        }
    }
    if let Some(r) = query(0x8000_0001, 0) {
        for (bit, feature) in [
            (20, Feature::Nx),
            (26, Feature::Page1g),
            (27, Feature::Rdtscp),
        ] {
            if r.edx & (1 << bit) != 0 {
                info.features.insert(feature);
            }
        }
        if r.ecx & (1 << 6) != 0 {
            info.features.insert(Feature::Sse4a);
        }
    }
    if query(0x8000_0007, 0).is_some_and(|r| r.edx & (1 << 8) != 0) {
        info.features.insert(Feature::InvariantTsc);
    }
    if query(0xd, 1).is_some_and(|r| r.eax & 1 != 0) {
        info.features.insert(Feature::Xsaveopt);
    }
    if info.features.contains(Feature::Sse) {
        info.features.insert(Feature::Simd);
    }
    if info.features.contains(Feature::Fpu) {
        info.features.insert(Feature::DoublePrecision);
    }
    let widths = query(0x8000_0008, 0);
    info.physical_bits = Some(widths.map_or(36, |r| (r.eax & 255) as u8).clamp(32, 52));
    info.virtual_bits = Some(
        widths
            .map_or(48, |r| ((r.eax >> 8) & 255) as u8)
            .clamp(48, 57),
    );
    let cache_leaf = if query(0x8000_001d, 0).is_some() {
        0x8000_001d
    } else {
        4
    };
    for i in 0..info.caches.len() {
        let Some(r) = query(cache_leaf, i as u32) else {
            break;
        };
        let line = (r.ebx & 0xfff) + 1;
        let partitions = ((r.ebx >> 12) & 0x3ff) + 1;
        let ways = (r.ebx >> 22) + 1;
        let bytes = u64::from(line)
            .checked_mul(u64::from(partitions))
            .and_then(|n| n.checked_mul(u64::from(ways)))
            .and_then(|n| n.checked_mul(u64::from(r.ecx) + 1));
        if let Some(bytes) = bytes {
            info.caches[info.cache_count] = Some(Cache {
                level: ((r.eax >> 5) & 7) as u8,
                kind: (r.eax & 31) as u8,
                bytes,
                line_bytes: line,
                sharing: ((r.eax >> 14) & 0xfff) + 1,
            });
            info.cache_count += 1;
        }
    }
    let topology_leaf = if query(0x1f, 0).is_some() { 0x1f } else { 0xb };
    let (mut smt_shift, mut package_shift, mut logical, mut id) =
        (0, 0, 0, info.hardware_id as u32);
    for i in 0..32 {
        let Some(r) = query(topology_leaf, i) else {
            break;
        };
        let shift = r.eax & 31;
        if (r.ecx >> 8) & 255 == 1 {
            smt_shift = shift;
        }
        package_shift = package_shift.max(shift);
        logical = r.ebx & 0xffff;
        id = r.edx;
    }
    if logical != 0 && smt_shift <= package_shift {
        info.hardware_id = u64::from(id);
        info.topology = Some(Topology {
            thread_id: id & ((1u32 << smt_shift) - 1),
            core_id: (id & ((1u32 << package_shift) - 1)) >> smt_shift,
            package_id: id >> package_shift,
            logical_per_package: logical,
        });
    }
    if let Some(r) = query(0x15, 0)
        && r.eax != 0
        && r.ebx != 0
        && r.ecx != 0
    {
        info.counter_hz = Some(u64::from(r.ecx) * u64::from(r.ebx) / u64::from(r.eax));
    }
    info
}
