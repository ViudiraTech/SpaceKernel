/*
 *       src/vfs/tree.rs
 *       Stable virtual directories, symlinks, attributes and open snapshots
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use super::{DirEntry, Inode, InodeKind, Stat, VfsError};
use crate::sync::SpinLock;
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0x1000_0000);
pub fn ino() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 255
        && !name.contains('/')
        && !name.as_bytes().contains(&0)
}
pub struct Directory {
    metadata: SpinLock<Stat>,
    children: SpinLock<BTreeMap<String, Arc<dyn Inode>>>,
    writable: bool,
}
impl Directory {
    pub fn new(mode: u32, writable: bool) -> Arc<Self> {
        Arc::new(Self {
            metadata: SpinLock::new(Stat::simple(ino(), InodeKind::Directory, mode, 0)),
            children: SpinLock::new(BTreeMap::new()),
            writable,
        })
    }
    pub fn insert(&self, name: &str, inode: Arc<dyn Inode>) -> Result<(), VfsError> {
        if !name_valid(name) {
            return Err(VfsError::InvalidArgument);
        }
        let mut children = self.children.lock();
        if children.contains_key(name) {
            return Err(VfsError::AlreadyExists);
        }
        children.insert(String::from(name), inode);
        Ok(())
    }
    pub fn remove(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        self.children.lock().remove(name).ok_or(VfsError::NotFound)
    }
    pub fn empty(&self) -> bool {
        self.children.lock().is_empty()
    }
    pub fn entries(&self) -> Vec<(String, Arc<dyn Inode>)> {
        self.children
            .lock()
            .iter()
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect()
    }
}
impl Inode for Directory {
    fn remove_node(&self, name: &str) -> Result<(), VfsError> {
        self.remove(name)?;
        Ok(())
    }
    fn insert_node(&self, name: &str, node: Arc<dyn Inode>) -> Result<(), VfsError> {
        self.insert(name, node)
    }
    fn metadata(&self) -> Result<Stat, VfsError> {
        let mut stat = *self.metadata.lock();
        stat.size = self.children.lock().len() as u64 * 64;
        Ok(stat)
    }
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        self.children
            .lock()
            .get(name)
            .cloned()
            .ok_or(VfsError::NotFound)
    }
    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        let id = self.metadata.lock().ino;
        let mut out = alloc::vec![
            DirEntry {
                ino: id,
                name: String::from("."),
                kind: InodeKind::Directory
            },
            DirEntry {
                ino: id,
                name: String::from(".."),
                kind: InodeKind::Directory
            }
        ];
        for (name, node) in self.entries() {
            if let Ok(stat) = node.metadata() {
                out.push(DirEntry {
                    ino: stat.ino,
                    name,
                    kind: stat.kind,
                });
            }
        }
        Ok(out)
    }
    fn create(&self, name: &str, kind: InodeKind, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        let node: Arc<dyn Inode> = match kind {
            InodeKind::Directory => Self::new(mode, true),
            InodeKind::Regular => super::ramfs::RamInode::new_file(mode),
            _ => return Err(VfsError::NotSupported),
        };
        self.insert(name, node.clone())?;
        Ok(node)
    }
    fn mkdir(&self, name: &str, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        self.create(name, InodeKind::Directory, mode)
    }
    fn unlink(&self, name: &str) -> Result<(), VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        let node = self.lookup(name)?;
        if node.metadata()?.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        self.remove(name)?;
        Ok(())
    }
    fn rmdir(&self, name: &str) -> Result<(), VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        let node = self.lookup(name)?;
        if node.metadata()?.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        if node
            .readdir()?
            .iter()
            .any(|e| e.name != "." && e.name != "..")
        {
            return Err(VfsError::NotEmpty);
        }
        self.remove(name)?;
        Ok(())
    }
    fn symlink(&self, name: &str, target: &str) -> Result<Arc<dyn Inode>, VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        let n = Symlink::new(target);
        self.insert(name, n.clone())?;
        Ok(n)
    }
    fn chmod(&self, mode: u32) -> Result<(), VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        self.metadata.lock().mode = mode & 0o7777;
        Ok(())
    }
    fn chown(&self, uid: u32, gid: u32) -> Result<(), VfsError> {
        if !self.writable {
            return Err(VfsError::ReadOnlyFs);
        }
        let mut m = self.metadata.lock();
        m.uid = uid;
        m.gid = gid;
        Ok(())
    }
}

pub struct Symlink {
    id: u64,
    target: String,
}
impl Symlink {
    pub fn new(target: &str) -> Arc<Self> {
        Arc::new(Self {
            id: ino(),
            target: String::from(target),
        })
    }
}
impl Inode for Symlink {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(
            self.id,
            InodeKind::Symlink,
            0o777,
            self.target.len() as u64,
        ))
    }
    fn readlink(&self) -> Result<String, VfsError> {
        Ok(self.target.clone())
    }
}

pub struct Snapshot {
    id: u64,
    bytes: Vec<u8>,
}
impl Snapshot {
    pub fn new(id: u64, bytes: Vec<u8>) -> Arc<Self> {
        Arc::new(Self { id, bytes })
    }
}
impl Inode for Snapshot {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(
            self.id,
            InodeKind::Regular,
            0o444,
            self.bytes.len() as u64,
        ))
    }
    fn read_at(&self, offset: u64, dst: &mut [u8]) -> Result<usize, VfsError> {
        Ok(read_slice(&self.bytes, offset, dst))
    }
}
pub fn read_slice(source: &[u8], offset: u64, dst: &mut [u8]) -> usize {
    if offset >= source.len() as u64 {
        return 0;
    }
    let n = dst.len().min(source.len() - offset as usize);
    dst[..n].copy_from_slice(&source[offset as usize..offset as usize + n]);
    n
}
pub struct Attribute {
    id: u64,
    mode: u32,
    reader: Arc<dyn Fn() -> Result<Vec<u8>, VfsError> + Send + Sync>,
    writer: Option<Arc<dyn Fn(&[u8]) -> Result<(), VfsError> + Send + Sync>>,
}
impl Attribute {
    pub fn new(
        mode: u32,
        read: impl Fn() -> Result<Vec<u8>, VfsError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: ino(),
            mode,
            reader: Arc::new(read),
            writer: None,
        })
    }
    pub fn text(value: impl Into<String>) -> Arc<Self> {
        let value = value.into();
        Self::new(0o444, move || Ok(value.as_bytes().to_vec()))
    }
    pub fn writable(
        mode: u32,
        read: impl Fn() -> Result<Vec<u8>, VfsError> + Send + Sync + 'static,
        write: impl Fn(&[u8]) -> Result<(), VfsError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: ino(),
            mode,
            reader: Arc::new(read),
            writer: Some(Arc::new(write)),
        })
    }
}
impl Inode for Attribute {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(self.id, InodeKind::Regular, self.mode, 4096))
    }
    fn open(&self, flags: u32) -> Result<Option<Arc<dyn Inode>>, VfsError> {
        if flags & 3 == 0 {
            Ok(Some(Snapshot::new(self.id, (self.reader)()?)))
        } else {
            Ok(None)
        }
    }
    fn read_at(&self, offset: u64, dst: &mut [u8]) -> Result<usize, VfsError> {
        Ok(read_slice(&(self.reader)()?, offset, dst))
    }
    fn write_at(&self, offset: u64, src: &[u8]) -> Result<usize, VfsError> {
        if offset != 0 || src.len() > 4096 {
            return Err(VfsError::InvalidArgument);
        }
        let write = self.writer.as_ref().ok_or(VfsError::PermissionDenied)?;
        write(src)?;
        Ok(src.len())
    }
    fn truncate(&self, _size: u64) -> Result<(), VfsError> {
        Ok(())
    }
}
