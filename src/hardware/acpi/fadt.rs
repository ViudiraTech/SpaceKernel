use super::{
    AcpiError,
    sdt::{Table, le_u16, le_u32, le_u64, read_table},
};

#[derive(Clone, Copy)]
pub struct Fadt {
    table: Table,
}

impl Fadt {
    pub(super) fn new(table: Table) -> Result<Self, AcpiError> {
        if table.bytes().len() < 116 {
            return Err(AcpiError::Length);
        }
        Ok(Self { table })
    }

    pub fn sci_interrupt(&self) -> u16 {
        le_u16(&self.table.bytes()[46..48])
    }
    pub fn flags(&self) -> u32 {
        le_u32(&self.table.bytes()[112..116])
    }
    pub fn hardware_reduced(&self) -> bool {
        self.flags() & (1 << 20) != 0
    }

    pub fn dsdt(&self) -> Result<Option<Table>, AcpiError> {
        let bytes = self.table.bytes();
        let extended = if bytes.len() >= 148 {
            le_u64(&bytes[140..148])
        } else {
            0
        };
        let legacy = u64::from(le_u32(&bytes[40..44]));
        let physical = if extended != 0 { extended } else { legacy };
        if physical == 0 {
            return Ok(None);
        }
        let table = read_table(physical)?;
        if table.signature() != b"DSDT" {
            return Err(AcpiError::Signature);
        }
        Ok(Some(table))
    }
}
