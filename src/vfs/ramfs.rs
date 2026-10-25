/*
 *
 *       src/vfs/ramfs.rs
 *       In-memory hierarchical filesystem (RamFS) supporting files, directories and symlinks
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Full-featured RamFS implementation for `/` and temporary mounts.

use super::{
    abi::RAMFS_MAGIC,
    inode::{DirEntry, FileSystem, Inode, InodeKind, Stat, StatFs, VfsError},
};
use crate::sync::SpinLock;
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_RAM_INO: AtomicU64 = AtomicU64::new(100);

fn alloc_ino() -> u64 {
    NEXT_RAM_INO.fetch_add(1, Ordering::Relaxed)
}

enum RamNodeData {
    Regular(Vec<u8>),
    Directory(BTreeMap<String, Arc<dyn Inode>>),
    Symlink(String),
}

pub struct RamInode {
    ino: u64,
    kind: InodeKind,
    mode: SpinLock<u32>,
    owner: SpinLock<(u32, u32)>,
    data: SpinLock<RamNodeData>,
}

impl RamInode {
    pub fn new_dir(mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino: alloc_ino(),
            kind: InodeKind::Directory,
            owner: SpinLock::new((0, 0)),
            mode: SpinLock::new(mode),
            data: SpinLock::new(RamNodeData::Directory(BTreeMap::new())),
        })
    }

    pub fn new_file(mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino: alloc_ino(),
            kind: InodeKind::Regular,
            owner: SpinLock::new((0, 0)),
            mode: SpinLock::new(mode),
            data: SpinLock::new(RamNodeData::Regular(Vec::new())),
        })
    }

    pub fn new_symlink(target: &str) -> Arc<Self> {
        Arc::new(Self {
            ino: alloc_ino(),
            kind: InodeKind::Symlink,
            owner: SpinLock::new((0, 0)),
            mode: SpinLock::new(0o777),
            data: SpinLock::new(RamNodeData::Symlink(String::from(target))),
        })
    }
}

impl Inode for RamInode {
    fn insert_node(&self, name: &str, node: Arc<dyn Inode>) -> Result<(), VfsError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.as_bytes().contains(&0)
            || name.len() > 255
        {
            return Err(VfsError::InvalidArgument);
        }
        if let RamNodeData::Directory(children) = &mut *self.data.lock() {
            if children.contains_key(name) {
                return Err(VfsError::AlreadyExists);
            }
            children.insert(String::from(name), node);
            Ok(())
        } else {
            Err(VfsError::NotADirectory)
        }
    }
    fn chmod(&self, mode: u32) -> Result<(), VfsError> {
        *self.mode.lock() = mode & 0o7777;
        Ok(())
    }
    fn chown(&self, uid: u32, gid: u32) -> Result<(), VfsError> {
        *self.owner.lock() = (uid, gid);
        Ok(())
    }

    fn metadata(&self) -> Result<Stat, VfsError> {
        let mode = *self.mode.lock();
        let size = match &*self.data.lock() {
            RamNodeData::Regular(bytes) => bytes.len() as u64,
            RamNodeData::Directory(children) => (children.len() * 64) as u64,
            RamNodeData::Symlink(target) => target.len() as u64,
        };
        let mut stat = Stat::simple(self.ino, self.kind, mode, size);
        let (uid, gid) = *self.owner.lock();
        stat.uid = uid;
        stat.gid = gid;
        Ok(stat)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        match &*self.data.lock() {
            RamNodeData::Regular(bytes) => {
                if offset >= bytes.len() as u64 || buf.is_empty() {
                    return Ok(0);
                }
                let start = offset as usize;
                let count = (bytes.len() - start).min(buf.len());
                buf[..count].copy_from_slice(&bytes[start..start + count]);
                Ok(count)
            }
            RamNodeData::Directory(_) => Err(VfsError::IsADirectory),
            RamNodeData::Symlink(_) => Err(VfsError::InvalidArgument),
        }
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        match &mut *self.data.lock() {
            RamNodeData::Regular(bytes) => {
                let start = offset as usize;
                let end = start.checked_add(buf.len()).ok_or(VfsError::NoSpace)?;
                if end > 64 * 1024 * 1024 {
                    return Err(VfsError::NoSpace);
                }
                if end > bytes.len() {
                    bytes.resize(end, 0);
                }
                bytes[start..end].copy_from_slice(buf);
                Ok(buf.len())
            }
            RamNodeData::Directory(_) => Err(VfsError::IsADirectory),
            RamNodeData::Symlink(_) => Err(VfsError::InvalidArgument),
        }
    }

    fn truncate(&self, size: u64) -> Result<(), VfsError> {
        match &mut *self.data.lock() {
            RamNodeData::Regular(bytes) => {
                if size > 64 * 1024 * 1024 {
                    return Err(VfsError::NoSpace);
                }
                bytes.resize(size as usize, 0);
                Ok(())
            }
            RamNodeData::Directory(_) => Err(VfsError::IsADirectory),
            RamNodeData::Symlink(_) => Err(VfsError::InvalidArgument),
        }
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        match &*self.data.lock() {
            RamNodeData::Directory(children) => children
                .get(name)
                .cloned()
                .map(|n| n as Arc<dyn Inode>)
                .ok_or(VfsError::NotFound),
            _ => Err(VfsError::NotADirectory),
        }
    }

    fn create(&self, name: &str, kind: InodeKind, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > 255
            || name.as_bytes().contains(&0)
            || name.contains('/')
        {
            return Err(VfsError::InvalidArgument);
        }
        match &mut *self.data.lock() {
            RamNodeData::Directory(children) => {
                if children.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }
                let node = match kind {
                    InodeKind::Regular => RamInode::new_file(mode),
                    InodeKind::Directory => RamInode::new_dir(mode),
                    _ => return Err(VfsError::NotSupported),
                };
                children.insert(String::from(name), node.clone());
                Ok(node)
            }
            _ => Err(VfsError::NotADirectory),
        }
    }

    fn mkdir(&self, name: &str, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        self.create(name, InodeKind::Directory, mode)
    }

    fn unlink(&self, name: &str) -> Result<(), VfsError> {
        match &mut *self.data.lock() {
            RamNodeData::Directory(children) => {
                let existing = children.get(name).ok_or(VfsError::NotFound)?;
                if existing.metadata()?.kind == InodeKind::Directory {
                    return Err(VfsError::IsADirectory);
                }
                children.remove(name);
                Ok(())
            }
            _ => Err(VfsError::NotADirectory),
        }
    }

    fn rmdir(&self, name: &str) -> Result<(), VfsError> {
        match &mut *self.data.lock() {
            RamNodeData::Directory(children) => {
                let existing = children.get(name).ok_or(VfsError::NotFound)?;
                if existing.metadata()?.kind != InodeKind::Directory {
                    return Err(VfsError::NotADirectory);
                }
                if existing
                    .readdir()?
                    .iter()
                    .any(|e| e.name != "." && e.name != "..")
                {
                    return Err(VfsError::NotEmpty);
                }
                children.remove(name);
                Ok(())
            }
            _ => Err(VfsError::NotADirectory),
        }
    }

    fn symlink(&self, name: &str, target: &str) -> Result<Arc<dyn Inode>, VfsError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > 255
            || name.as_bytes().contains(&0)
            || name.contains('/')
        {
            return Err(VfsError::InvalidArgument);
        }
        match &mut *self.data.lock() {
            RamNodeData::Directory(children) => {
                if children.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }
                let node = RamInode::new_symlink(target);
                children.insert(String::from(name), node.clone());
                Ok(node)
            }
            _ => Err(VfsError::NotADirectory),
        }
    }

    fn readlink(&self) -> Result<String, VfsError> {
        match &*self.data.lock() {
            RamNodeData::Symlink(target) => Ok(target.clone()),
            _ => Err(VfsError::InvalidArgument),
        }
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        match &*self.data.lock() {
            RamNodeData::Directory(children) => {
                let mut entries = Vec::with_capacity(children.len() + 2);
                entries.push(DirEntry {
                    ino: self.ino,
                    name: String::from("."),
                    kind: InodeKind::Directory,
                });
                entries.push(DirEntry {
                    ino: self.ino,
                    name: String::from(".."),
                    kind: InodeKind::Directory,
                });
                for (name, child) in children.iter() {
                    entries.push(DirEntry {
                        ino: child.metadata()?.ino,
                        name: name.clone(),
                        kind: child.metadata()?.kind,
                    });
                }
                Ok(entries)
            }
            _ => Err(VfsError::NotADirectory),
        }
    }
}

pub struct RamFs {
    root: Arc<RamInode>,
}

impl Default for RamFs {
    fn default() -> Self {
        Self::new()
    }
}

impl RamFs {
    pub fn new() -> Self {
        Self {
            root: RamInode::new_dir(0o755),
        }
    }
}

impl FileSystem for RamFs {
    fn fs_type(&self) -> &'static str {
        "ramfs"
    }

    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }

    fn statfs(&self) -> Result<StatFs, VfsError> {
        let pmm_stats = crate::mm::pmm::stats();
        Ok(StatFs {
            magic: RAMFS_MAGIC,
            block_size: 4096,
            total_blocks: pmm_stats.frames as u64,
            free_blocks: pmm_stats.free as u64,
            total_files: 4096,
            free_files: 4000,
            max_name_len: 255,
        })
    }
}
