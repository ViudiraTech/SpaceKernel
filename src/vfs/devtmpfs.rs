/*
 *       src/vfs/devtmpfs.rs
 *       Shared kernel-populated mutable device filesystem
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use super::{FileSystem, Inode, StatFs, VfsError};
use alloc::sync::Arc;
pub struct DevTmpFs;
impl FileSystem for DevTmpFs {
    fn fs_type(&self) -> &'static str {
        "devtmpfs"
    }
    fn root_inode(&self) -> Arc<dyn Inode> {
        crate::device::dev_root()
    }
    fn statfs(&self) -> Result<StatFs, VfsError> {
        let stats = crate::mm::pmm::stats();
        Ok(StatFs {
            magic: 0x01021994,
            block_size: 4096,
            total_blocks: stats.frames as u64,
            free_blocks: stats.free as u64,
            total_files: crate::device::records().len() as u64,
            free_files: 0,
            max_name_len: 255,
        })
    }
}
