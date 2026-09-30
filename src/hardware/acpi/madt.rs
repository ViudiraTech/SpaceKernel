/*
 *
 *       src/hardware/acpi/madt.rs
 *       Multiple APIC description and interrupt-controller entries
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use super::{
    AcpiError,
    sdt::{Table, le_u16, le_u32, le_u64},
};

#[derive(Clone, Copy)]
pub struct Madt {
    table: Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadtEntry {
    LocalApic {
        uid: u8,
        apic_id: u8,
        enabled: bool,
    },
    X2Apic {
        uid: u32,
        apic_id: u32,
        enabled: bool,
    },
    IoApic {
        id: u8,
        address: u32,
        gsi_base: u32,
    },
    InterruptOverride {
        bus: u8,
        source: u8,
        gsi: u32,
        flags: u16,
    },
    NmiSource {
        gsi: u32,
        flags: u16,
    },
    LocalApicNmi {
        uid: u8,
        lint: u8,
        flags: u16,
    },
    LocalApicAddressOverride {
        address: u64,
    },
    X2ApicNmi {
        uid: u32,
        lint: u8,
        flags: u16,
    },
    Gicc {
        uid: u32,
        mpidr: u64,
        address: u64,
        enabled: bool,
    },
    Gicd {
        id: u32,
        address: u64,
        version: u8,
    },
    Gicr {
        address: u64,
        length: u32,
    },
    Rintc {
        uid: u32,
        hart_id: u64,
        enabled: bool,
    },
    Plic {
        id: u8,
        address: u64,
        size: u32,
        max_priority: u16,
    },
    Other {
        kind: u8,
    },
}

impl Madt {
    pub(super) fn new(table: Table) -> Result<Self, AcpiError> {
        if table.bytes().len() < 44 {
            return Err(AcpiError::Length);
        }
        let madt = Self { table };
        for entry in madt.entries() {
            entry?;
        }
        Ok(madt)
    }

    pub fn local_controller_address(&self) -> u32 {
        le_u32(&self.table.bytes()[36..40])
    }

    pub fn local_apic_address(&self) -> u64 {
        self.entries()
            .filter_map(Result::ok)
            .find_map(|entry| match entry {
                MadtEntry::LocalApicAddressOverride { address } => Some(address),
                _ => None,
            })
            .unwrap_or(u64::from(self.local_controller_address()))
    }

    pub fn entries(&self) -> MadtEntries<'_> {
        MadtEntries {
            bytes: &self.table.bytes()[44..],
            cursor: 0,
            failed: false,
        }
    }

    pub fn enabled_processor_count(&self) -> usize {
        self.entries()
            .filter_map(Result::ok)
            .filter(|entry| {
                #[cfg(target_arch = "x86_64")]
                {
                    matches!(
                        entry,
                        MadtEntry::LocalApic { enabled: true, .. }
                            | MadtEntry::X2Apic { enabled: true, .. }
                    )
                }
                #[cfg(target_arch = "aarch64")]
                {
                    matches!(entry, MadtEntry::Gicc { enabled: true, .. })
                }
                #[cfg(target_arch = "riscv64")]
                {
                    matches!(entry, MadtEntry::Rintc { enabled: true, .. })
                }
            })
            .count()
    }
}

pub struct MadtEntries<'a> {
    bytes: &'a [u8],
    cursor: usize,
    failed: bool,
}

impl Iterator for MadtEntries<'_> {
    type Item = Result<MadtEntry, AcpiError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.cursor == self.bytes.len() {
            return None;
        }
        let remaining = &self.bytes[self.cursor..];
        if remaining.len() < 2 || remaining[1] < 2 || usize::from(remaining[1]) > remaining.len() {
            self.failed = true;
            return Some(Err(AcpiError::Malformed));
        }
        let length = usize::from(remaining[1]);
        let entry = &remaining[..length];
        self.cursor += length;
        let value = match entry[0] {
            0 if length >= 8 => MadtEntry::LocalApic {
                uid: entry[2],
                apic_id: entry[3],
                enabled: le_u32(&entry[4..8]) & 1 != 0,
            },
            1 if length >= 12 => MadtEntry::IoApic {
                id: entry[2],
                address: le_u32(&entry[4..8]),
                gsi_base: le_u32(&entry[8..12]),
            },
            2 if length >= 10 => MadtEntry::InterruptOverride {
                bus: entry[2],
                source: entry[3],
                gsi: le_u32(&entry[4..8]),
                flags: le_u16(&entry[8..10]),
            },
            3 if length >= 8 => MadtEntry::NmiSource {
                flags: le_u16(&entry[2..4]),
                gsi: le_u32(&entry[4..8]),
            },
            4 if length >= 6 => MadtEntry::LocalApicNmi {
                uid: entry[2],
                flags: le_u16(&entry[3..5]),
                lint: entry[5],
            },
            5 if length >= 12 => MadtEntry::LocalApicAddressOverride {
                address: le_u64(&entry[4..12]),
            },
            9 if length >= 16 => MadtEntry::X2Apic {
                apic_id: le_u32(&entry[4..8]),
                enabled: le_u32(&entry[8..12]) & 1 != 0,
                uid: le_u32(&entry[12..16]),
            },
            10 if length >= 12 => MadtEntry::X2ApicNmi {
                flags: le_u16(&entry[2..4]),
                uid: le_u32(&entry[4..8]),
                lint: entry[8],
            },
            0x0b if length >= 76 => MadtEntry::Gicc {
                uid: le_u32(&entry[8..12]),
                enabled: le_u32(&entry[12..16]) & 1 != 0,
                address: le_u64(&entry[24..32]),
                mpidr: le_u64(&entry[68..76]),
            },
            0x0c if length >= 24 => MadtEntry::Gicd {
                id: le_u32(&entry[4..8]),
                address: le_u64(&entry[8..16]),
                version: entry[20],
            },
            0x0e if length >= 16 => MadtEntry::Gicr {
                address: le_u64(&entry[4..12]),
                length: le_u32(&entry[12..16]),
            },
            0x18 if length >= 36 => MadtEntry::Rintc {
                enabled: le_u32(&entry[4..8]) & 1 != 0,
                hart_id: le_u64(&entry[8..16]),
                uid: le_u32(&entry[16..20]),
            },
            0x1b if length >= 36 => MadtEntry::Plic {
                id: entry[3],
                max_priority: le_u16(&entry[14..16]),
                size: le_u32(&entry[20..24]),
                address: le_u64(&entry[24..32]),
            },
            0 | 1 | 2 | 3 | 4 | 5 | 9 | 10 | 0x0b | 0x0c | 0x0e | 0x18 | 0x1b => {
                return Some(Err(AcpiError::Malformed));
            }
            kind => MadtEntry::Other { kind },
        };
        Some(Ok(value))
    }
}
