use crate::{boot, mm::pmm};
use limine::memmap::{
    MEMMAP_ACPI_NVS, MEMMAP_ACPI_RECLAIMABLE, MEMMAP_BOOTLOADER_RECLAIMABLE, MEMMAP_MAPPED_RESERVED,
};

use super::AcpiError;

pub(super) const HEADER_SIZE: usize = 36;
const MAX_TABLE_SIZE: usize = 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Table {
    bytes: &'static [u8],
}

impl Table {
    pub fn signature(&self) -> &[u8; 4] {
        self.bytes[..4].try_into().unwrap()
    }
    pub fn bytes(&self) -> &'static [u8] {
        self.bytes
    }
}

pub(super) fn read_table(physical: u64) -> Result<Table, AcpiError> {
    if !mapped_acpi(physical, HEADER_SIZE) {
        return Err(AcpiError::Unmapped);
    }
    let virtual_address = pmm::phys_to_virt(physical);
    // SAFETY: the memory map covers the fixed ACPI header through the HHDM.
    let header = unsafe { core::slice::from_raw_parts(virtual_address as *const u8, HEADER_SIZE) };
    let length = le_u32(&header[4..8]) as usize;
    if !(HEADER_SIZE..=MAX_TABLE_SIZE).contains(&length) {
        return Err(AcpiError::Length);
    }
    if !mapped_acpi(physical, length) {
        return Err(AcpiError::Unmapped);
    }
    // SAFETY: the complete bounded table lies in mapped firmware RAM that
    // remains reserved by the PMM for the lifetime of this kernel.
    let bytes = unsafe { core::slice::from_raw_parts(virtual_address as *const u8, length) };
    if !valid_checksum(bytes) {
        return Err(AcpiError::Checksum);
    }
    Ok(Table { bytes })
}

pub(super) fn mapped_acpi(physical: u64, length: usize) -> bool {
    let Some(end) = physical.checked_add(length as u64) else {
        return false;
    };
    boot::memory_map().iter().any(|entry| {
        matches!(
            entry.type_,
            MEMMAP_ACPI_RECLAIMABLE
                | MEMMAP_ACPI_NVS
                | MEMMAP_BOOTLOADER_RECLAIMABLE
                | MEMMAP_MAPPED_RESERVED
        ) && entry.base <= physical
            && entry
                .base
                .checked_add(entry.length)
                .is_some_and(|region_end| end <= region_end)
    })
}

pub(super) fn valid_checksum(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .fold(0u8, |sum, value| sum.wrapping_add(*value))
        == 0
}
pub(super) fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().unwrap())
}
pub(super) fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}
pub(super) fn le_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}
