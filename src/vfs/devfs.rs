/*
 *
 *       src/vfs/devfs.rs
 *       Device filesystem (/dev) exposing character and block device nodes
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Dynamic `/dev` filesystem exposing standard character devices (`null`, `zero`, `full`,
//! `urandom`, `console`, `tty`, `tty0`, `ttyS0`) and all registered block devices (`ram0`,
//! `sda`, `nvme0n1`, `hda`, and partitions).

use super::{
    abi::{self, DEVFS_MAGIC},
    fd::ConsoleInode,
    inode::{DirEntry, FileSystem, Inode, InodeKind, Stat, StatFs, VfsError},
};
use crate::block::{self, BlockDevice};
use alloc::{string::String, sync::Arc, vec::Vec};

enum SpecialCharKind {
    Null,
    Zero,
    Full,
    Urandom,
}

struct SpecialCharInode {
    ino: u64,
    kind: SpecialCharKind,
    rdev: u64,
}

impl Inode for SpecialCharInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let mut s = Stat::simple(self.ino, InodeKind::CharDevice, 0o666, 0);
        s.rdev = self.rdev;
        Ok(s)
    }

    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        match self.kind {
            SpecialCharKind::Null => Ok(0),
            SpecialCharKind::Zero | SpecialCharKind::Full => {
                buf.fill(0);
                Ok(buf.len())
            }
            SpecialCharKind::Urandom => {
                crate::random::fill(buf)?;
                Ok(buf.len())
            }
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        match self.kind {
            SpecialCharKind::Null | SpecialCharKind::Zero | SpecialCharKind::Urandom => {
                Ok(buf.len())
            }
            SpecialCharKind::Full => Err(VfsError::NoSpace),
        }
    }
}

/// Block device node in `/dev/<name>` backed by `crate::block`.
struct BlockDevInode {
    dev_id: usize,
    dev: Arc<dyn BlockDevice>,
}

impl Inode for BlockDevInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let info = self.dev.info();
        let cap = info.capacity_bytes();
        let mut s = Stat::simple(
            2000 + self.dev_id as u64,
            InodeKind::BlockDevice,
            if info.read_only { 0o440 } else { 0o660 },
            cap,
        );
        s.blksize = info.block_size as u64;
        s.blocks = cap / 512;
        s.rdev = (8 << 8) | (self.dev_id as u64);
        Ok(s)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        block::read_at(self.dev_id, &self.dev, offset, buf).map_err(|_| VfsError::IoError)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        block::write_at(self.dev_id, &self.dev, offset, buf).map_err(|_| VfsError::IoError)
    }

    fn sync(&self) -> Result<(), VfsError> {
        block::cache::record_flush();
        self.dev.flush().map_err(|_| VfsError::IoError)
    }

    fn ioctl(&self, cmd: u64, _arg: u64) -> Result<u64, VfsError> {
        let info = self.dev.info();
        match cmd {
            abi::BLKGETSIZE64 => Ok(info.capacity_bytes()),
            abi::BLKGETSIZE => Ok(info.capacity_bytes() / 512),
            abi::BLKSSZGET => Ok(info.block_size as u64),
            abi::BLKROGET => Ok(u64::from(info.read_only)),
            _ => Err(VfsError::InvalidArgument),
        }
    }
}

struct DevRootInode;

impl Inode for DevRootInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(1, InodeKind::Directory, 0o755, 0))
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        match name {
            "null" => Ok(Arc::new(SpecialCharInode {
                ino: 10,
                kind: SpecialCharKind::Null,
                rdev: (1 << 8) | 3,
            })),
            "zero" => Ok(Arc::new(SpecialCharInode {
                ino: 11,
                kind: SpecialCharKind::Zero,
                rdev: (1 << 8) | 5,
            })),
            "full" => Ok(Arc::new(SpecialCharInode {
                ino: 12,
                kind: SpecialCharKind::Full,
                rdev: (1 << 8) | 7,
            })),
            "random" => Ok(Arc::new(SpecialCharInode {
                ino: 13,
                kind: SpecialCharKind::Urandom,
                rdev: (1 << 8) | 8,
            })),
            "urandom" => Ok(Arc::new(SpecialCharInode {
                ino: 14,
                kind: SpecialCharKind::Urandom,
                rdev: (1 << 8) | 9,
            })),
            "console" => Ok(Arc::new(ConsoleInode::with_ino(15))),
            "tty" => Ok(Arc::new(super::fd::ControllingTty)),
            "tty0" => Ok(Arc::new(ConsoleInode::for_vt(17, 0))),
            "ptmx" => Ok(Arc::new(crate::tty::pty::PtmxNode)),
            "tty1" | "tty2" | "tty3" | "tty4" | "tty5" | "tty6" | "tty7" | "tty8" => {
                let vt = usize::from(name.as_bytes()[3] - b'0');
                Ok(Arc::new(ConsoleInode::for_vt(30 + vt as u64, vt)))
            }
            "ttyS0" => Ok(Arc::new(ConsoleInode::with_ino(18))),
            _ => {
                if let Some((dev_id, dev)) = block::find_device(name) {
                    Ok(Arc::new(BlockDevInode { dev_id, dev }))
                } else {
                    Err(VfsError::NotFound)
                }
            }
        }
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        let mut entries = Vec::new();
        entries.push(DirEntry {
            ino: 1,
            name: String::from("."),
            kind: InodeKind::Directory,
        });
        entries.push(DirEntry {
            ino: 1,
            name: String::from(".."),
            kind: InodeKind::Directory,
        });
        for (idx, name) in [
            "null", "zero", "full", "random", "urandom", "console", "tty", "tty0", "ttyS0", "ptmx",
            "tty1", "tty2", "tty3", "tty4", "tty5", "tty6", "tty7", "tty8",
        ]
        .iter()
        .enumerate()
        {
            entries.push(DirEntry {
                ino: 10 + idx as u64,
                name: String::from(*name),
                kind: InodeKind::CharDevice,
            });
        }
        for (dev_id, dev) in block::devices().iter().enumerate() {
            entries.push(DirEntry {
                ino: 2000 + dev_id as u64,
                name: dev.info().name,
                kind: InodeKind::BlockDevice,
            });
        }
        Ok(entries)
    }
}

pub struct DevFs {
    root: Arc<DevRootInode>,
}

impl Default for DevFs {
    fn default() -> Self {
        Self::new()
    }
}

impl DevFs {
    pub fn new() -> Self {
        Self {
            root: Arc::new(DevRootInode),
        }
    }
}

impl FileSystem for DevFs {
    fn fs_type(&self) -> &'static str {
        "devfs"
    }

    fn cache_mode(&self) -> super::CacheMode {
        super::CacheMode::Revalidate
    }

    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }

    fn statfs(&self) -> Result<StatFs, VfsError> {
        Ok(StatFs {
            magic: DEVFS_MAGIC,
            block_size: 4096,
            total_blocks: 0,
            free_blocks: 0,
            total_files: 64,
            free_files: 64,
            max_name_len: 255,
        })
    }
}
