/*
 *
 *       src/vfs/fd.rs
 *       Open file descriptions, per-process file descriptor tables and pipes
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Open file descriptions (`OpenFile`), per-task `FileDescriptorTable`, console FDs, and pipes.

use super::{
    abi,
    inode::{DirEntry, Inode, InodeKind, Stat, VfsError},
};
use crate::{
    sync::SpinLock,
    tty::{self, ReadError, TtyDevice},
};
use alloc::{collections::VecDeque, string::String, sync::Arc, vec::Vec};

pub const MAX_FDS: usize = 256;

/// Console character device inode backing `/dev/console` and default FDs 0, 1, 2.
pub struct ConsoleInode;

impl Inode for ConsoleInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let mut s = Stat::simple(2, InodeKind::CharDevice, 0o666, 0);
        s.rdev = (5 << 8) | 1;
        Ok(s)
    }

    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if buf.is_empty() {
            return Ok(0);
        }
        tty::poll_serial();
        match tty::read(TtyDevice::Serial0, buf) {
            Ok(n) if n > 0 => Ok(n),
            _ => match tty::read(TtyDevice::Foreground, buf) {
                Ok(n) => Ok(n),
                Err(ReadError::WouldBlock) => Ok(0),
                Err(_) => Err(VfsError::IoError),
            },
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        tty::write_kernel(buf);
        Ok(buf.len())
    }

    fn ioctl(&self, cmd: u64, _arg: u64) -> Result<u64, VfsError> {
        match cmd {
            abi::TCGETS | abi::TIOCGWINSZ => Ok(0),
            _ => Err(VfsError::InvalidArgument),
        }
    }
}

struct PipeBuffer {
    queue: VecDeque<u8>,
    write_closed: bool,
    read_closed: bool,
}

pub struct PipeEndInode {
    shared: Arc<SpinLock<PipeBuffer>>,
    is_writer: bool,
    ino: u64,
}

impl Inode for PipeEndInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let len = self.shared.lock().queue.len() as u64;
        Ok(Stat::simple(self.ino, InodeKind::Fifo, 0o600, len))
    }

    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if self.is_writer {
            return Err(VfsError::BadFileDescriptor);
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let mut pipe = self.shared.lock();
        if pipe.queue.is_empty() {
            if pipe.write_closed {
                return Ok(0);
            }
            return Err(VfsError::WouldBlock);
        }
        let count = buf.len().min(pipe.queue.len());
        for slot in &mut buf[..count] {
            *slot = pipe.queue.pop_front().unwrap();
        }
        Ok(count)
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if !self.is_writer {
            return Err(VfsError::BadFileDescriptor);
        }
        let mut pipe = self.shared.lock();
        if pipe.read_closed {
            return Err(VfsError::BrokenPipe);
        }
        for &b in buf {
            pipe.queue.push_back(b);
        }
        Ok(buf.len())
    }
}

impl Drop for PipeEndInode {
    fn drop(&mut self) {
        let mut pipe = self.shared.lock();
        if self.is_writer {
            pipe.write_closed = true;
        } else {
            pipe.read_closed = true;
        }
    }
}

pub fn create_pipe_pair() -> (Arc<OpenFile>, Arc<OpenFile>) {
    let shared = Arc::new(SpinLock::new(PipeBuffer {
        queue: VecDeque::with_capacity(4096),
        write_closed: false,
        read_closed: false,
    }));
    let read_inode: Arc<dyn Inode> = Arc::new(PipeEndInode {
        shared: shared.clone(),
        is_writer: false,
        ino: 0x9000_0001,
    });
    let write_inode: Arc<dyn Inode> = Arc::new(PipeEndInode {
        shared,
        is_writer: true,
        ino: 0x9000_0002,
    });
    let reader = Arc::new(OpenFile::new(
        read_inode,
        String::from("pipe:[r]"),
        abi::O_RDONLY,
    ));
    let writer = Arc::new(OpenFile::new(
        write_inode,
        String::from("pipe:[w]"),
        abi::O_WRONLY,
    ));
    (reader, writer)
}

/// Kernel open file description (`struct file` in Linux).
pub struct OpenFile {
    pub inode: Arc<dyn Inode>,
    pub abs_path: String,
    flags: SpinLock<u32>,
    offset: SpinLock<u64>,
    dir_cursor: SpinLock<usize>,
}

impl OpenFile {
    pub fn new(inode: Arc<dyn Inode>, abs_path: String, flags: u32) -> Self {
        Self {
            inode,
            abs_path,
            flags: SpinLock::new(flags),
            offset: SpinLock::new(0),
            dir_cursor: SpinLock::new(0),
        }
    }

    pub fn flags(&self) -> u32 {
        *self.flags.lock()
    }

    pub fn set_flags(&self, new_flags: u32) {
        let mut f = self.flags.lock();
        *f = (*f & abi::O_ACCMODE) | (new_flags & !abi::O_ACCMODE);
    }

    pub fn offset(&self) -> u64 {
        *self.offset.lock()
    }

    pub fn can_read(&self) -> bool {
        let mode = self.flags() & abi::O_ACCMODE;
        mode == abi::O_RDONLY || mode == abi::O_RDWR
    }

    pub fn can_write(&self) -> bool {
        let mode = self.flags() & abi::O_ACCMODE;
        mode == abi::O_WRONLY || mode == abi::O_RDWR
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, VfsError> {
        if !self.can_read() {
            return Err(VfsError::BadFileDescriptor);
        }
        let meta = self.inode.metadata()?;
        if meta.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        let mut off = self.offset.lock();
        let n = self.inode.read_at(*off, buf)?;
        *off = off.saturating_add(n as u64);
        Ok(n)
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize, VfsError> {
        if !self.can_write() {
            return Err(VfsError::BadFileDescriptor);
        }
        let meta = self.inode.metadata()?;
        if meta.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        let mut off = self.offset.lock();
        if (self.flags() & abi::O_APPEND) != 0 {
            *off = meta.size;
        }
        let n = self.inode.write_at(*off, buf)?;
        *off = off.saturating_add(n as u64);
        Ok(n)
    }

    pub fn pread(&self, pos: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if !self.can_read() {
            return Err(VfsError::BadFileDescriptor);
        }
        let meta = self.inode.metadata()?;
        if meta.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        if meta.kind == InodeKind::Fifo {
            return Err(VfsError::IllegalSeek);
        }
        self.inode.read_at(pos, buf)
    }

    pub fn pwrite(&self, pos: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if !self.can_write() {
            return Err(VfsError::BadFileDescriptor);
        }
        let meta = self.inode.metadata()?;
        if meta.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        if meta.kind == InodeKind::Fifo {
            return Err(VfsError::IllegalSeek);
        }
        self.inode.write_at(pos, buf)
    }

    pub fn seek(&self, offset: i64, whence: u32) -> Result<u64, VfsError> {
        let meta = self.inode.metadata()?;
        if meta.kind == InodeKind::Fifo || meta.kind == InodeKind::CharDevice {
            return Err(VfsError::IllegalSeek);
        }
        let mut cur = self.offset.lock();
        let base: i64 = match whence {
            abi::SEEK_SET => 0,
            abi::SEEK_CUR => *cur as i64,
            abi::SEEK_END => meta.size as i64,
            _ => return Err(VfsError::InvalidArgument),
        };
        let target = base.checked_add(offset).ok_or(VfsError::InvalidArgument)?;
        if target < 0 {
            return Err(VfsError::InvalidArgument);
        }
        *cur = target as u64;
        if meta.kind == InodeKind::Directory && target == 0 {
            *self.dir_cursor.lock() = 0;
        }
        Ok(*cur)
    }

    pub fn getdents64(&self, out_buf: &mut [u8]) -> Result<usize, VfsError> {
        let entries: Vec<DirEntry> = super::readdir_with_mounts(&self.abs_path, &self.inode)?;
        let mut cursor = self.dir_cursor.lock();
        if *cursor >= entries.len() {
            return Ok(0);
        }
        let (written, consumed) = abi::encode_dirents64(&entries, *cursor, out_buf);
        if written == 0 && *cursor < entries.len() {
            return Err(VfsError::InvalidArgument);
        }
        *cursor += consumed;
        Ok(written)
    }
}

#[derive(Clone)]
pub struct FdEntry {
    pub file: Arc<OpenFile>,
    pub cloexec: bool,
}

/// Per-process file descriptor table and working directory (`struct files_struct` + `fs_struct`).
#[derive(Clone)]
pub struct FileDescriptorTable {
    entries: Vec<Option<FdEntry>>,
    cwd: String,
}

impl Default for FileDescriptorTable {
    fn default() -> Self {
        Self::new_with_stdio()
    }
}

impl FileDescriptorTable {
    pub fn new_with_stdio() -> Self {
        let console: Arc<dyn Inode> = Arc::new(ConsoleInode);
        let stdin = Arc::new(OpenFile::new(
            console.clone(),
            String::from("/dev/console"),
            abi::O_RDONLY,
        ));
        let stdout = Arc::new(OpenFile::new(
            console.clone(),
            String::from("/dev/console"),
            abi::O_WRONLY,
        ));
        let stderr = Arc::new(OpenFile::new(
            console,
            String::from("/dev/console"),
            abi::O_WRONLY,
        ));

        let mut entries = Vec::with_capacity(16);
        entries.push(Some(FdEntry {
            file: stdin,
            cloexec: false,
        }));
        entries.push(Some(FdEntry {
            file: stdout,
            cloexec: false,
        }));
        entries.push(Some(FdEntry {
            file: stderr,
            cloexec: false,
        }));

        Self {
            entries,
            cwd: String::from("/"),
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn set_cwd(&mut self, abs_path: String) {
        self.cwd = abs_path;
    }

    pub fn get(&self, fd: usize) -> Option<FdEntry> {
        self.entries.get(fd).and_then(|slot| slot.clone())
    }

    pub fn alloc_fd(&mut self, min_fd: usize, entry: FdEntry) -> Result<usize, VfsError> {
        if min_fd >= MAX_FDS {
            return Err(VfsError::InvalidArgument);
        }
        for (idx, slot) in self.entries.iter_mut().enumerate().skip(min_fd) {
            if slot.is_none() {
                *slot = Some(entry);
                return Ok(idx);
            }
        }
        while self.entries.len() < min_fd {
            self.entries.push(None);
        }
        if self.entries.len() >= MAX_FDS {
            return Err(VfsError::TooManyOpenFiles);
        }
        let fd = self.entries.len();
        self.entries.push(Some(entry));
        Ok(fd)
    }

    pub fn insert_at(&mut self, fd: usize, entry: FdEntry) -> Result<usize, VfsError> {
        if fd >= MAX_FDS {
            return Err(VfsError::BadFileDescriptor);
        }
        while self.entries.len() <= fd {
            self.entries.push(None);
        }
        self.entries[fd] = Some(entry);
        Ok(fd)
    }

    pub fn set_cloexec(&mut self, fd: usize, cloexec: bool) -> Result<(), VfsError> {
        let slot = self
            .entries
            .get_mut(fd)
            .and_then(|s| s.as_mut())
            .ok_or(VfsError::BadFileDescriptor)?;
        slot.cloexec = cloexec;
        Ok(())
    }

    pub fn close(&mut self, fd: usize) -> Result<(), VfsError> {
        let slot = self
            .entries
            .get_mut(fd)
            .ok_or(VfsError::BadFileDescriptor)?;
        if slot.take().is_none() {
            return Err(VfsError::BadFileDescriptor);
        }
        Ok(())
    }
}
