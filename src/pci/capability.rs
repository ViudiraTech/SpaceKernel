/*
 *
 *       src/pci/capability.rs
 *       Bounded PCI capability-chain enumeration
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use alloc::vec::Vec;

use super::{Address, PciError, config_for};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub id: u8,
    pub offset: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedCapability {
    pub id: u16,
    pub version: u8,
    pub offset: u16,
}

pub fn standard(address: Address) -> Result<Vec<Capability>, PciError> {
    let mut result = Vec::with_capacity(48);
    let config = config_for(address)?;
    if config.read_u16(0x06)? & 0x10 == 0 {
        return Ok(Vec::new());
    }
    let header_type = config.read_u8(0x0e)? & 0x7f;
    let head = if header_type == 2 { 0x14 } else { 0x34 };
    let mut offset = config.read_u8(head)? & !3;
    let mut seen = [false; 64];
    while offset != 0 {
        if !(0x40..=0xfc).contains(&offset) || seen[usize::from(offset / 4)] {
            return Err(PciError::MalformedCapability);
        }
        seen[usize::from(offset / 4)] = true;
        let id = config.read_u8(u16::from(offset))?;
        let next = config.read_u8(u16::from(offset) + 1)? & !3;
        result.push(Capability { id, offset });
        offset = next;
    }
    Ok(result)
}

pub fn extended(address: Address) -> Result<Vec<ExtendedCapability>, PciError> {
    let mut result = Vec::with_capacity(960);
    let config = config_for(address)?;
    let mut offset = 0x100u16;
    let mut seen = [false; 1024];
    loop {
        if !(0x100..=0xffc).contains(&offset) || offset & 3 != 0 || seen[usize::from(offset / 4)] {
            return Err(PciError::MalformedCapability);
        }
        seen[usize::from(offset / 4)] = true;
        let header = config.read_u32(offset)?;
        if header == 0 || header == u32::MAX {
            break;
        }
        result.push(ExtendedCapability {
            id: header as u16,
            version: ((header >> 16) & 0x0f) as u8,
            offset,
        });
        let next = ((header >> 20) & 0xfff) as u16;
        if next == 0 {
            break;
        }
        offset = next;
    }
    Ok(result)
}
