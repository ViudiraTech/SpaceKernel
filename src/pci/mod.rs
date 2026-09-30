/*
 *
 *       src/pci/mod.rs
 *       PCIe configuration and firmware-topology discovery, independent of CPU ISA
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! PCIe configuration and firmware-topology discovery, independent of CPU ISA.

mod address;
mod capability;
mod config;
mod device;
mod ecam;
mod enumerate;

pub use address::Address;
pub use capability::{
    Capability, ExtendedCapability, extended as extended_capabilities,
    standard as standard_capabilities,
};
pub use config::ConfigSpace;
pub use device::{Bar, BridgeBuses, DeviceInfo};

use alloc::{boxed::Box, vec::Vec};
use core::{
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    boot,
    hardware::{
        acpi::{Acpi, AcpiError, Mcfg, PciSegment},
        fdt::Fdt,
    },
    mm::vmm::MapError,
};
use ecam::EcamWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciError {
    NoFirmware,
    NoWindow,
    InvalidAddress,
    InvalidOffset,
    MalformedWindow,
    WindowOverlapsMemory,
    MalformedCapability,
    AlreadyInitialized,
    Acpi(AcpiError),
    Mapping(MapError),
}

impl From<AcpiError> for PciError {
    fn from(error: AcpiError) -> Self {
        Self::Acpi(error)
    }
}

struct PciCore {
    windows: Vec<EcamWindow>,
    devices: Box<[DeviceInfo]>,
}

static CORE: AtomicPtr<PciCore> = AtomicPtr::new(ptr::null_mut());

impl PciCore {
    fn from_mcfg(mcfg: Mcfg) -> Result<Self, PciError> {
        let mut windows: Vec<EcamWindow> = Vec::new();
        for segment in mcfg.segments() {
            let segment = mcfg_bus_window(segment)?;
            for existing in &windows {
                let other = existing.segment();
                if segment.group == other.group
                    && segment.start_bus <= other.end_bus
                    && other.start_bus <= segment.end_bus
                {
                    return Err(PciError::MalformedWindow);
                }
                let end = ecam_end(segment).ok_or(PciError::MalformedWindow)?;
                let other_end = ecam_end(other).ok_or(PciError::MalformedWindow)?;
                if segment.base < other_end && other.base < end {
                    return Err(PciError::MalformedWindow);
                }
            }
            windows.push(EcamWindow::new(segment)?);
        }
        if windows.is_empty() {
            return Err(PciError::NoWindow);
        }
        Ok(Self {
            windows,
            devices: Vec::new().into_boxed_slice(),
        })
    }

    fn from_fdt(fdt: Fdt<'_>) -> Result<Self, PciError> {
        let node = fdt
            .find_compatible("pci-host-ecam-generic")
            .ok_or(PciError::NoWindow)?;
        let reg = node
            .reg()
            .and_then(|mut r| r.next())
            .ok_or(PciError::MalformedWindow)?;
        let (start_bus, end_bus) = if let Some(bus_range) = node.property("bus-range") {
            if bus_range.value.len() == 8 {
                let start = u8::try_from(u32::from_be_bytes(
                    bus_range.value[0..4].try_into().unwrap(),
                ))
                .map_err(|_| PciError::MalformedWindow)?;
                let end = u8::try_from(u32::from_be_bytes(
                    bus_range.value[4..8].try_into().unwrap(),
                ))
                .map_err(|_| PciError::MalformedWindow)?;
                (start, end)
            } else {
                return Err(PciError::MalformedWindow);
            }
        } else {
            (0, 255)
        };
        let buses = u64::from(
            end_bus
                .checked_sub(start_bus)
                .ok_or(PciError::MalformedWindow)?,
        ) + 1;
        if reg.size < buses * (1 << 20) {
            return Err(PciError::MalformedWindow);
        }
        let segment = PciSegment {
            base: reg.address,
            group: 0,
            start_bus,
            end_bus,
        };
        let window = EcamWindow::new(segment)?;
        Ok(Self {
            windows: alloc::vec![window],
            devices: Vec::new().into_boxed_slice(),
        })
    }

    fn window(&self, address: Address) -> Option<&EcamWindow> {
        self.windows.iter().find(|window| window.contains(address))
    }

    fn config(&self, address: Address) -> Result<ConfigSpace<'_>, PciError> {
        self.window(address)
            .ok_or(PciError::NoWindow)?
            .open(address)
    }
}

/// MCFG bases always describe bus zero. Our common ECAM window (also used for
/// DT reg ranges) starts at the first advertised bus, so normalize exactly once.
fn mcfg_bus_window(mut segment: PciSegment) -> Result<PciSegment, PciError> {
    segment.base = segment
        .base
        .checked_add(u64::from(segment.start_bus) << 20)
        .ok_or(PciError::MalformedWindow)?;
    Ok(segment)
}

#[cfg(feature = "boot-self-test")]
pub(crate) fn self_test() {
    let descriptor = PciSegment {
        base: 0x8000_0000,
        group: 1,
        start_bus: 64,
        end_bus: 127,
    };
    let window = mcfg_bus_window(descriptor).unwrap();
    assert_eq!(window.base, 0x8400_0000);
    assert_eq!(ecam_end(window), Some(0x8800_0000));
    assert_eq!(
        mcfg_bus_window(PciSegment {
            base: !((1u64 << 20) - 1),
            ..descriptor
        }),
        Err(PciError::MalformedWindow)
    );
}

fn ecam_end(segment: PciSegment) -> Option<u64> {
    let buses = u64::from(segment.end_bus.checked_sub(segment.start_bus)?) + 1;
    segment.base.checked_add(buses.checked_mul(1 << 20)?)
}

fn core() -> Option<&'static PciCore> {
    let pointer = CORE.load(Ordering::Acquire);
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the boxed core is published with Release and never freed.
        Some(unsafe { &*pointer })
    }
}

fn discover_core() -> Result<PciCore, PciError> {
    if let Some(rsdp) = boot::rsdp_address()
        && let Ok(acpi) = Acpi::from_rsdp(rsdp)
        && let Ok(Some(mcfg)) = acpi.mcfg()
        && let Ok(core) = PciCore::from_mcfg(mcfg)
    {
        return Ok(core);
    }
    if let Some(dtb) = boot::dtb_address()
        && let Ok(fdt) = Fdt::from_boot_address(dtb)
        && let Ok(core) = PciCore::from_fdt(fdt)
    {
        return Ok(core);
    }
    Err(PciError::NoFirmware)
}

/// Discover ECAM windows and firmware-configured buses before drivers start.
pub fn init() -> Result<usize, PciError> {
    if core().is_some() {
        return Err(PciError::AlreadyInitialized);
    }
    let mut found = discover_core()?;
    let discovered = enumerate::scan(&found)?;
    let count = discovered.len();
    found.devices = discovered.into_boxed_slice();
    let pointer = Box::into_raw(Box::new(found));
    if CORE
        .compare_exchange(
            ptr::null_mut(),
            pointer,
            Ordering::Release,
            Ordering::Acquire,
        )
        .is_err()
    {
        // SAFETY: no other reader received this losing, unpublished allocation.
        unsafe { drop(Box::from_raw(pointer)) };
        return Err(PciError::AlreadyInitialized);
    }
    Ok(count)
}

/// Opening the function holds a per-bus IRQ-safe lock until the guard drops.
pub fn config_for(address: Address) -> Result<ConfigSpace<'static>, PciError> {
    core().ok_or(PciError::NoFirmware)?.config(address)
}

pub fn devices() -> &'static [DeviceInfo] {
    core().map_or(&[], |core| &core.devices)
}

pub fn segments() -> Vec<PciSegment> {
    core().map_or_else(Vec::new, |core| {
        core.windows.iter().map(EcamWindow::segment).collect()
    })
}

pub fn device(address: Address) -> Option<DeviceInfo> {
    core()?
        .devices
        .iter()
        .find(|device| device.address == address)
        .copied()
}
