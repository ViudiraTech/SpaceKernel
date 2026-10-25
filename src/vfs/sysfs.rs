/*
 *       src/vfs/sysfs.rs
 *       Registry-backed device topology and attribute filesystem
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use super::{FileSystem, Inode, StatFs, VfsError};
use alloc::sync::Arc;
pub struct SysFs;
impl FileSystem for SysFs {
    fn fs_type(&self) -> &'static str {
        "sysfs"
    }
    fn cache_mode(&self) -> super::CacheMode {
        super::CacheMode::Revalidate
    }
    fn root_inode(&self) -> Arc<dyn Inode> {
        crate::device::sys_root()
    }
    fn statfs(&self) -> Result<StatFs, VfsError> {
        Ok(StatFs {
            magic: 0x62656572,
            block_size: 4096,
            total_blocks: 0,
            free_blocks: 0,
            total_files: crate::device::records().len() as u64,
            free_files: 0,
            max_name_len: 255,
        })
    }
}
