/*
 *
 *       src/vfs/inode.rs
 *       Core VFS Inode, Superblock/FileSystem and DirEntry abstractions
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! VFS Inode and FileSystem traits.

use super::abi;
use alloc::{string::String, sync::Arc, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    NotEmpty,
    InvalidArgument,
    BadFileDescriptor,
    PermissionDenied,
    ReadOnlyFs,
    NoSpace,
    IoError,
    NotSupported,
    TooManySymlinks,
    NameTooLong,
    Busy,
    Fault,
    WouldBlock,
    IllegalSeek,
    TooManyOpenFiles,
    NotATty,
    BrokenPipe,
    NoMemory,
    NoDevice,
    TimedOut,
    ExecFormat,
    AddressInUse,
    AddressNotAvailable,
    NotASocket,
    ConnectionRefused,
    ConnectionReset,
    AlreadyConnected,
    NotConnected,
    ProtocolType,
    AddressFamilyNotSupported,
    Interrupted,
    MessageTooLong,
    TooManyReferences,
    NoProtocolOption,
    DestinationRequired,
}

impl VfsError {
    pub fn to_errno(self) -> i64 {
        match self {
            Self::NotFound => -abi::ENOENT,
            Self::AlreadyExists => -abi::EEXIST,
            Self::NotADirectory => -abi::ENOTDIR,
            Self::IsADirectory => -abi::EISDIR,
            Self::NotEmpty => -abi::ENOTEMPTY,
            Self::InvalidArgument => -abi::EINVAL,
            Self::BadFileDescriptor => -abi::EBADF,
            Self::PermissionDenied => -abi::EACCES,
            Self::ReadOnlyFs => -abi::EROFS,
            Self::NoSpace => -abi::ENOSPC,
            Self::IoError => -abi::EIO,
            Self::NotSupported => -95,
            Self::TooManySymlinks => -abi::ELOOP,
            Self::NameTooLong => -abi::ENAMETOOLONG,
            Self::Busy => -abi::EBUSY,
            Self::Fault => -abi::EFAULT,
            Self::WouldBlock => -abi::EAGAIN,
            Self::IllegalSeek => -abi::ESPIPE,
            Self::TooManyOpenFiles => -abi::EMFILE,
            Self::NotATty => -abi::ENOTTY,
            Self::BrokenPipe => -abi::EPIPE,
            Self::NoMemory => -abi::ENOMEM,
            Self::NoDevice => -abi::ENODEV,
            Self::TimedOut => -110,
            Self::ExecFormat => -8,
            Self::AddressInUse => -abi::EADDRINUSE,
            Self::AddressNotAvailable => -abi::EADDRNOTAVAIL,
            Self::NotASocket => -abi::ENOTSOCK,
            Self::ConnectionRefused => -abi::ECONNREFUSED,
            Self::ConnectionReset => -abi::ECONNRESET,
            Self::AlreadyConnected => -abi::EISCONN,
            Self::NotConnected => -abi::ENOTCONN,
            Self::ProtocolType => -abi::EPROTOTYPE,
            Self::AddressFamilyNotSupported => -abi::EAFNOSUPPORT,
            Self::Interrupted => -abi::EINTR,
            Self::MessageTooLong => -abi::EMSGSIZE,
            Self::TooManyReferences => -abi::ETOOMANYREFS,
            Self::NoProtocolOption => -abi::ENOPROTOOPT,
            Self::DestinationRequired => -abi::EDESTADDRREQ,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InodeKind {
    Regular,
    Directory,
    CharDevice,
    BlockDevice,
    Symlink,
    Fifo,
    Socket,
}

#[derive(Clone, Copy, Debug)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub kind: InodeKind,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u64,
    pub blocks: u64,
    pub mtime: i64,
}

impl Stat {
    pub fn simple(ino: u64, kind: InodeKind, mode: u32, size: u64) -> Self {
        Self {
            dev: 1,
            ino,
            kind,
            mode,
            nlink: if kind == InodeKind::Directory { 2 } else { 1 },
            uid: 0,
            gid: 0,
            rdev: 0,
            size,
            blksize: 4096,
            blocks: size.div_ceil(512),
            mtime: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StatFs {
    pub magic: u64,
    pub block_size: u64,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub total_files: u64,
    pub free_files: u64,
    pub max_name_len: u64,
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub ino: u64,
    pub name: String,
    pub kind: InodeKind,
}

pub trait Inode: Send + Sync + core::any::Any {
    fn metadata(&self) -> Result<Stat, VfsError>;

    /// A fresh open-file session; dup() retains the returned session.
    fn open(&self, _flags: u32) -> Result<Option<Arc<dyn Inode>>, VfsError> {
        Ok(None)
    }

    fn deactivate(&self) -> Result<(), VfsError> {
        Ok(())
    }

    fn poll(&self) -> u32 {
        1 | 4
    }

    fn mmap(
        &self,
        _space: &mut crate::user::UserAddressSpace,
        _address: u64,
        _length: u64,
        _offset: u64,
        _prot: u32,
    ) -> Result<(), VfsError> {
        Err(VfsError::NoDevice)
    }

    /// Internal device publication hook. Callers must own the device registry transaction.
    fn remove_node(&self, _name: &str) -> Result<(), VfsError> {
        Err(VfsError::ReadOnlyFs)
    }
    fn insert_node(&self, _name: &str, _node: Arc<dyn Inode>) -> Result<(), VfsError> {
        Err(VfsError::ReadOnlyFs)
    }

    fn chmod(&self, _mode: u32) -> Result<(), VfsError> {
        Err(VfsError::ReadOnlyFs)
    }
    fn chown(&self, _uid: u32, _gid: u32) -> Result<(), VfsError> {
        Err(VfsError::ReadOnlyFs)
    }

    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<usize, VfsError> {
        Err(VfsError::InvalidArgument)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> Result<usize, VfsError> {
        Err(VfsError::InvalidArgument)
    }

    fn truncate(&self, _size: u64) -> Result<(), VfsError> {
        Err(VfsError::InvalidArgument)
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        Err(VfsError::NotADirectory)
    }

    fn create(
        &self,
        _name: &str,
        _kind: InodeKind,
        _mode: u32,
    ) -> Result<Arc<dyn Inode>, VfsError> {
        Err(VfsError::NotSupported)
    }

    fn mkdir(&self, _name: &str, _mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> Result<(), VfsError> {
        Err(VfsError::NotSupported)
    }

    fn symlink(&self, _name: &str, _target: &str) -> Result<Arc<dyn Inode>, VfsError> {
        Err(VfsError::NotSupported)
    }

    fn readlink(&self) -> Result<String, VfsError> {
        Err(VfsError::InvalidArgument)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        Err(VfsError::NotADirectory)
    }

    fn ioctl(&self, _cmd: u64, _arg: u64) -> Result<u64, VfsError> {
        Err(VfsError::NotATty)
    }

    /// Status-flag propagation for `fcntl(F_SETFL)`.
    fn set_nonblock(&self, _nonblock: bool) -> Result<(), VfsError> {
        Ok(())
    }

    fn sync(&self) -> Result<(), VfsError> {
        Ok(())
    }
}

pub trait FileSystem: Send + Sync {
    fn fs_type(&self) -> &'static str;
    fn root_inode(&self) -> Arc<dyn Inode>;
    fn statfs(&self) -> Result<StatFs, VfsError>;
    fn sync(&self) -> Result<(), VfsError> {
        Ok(())
    }
}
