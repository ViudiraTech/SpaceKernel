/*
 *
 *       src/pci/config.rs
 *       Serialized PCI configuration-register access
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use crate::sync::Guard;

use super::{PciError, ecam::BusState};

/// One function's 4 KiB ECAM page, locked against concurrent config updates.
///
/// Holding this guard across unrelated work would stall all functions on the
/// same bus. Callers should read or update registers and release it promptly.
pub struct ConfigSpace<'a> {
    _bus: Guard<'a, BusState>,
    base: usize,
}

impl<'a> ConfigSpace<'a> {
    pub(super) fn new(bus: Guard<'a, BusState>, base: usize) -> Self {
        Self { _bus: bus, base }
    }

    fn offset(offset: u16, width: usize) -> Result<usize, PciError> {
        let offset = usize::from(offset);
        if offset % width != 0 || offset.checked_add(width).is_none_or(|end| end > 4096) {
            return Err(PciError::InvalidOffset);
        }
        Ok(offset)
    }

    pub fn read_u8(&self, offset: u16) -> Result<u8, PciError> {
        let offset = Self::offset(offset, 1)?;
        // SAFETY: ECAM is device-mapped, offset is within the locked 4 KiB page.
        Ok(unsafe { (self.base.wrapping_add(offset) as *const u8).read_volatile() })
    }

    pub fn read_u16(&self, offset: u16) -> Result<u16, PciError> {
        let offset = Self::offset(offset, 2)?;
        // SAFETY: the checked offset is aligned and entirely within the page.
        let value = unsafe { (self.base.wrapping_add(offset) as *const u16).read_volatile() };
        Ok(u16::from_le(value))
    }

    pub fn read_u32(&self, offset: u16) -> Result<u32, PciError> {
        let offset = Self::offset(offset, 4)?;
        // SAFETY: the checked offset is aligned and entirely within the page.
        let value = unsafe { (self.base.wrapping_add(offset) as *const u32).read_volatile() };
        Ok(u32::from_le(value))
    }

    pub fn write_u8(&mut self, offset: u16, value: u8) -> Result<(), PciError> {
        let offset = Self::offset(offset, 1)?;
        // SAFETY: exclusive bus guard and bounded ECAM address protect this write.
        unsafe { (self.base.wrapping_add(offset) as *mut u8).write_volatile(value) };
        Ok(())
    }

    pub fn write_u16(&mut self, offset: u16, value: u16) -> Result<(), PciError> {
        let offset = Self::offset(offset, 2)?;
        // SAFETY: exclusive bus guard; the register access is naturally aligned.
        unsafe { (self.base.wrapping_add(offset) as *mut u16).write_volatile(value.to_le()) };
        Ok(())
    }

    pub fn write_u32(&mut self, offset: u16, value: u32) -> Result<(), PciError> {
        let offset = Self::offset(offset, 4)?;
        // SAFETY: exclusive bus guard; the register access is naturally aligned.
        unsafe { (self.base.wrapping_add(offset) as *mut u32).write_volatile(value.to_le()) };
        Ok(())
    }
}
