/*
 *
 *       src/pci/ecam.rs
 *       Firmware-advertised ECAM windows and per-bus ownership
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use alloc::boxed::Box;

use crate::{boot, hardware::acpi::PciSegment, mm::vmm, sync::SpinLock};
use limine::memmap::{
    MEMMAP_ACPI_NVS, MEMMAP_ACPI_RECLAIMABLE, MEMMAP_BOOTLOADER_RECLAIMABLE,
    MEMMAP_EXECUTABLE_AND_MODULES, MEMMAP_FRAMEBUFFER, MEMMAP_USABLE,
};

use super::{Address, ConfigSpace, PciError};

const FUNCTION_SIZE: u64 = 4096;
const BUS_SIZE: u64 = 1 << 20;

pub(super) struct BusState {
    functions: Option<Box<[Option<usize>; 256]>>,
}

/// One firmware-advertised ECAM allocation; each bus has an independent lock.
pub(super) struct EcamWindow {
    segment: PciSegment,
    buses: [SpinLock<BusState>; 256],
}

impl EcamWindow {
    pub fn new(segment: PciSegment) -> Result<Self, PciError> {
        if segment.base == 0
            || !segment.base.is_multiple_of(BUS_SIZE)
            || segment.start_bus > segment.end_bus
        {
            return Err(PciError::MalformedWindow);
        }
        let bus_count = u64::from(segment.end_bus) - u64::from(segment.start_bus) + 1;
        let end = segment
            .base
            .checked_add(bus_count * BUS_SIZE)
            .ok_or(PciError::MalformedWindow)?;
        // Firmware config windows must never alias allocator-owned RAM or an
        // already used framebuffer. Missing map entries are valid for MMIO.
        for entry in boot::memory_map() {
            let occupied = matches!(
                entry.type_,
                MEMMAP_USABLE
                    | MEMMAP_ACPI_RECLAIMABLE
                    | MEMMAP_ACPI_NVS
                    | MEMMAP_BOOTLOADER_RECLAIMABLE
                    | MEMMAP_EXECUTABLE_AND_MODULES
                    | MEMMAP_FRAMEBUFFER
            );
            if occupied
                && entry.base < end
                && entry
                    .base
                    .checked_add(entry.length)
                    .is_some_and(|limit| segment.base < limit)
            {
                return Err(PciError::WindowOverlapsMemory);
            }
        }
        Ok(Self {
            segment,
            buses: [const { SpinLock::new(BusState { functions: None }) }; 256],
        })
    }

    pub fn segment(&self) -> PciSegment {
        self.segment
    }

    pub fn contains(&self, address: Address) -> bool {
        address.segment() == self.segment.group
            && (self.segment.start_bus..=self.segment.end_bus).contains(&address.bus())
    }

    pub fn open(&self, address: Address) -> Result<ConfigSpace<'_>, PciError> {
        if !self.contains(address) {
            return Err(PciError::NoWindow);
        }
        let mut bus = self.buses[usize::from(address.bus())].lock();
        let functions = bus.functions.get_or_insert_with(|| Box::new([None; 256]));
        let index = address.function_index();
        let base = match functions[index] {
            Some(base) => base,
            None => {
                let bus_offset = u64::from(address.bus() - self.segment.start_bus) * BUS_SIZE;
                let physical = self.segment.base + bus_offset + index as u64 * FUNCTION_SIZE;
                let base = vmm::map_device_page(physical).map_err(PciError::Mapping)? as usize;
                functions[index] = Some(base);
                base
            }
        };
        Ok(ConfigSpace::new(bus, base))
    }
}
