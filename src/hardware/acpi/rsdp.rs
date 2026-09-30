/*
 *
 *       src/hardware/acpi/rsdp.rs
 *       ACPI root descriptor validation
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use crate::mm::pmm;

use super::{
    AcpiError,
    sdt::{le_u32, le_u64, mapped_acpi, valid_checksum},
};

pub(super) struct Rsdp {
    pub revision: u8,
    pub root_physical: u64,
    pub entry_size: usize,
}

impl Rsdp {
    pub fn from_boot_address(address: usize) -> Result<Self, AcpiError> {
        let physical = (address as u64)
            .checked_sub(pmm::hhdm())
            .ok_or(AcpiError::Unmapped)?;
        if address == 0 {
            return Err(AcpiError::Null);
        }
        if !mapped_acpi(physical, 20) {
            return Err(AcpiError::Unmapped);
        }
        // SAFETY: Limine supplies an HHDM pointer and the memory map covers
        // the minimum RSDP header, which is never freed by the PMM.
        let first = unsafe { core::slice::from_raw_parts(address as *const u8, 20) };
        if &first[..8] != b"RSD PTR " {
            return Err(AcpiError::Signature);
        }
        if !valid_checksum(first) {
            return Err(AcpiError::Checksum);
        }
        let revision = first[15];
        let (root_physical, entry_size) = if revision >= 2 {
            if !mapped_acpi(physical, 36) {
                return Err(AcpiError::Unmapped);
            }
            // SAFETY: the extended fixed header was verified above.
            let extended = unsafe { core::slice::from_raw_parts(address as *const u8, 36) };
            let length = le_u32(&extended[20..24]) as usize;
            if !(36..=4096).contains(&length) {
                return Err(AcpiError::Length);
            }
            if !mapped_acpi(physical, length) {
                return Err(AcpiError::Unmapped);
            }
            // SAFETY: the complete bounded RSDP lies in reserved mapped RAM.
            let all = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
            if !valid_checksum(all) {
                return Err(AcpiError::Checksum);
            }
            (le_u64(&extended[24..32]), 8)
        } else {
            (u64::from(le_u32(&first[16..20])), 4)
        };
        if root_physical == 0 {
            return Err(AcpiError::Null);
        }
        Ok(Self {
            revision,
            root_physical,
            entry_size,
        })
    }
}
