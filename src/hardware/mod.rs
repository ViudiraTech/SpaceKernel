/*
 *
 *       src/hardware/mod.rs
 *       Firmware discovery and hardware reporting
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

pub mod acpi;
pub mod fdt;

use crate::boot;

pub fn report() {
    if let Some(address) = boot::rsdp_address() {
        match acpi::Acpi::from_rsdp(address) {
            Ok(acpi) => {
                crate::kinfo!("ACPI: RSDP revision {}", acpi.revision());
                match acpi.madt() {
                    Ok(Some(madt)) => crate::kinfo!(
                        "ACPI MADT: {} enabled processors, controller {:#x}",
                        madt.enabled_processor_count(),
                        madt.local_controller_address()
                    ),
                    Ok(None) => crate::kwarn!("ACPI MADT missing"),
                    Err(error) => crate::kwarn!("ACPI MADT rejected: {error:?}"),
                }
                match acpi.fadt() {
                    Ok(Some(fadt)) => crate::kinfo!(
                        "ACPI FADT: SCI {} hardware_reduced={} DSDT={}",
                        fadt.sci_interrupt(),
                        fadt.hardware_reduced(),
                        fadt.dsdt().ok().flatten().is_some()
                    ),
                    Ok(None) => crate::kwarn!("ACPI FADT missing"),
                    Err(error) => crate::kwarn!("ACPI FADT rejected: {error:?}"),
                }
                if let Ok(Some(mcfg)) = acpi.mcfg() {
                    crate::kinfo!("ACPI MCFG: {} PCI segments", mcfg.segments().count());
                }
                if let Ok(Some(srat)) = acpi.srat() {
                    crate::kinfo!(
                        "ACPI SRAT: {} memory affinities",
                        srat.memory_affinities().count()
                    );
                }
            }
            Err(error) => crate::kwarn!("ACPI rejected: {error:?}"),
        }
    }
    if let Some(address) = boot::dtb_address() {
        match fdt::Fdt::from_boot_address(address) {
            Ok(tree) => {
                let header = tree.header();
                crate::kinfo!("DTB: {} bytes, v{}", tree.len(), header.version);
                if let Ok(root) = tree.root() {
                    if let Some(model) = root.property("model").and_then(|p| p.as_str()) {
                        crate::kinfo!("DTB model: {model}");
                    }
                    if let Some(compat) = root.property("compatible").and_then(|p| p.as_str()) {
                        crate::kinfo!("DTB compatible: {compat}");
                    }
                }
                let rsv_count = tree.memory_reservations().count();
                if rsv_count > 0 {
                    crate::kinfo!("DTB: {rsv_count} reserved memory regions");
                }
                if let Some(chosen) = tree.chosen() {
                    if let Some(stdout) = chosen.stdout_path() {
                        crate::kinfo!("DTB chosen stdout: {stdout}");
                    }
                    if let Some(bootargs) = chosen.bootargs()
                        && !bootargs.is_empty()
                    {
                        crate::kinfo!("DTB chosen bootargs: {bootargs}");
                    }
                }
            }
            Err(error) => crate::kwarn!("DTB rejected: {error:?}"),
        }
    }
}
