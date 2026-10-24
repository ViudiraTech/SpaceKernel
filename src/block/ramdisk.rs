/*
 *
 *       src/block/ramdisk.rs
 *       In-memory block device for boot filesystems and storage self-tests
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! RAM-backed block device (`ram0`) supporting sector-level reads and writes.

use super::{BlockDevice, BlockDeviceInfo, BlockDeviceKind, BlockError, SECTOR_SIZE};
use crate::sync::SpinLock;
use alloc::{string::String, sync::Arc, vec, vec::Vec};

pub struct RamDisk {
    info: BlockDeviceInfo,
    storage: SpinLock<Vec<u8>>,
}

impl RamDisk {
    pub fn new(name: &str, sectors: usize) -> Arc<Self> {
        let bytes = sectors * SECTOR_SIZE;
        Arc::new(Self {
            info: BlockDeviceInfo {
                name: String::from(name),
                model: String::from("SpaceKernel RAMDisk"),
                serial: String::from("RAMDISK0001"),
                kind: BlockDeviceKind::RamDisk,
                block_size: SECTOR_SIZE,
                total_blocks: sectors as u64,
                read_only: false,
                removable: false,
            },
            storage: SpinLock::new(vec![0u8; bytes]),
        })
    }
}

impl BlockDevice for RamDisk {
    fn info(&self) -> BlockDeviceInfo {
        self.info.clone()
    }

    fn read_blocks(&self, lba: u64, count: usize, buf: &mut [u8]) -> Result<(), BlockError> {
        let needed = count
            .checked_mul(SECTOR_SIZE)
            .ok_or(BlockError::InvalidBuffer)?;
        if buf.len() < needed {
            return Err(BlockError::InvalidBuffer);
        }
        let end_lba = lba
            .checked_add(count as u64)
            .ok_or(BlockError::OutOfRange)?;
        if end_lba > self.info.total_blocks {
            return Err(BlockError::OutOfRange);
        }
        let start_byte = (lba as usize) * SECTOR_SIZE;
        let data = self.storage.lock();
        buf[..needed].copy_from_slice(&data[start_byte..start_byte + needed]);
        Ok(())
    }

    fn write_blocks(&self, lba: u64, count: usize, buf: &[u8]) -> Result<(), BlockError> {
        let needed = count
            .checked_mul(SECTOR_SIZE)
            .ok_or(BlockError::InvalidBuffer)?;
        if buf.len() < needed {
            return Err(BlockError::InvalidBuffer);
        }
        let end_lba = lba
            .checked_add(count as u64)
            .ok_or(BlockError::OutOfRange)?;
        if end_lba > self.info.total_blocks {
            return Err(BlockError::OutOfRange);
        }
        let start_byte = (lba as usize) * SECTOR_SIZE;
        let mut data = self.storage.lock();
        data[start_byte..start_byte + needed].copy_from_slice(&buf[..needed]);
        Ok(())
    }

    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
}
