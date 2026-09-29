use super::{
    AcpiError,
    sdt::{Table, le_u16, le_u64},
};

#[derive(Clone, Copy)]
pub struct Mcfg {
    table: Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciSegment {
    pub base: u64,
    pub group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

impl Mcfg {
    pub(super) fn new(table: Table) -> Result<Self, AcpiError> {
        if table.bytes().len() < 44 || (table.bytes().len() - 44) % 16 != 0 {
            return Err(AcpiError::Length);
        }
        let mcfg = Self { table };
        for segment in mcfg.segments() {
            if segment.base == 0
                || segment.base & ((1 << 20) - 1) != 0
                || segment.start_bus > segment.end_bus
            {
                return Err(AcpiError::Malformed);
            }
        }
        Ok(mcfg)
    }

    pub fn segments(&self) -> impl Iterator<Item = PciSegment> + '_ {
        self.table.bytes()[44..]
            .chunks_exact(16)
            .map(|bytes| PciSegment {
                base: le_u64(&bytes[0..8]),
                group: le_u16(&bytes[8..10]),
                start_bus: bytes[10],
                end_bus: bytes[11],
            })
    }
}
