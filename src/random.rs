/*
 *       src/random.rs
 *       Hardware-backed random bytes without clock-based entropy claims
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
//! Unsupported or temporarily exhausted sources return WouldBlock. Time and
//! public boot data are never substituted for entropy.
use crate::vfs::VfsError;
pub fn supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        return crate::cpuid::has(crate::cpuid::Feature::Rdrand);
    }
    #[cfg(target_arch = "aarch64")]
    {
        let features: u64;
        // SAFETY: EL1 can read the architectural instruction feature register.
        unsafe {
            core::arch::asm!("mrs {}, ID_AA64ISAR0_EL1", out(reg) features, options(nomem, nostack));
        }
        return (features >> 60 & 15) != 0;
    }
    #[cfg(target_arch = "riscv64")]
    {
        false
    }
}
pub fn fill(dst: &mut [u8]) -> Result<(), VfsError> {
    for chunk in dst.chunks_mut(8) {
        let value = word().ok_or(VfsError::WouldBlock)?.to_le_bytes();
        chunk.copy_from_slice(&value[..chunk.len()]);
    }
    Ok(())
}
fn word() -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        if !crate::cpuid::has(crate::cpuid::Feature::Rdrand) {
            return None;
        }
        for _ in 0..64 {
            let value: u64;
            let ready: u8;
            // SAFETY: CPUID reports RDRAND; carry is checked for source exhaustion.
            unsafe {
                core::arch::asm!("rdrand {v}","setc {r}",v=out(reg)value,r=out(reg_byte)ready,options(nostack));
            }
            if ready != 0 {
                return Some(value);
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        let features: u64;
        // SAFETY: EL1 can read the architectural instruction feature register.
        unsafe {
            core::arch::asm!("mrs {}, ID_AA64ISAR0_EL1",out(reg)features,options(nomem,nostack));
        }
        if features >> 60 & 15 == 0 {
            return None;
        }
        for _ in 0..64 {
            let value: u64;
            let ready: u32;
            unsafe {
                core::arch::asm!("mrs {v}, S3_3_C2_C4_0","cset {r:w}, ne",v=out(reg)value,r=out(reg)ready,options(nostack));
            }
            if ready != 0 {
                return Some(value);
            }
        }
    }
    None
}
