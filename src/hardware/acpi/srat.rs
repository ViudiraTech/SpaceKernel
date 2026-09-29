use super::{
    AcpiError,
    sdt::{Table, le_u32, le_u64},
};

#[derive(Clone, Copy)]
pub struct Srat {
    table: Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryAffinity {
    pub proximity_domain: u32,
    pub base: u64,
    pub length: u64,
    pub enabled: bool,
    pub hot_pluggable: bool,
}

impl Srat {
    pub(super) fn new(table: Table) -> Result<Self, AcpiError> {
        if table.bytes().len() < 48 {
            return Err(AcpiError::Length);
        }
        let srat = Self { table };
        let mut cursor = 48;
        let bytes = srat.table.bytes();
        while cursor < bytes.len() {
            if cursor + 2 > bytes.len()
                || bytes[cursor + 1] < 2
                || cursor + usize::from(bytes[cursor + 1]) > bytes.len()
            {
                return Err(AcpiError::Malformed);
            }
            cursor += usize::from(bytes[cursor + 1]);
        }
        Ok(srat)
    }

    pub fn memory_affinities(&self) -> impl Iterator<Item = MemoryAffinity> + '_ {
        let bytes = &self.table.bytes()[48..];
        SratEntries { bytes, cursor: 0 }.filter_map(|entry| {
            if entry[0] != 1 || entry.len() < 40 {
                return None;
            }
            let flags = le_u32(&entry[28..32]);
            Some(MemoryAffinity {
                proximity_domain: le_u32(&entry[2..6]),
                base: le_u64(&entry[8..16]),
                length: le_u64(&entry[16..24]),
                enabled: flags & 1 != 0,
                hot_pluggable: flags & 2 != 0,
            })
        })
    }
}

struct SratEntries<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for SratEntries<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == self.bytes.len() {
            return None;
        }
        let length = usize::from(self.bytes[self.cursor + 1]);
        let entry = &self.bytes[self.cursor..self.cursor + length];
        self.cursor += length;
        Some(entry)
    }
}
