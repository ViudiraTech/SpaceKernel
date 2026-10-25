/*
 *
 *       src/vfs/rootfs.rs
 *       Read-only block device backed by a Limine module
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! The FAT32 root image is loaded by Limine as a module and exposed as a
//! read-only block device so the existing FAT32 driver can mount it directly.

use crate::{
    block::{BlockDevice, BlockDeviceInfo, BlockDeviceKind, BlockError, SECTOR_SIZE},
    boot,
};
use alloc::{string::String, sync::Arc};

struct ModuleDisk {
    data: &'static [u8],
}

impl BlockDevice for ModuleDisk {
    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: String::from("rootfs"),
            model: String::from("Limine rootfs module"),
            serial: String::new(),
            kind: BlockDeviceKind::RamDisk,
            block_size: SECTOR_SIZE,
            total_blocks: (self.data.len() / SECTOR_SIZE) as u64,
            read_only: true,
            removable: false,
        }
    }

    fn read_blocks(&self, lba: u64, count: usize, buffer: &mut [u8]) -> Result<(), BlockError> {
        let offset = (lba as usize)
            .checked_mul(SECTOR_SIZE)
            .ok_or(BlockError::OutOfRange)?;
        let length = count
            .checked_mul(SECTOR_SIZE)
            .ok_or(BlockError::OutOfRange)?;
        if buffer.len() < length {
            return Err(BlockError::InvalidBuffer);
        }
        let end = offset.checked_add(length).ok_or(BlockError::OutOfRange)?;
        let source = self.data.get(offset..end).ok_or(BlockError::OutOfRange)?;
        buffer[..length].copy_from_slice(source);
        Ok(())
    }

    fn write_blocks(&self, _lba: u64, _count: usize, _buffer: &[u8]) -> Result<(), BlockError> {
        Err(BlockError::ReadOnly)
    }

    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
}

/// Return the `rootfs.img` Limine module as a block device.
pub fn module_disk() -> Option<Arc<dyn BlockDevice>> {
    for file in boot::modules() {
        let path = file.path();
        if path.ends_with("rootfs.img") || path.ends_with("rootfs.fat") {
            return Some(Arc::new(ModuleDisk { data: file.data() }));
        }
    }
    None
}
