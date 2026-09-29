//! ACPI table discovery and validated hardware-description views.

mod fadt;
mod madt;
mod mcfg;
mod rsdp;
mod sdt;
mod srat;

pub use fadt::Fadt;
pub use madt::{Madt, MadtEntry};
pub use mcfg::{Mcfg, PciSegment};
pub use sdt::Table;
pub use srat::Srat;

use rsdp::Rsdp;
use sdt::{HEADER_SIZE, read_table};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiError {
    Null,
    Unmapped,
    Signature,
    Checksum,
    Length,
    Malformed,
}

pub struct Acpi {
    revision: u8,
    root: Table,
    entry_size: usize,
}

impl Acpi {
    pub fn from_rsdp(address: usize) -> Result<Self, AcpiError> {
        let rsdp = Rsdp::from_boot_address(address)?;
        let root = read_table(rsdp.root_physical)?;
        if root.signature()
            != if rsdp.entry_size == 8 {
                b"XSDT"
            } else {
                b"RSDT"
            }
        {
            return Err(AcpiError::Signature);
        }
        if (root.bytes().len() - HEADER_SIZE) % rsdp.entry_size != 0 {
            return Err(AcpiError::Length);
        }
        Ok(Self {
            revision: rsdp.revision,
            root,
            entry_size: rsdp.entry_size,
        })
    }

    pub fn revision(&self) -> u8 {
        self.revision
    }

    pub fn tables(&self) -> impl Iterator<Item = Result<Table, AcpiError>> + '_ {
        self.root.bytes()[HEADER_SIZE..]
            .chunks_exact(self.entry_size)
            .map(|bytes| {
                let physical = if self.entry_size == 8 {
                    sdt::le_u64(bytes)
                } else {
                    u64::from(sdt::le_u32(bytes))
                };
                read_table(physical)
            })
    }

    pub fn table(&self, signature: &[u8; 4]) -> Result<Option<Table>, AcpiError> {
        for item in self.tables() {
            let table = item?;
            if table.signature() == signature {
                return Ok(Some(table));
            }
        }
        Ok(None)
    }

    pub fn madt(&self) -> Result<Option<Madt>, AcpiError> {
        self.table(b"APIC")?.map(Madt::new).transpose()
    }

    pub fn fadt(&self) -> Result<Option<Fadt>, AcpiError> {
        self.table(b"FACP")?.map(Fadt::new).transpose()
    }

    pub fn mcfg(&self) -> Result<Option<Mcfg>, AcpiError> {
        self.table(b"MCFG")?.map(Mcfg::new).transpose()
    }

    pub fn srat(&self) -> Result<Option<Srat>, AcpiError> {
        self.table(b"SRAT")?.map(Srat::new).transpose()
    }
}
