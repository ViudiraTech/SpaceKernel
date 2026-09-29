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
                return;
            }
            Err(error) => crate::kwarn!("ACPI rejected: {error:?}"),
        }
    }
    if let Some(address) = boot::dtb_address() {
        match fdt::Fdt::from_boot_address(address) {
            Ok(tree) => crate::kinfo!("DTB: {} bytes", tree.len()),
            Err(error) => crate::kwarn!("DTB rejected: {error:?}"),
        }
    }
}
