/*
 *       src/ipc/mod.rs
 *       Core IPC objects: eventfd and future Linux-compatible IPC facilities
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

//! Core IPC objects.  The first implemented object is `eventfd2`; AF_UNIX,
//! epoll, signalfd, timerfd and memfd build on the same inode/fd model.

use crate::{
    sched,
    sync::SpinLock,
    vfs::{
        self, FdEntry, Inode, InodeKind, OpenFile, Stat, VfsError,
        abi::{self, O_CLOEXEC, O_NONBLOCK},
    },
};
use alloc::{string::String, sync::Arc};

const EFD_SEMAPHORE: u32 = 1;
const MAX_COUNTER: u64 = u64::MAX - 1;

pub struct EventFd {
    counter: SpinLock<u64>,
    semaphore: bool,
    nonblock: bool,
    ino: u64,
}

impl Inode for EventFd {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(self.ino, InodeKind::CharDevice, 0o600, 0))
    }

    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if buf.len() < 8 {
            return Err(VfsError::InvalidArgument);
        }
        loop {
            let mut counter = self.counter.lock();
            if *counter != 0 {
                let value = if self.semaphore {
                    *counter -= 1;
                    1
                } else {
                    let value = *counter;
                    *counter = 0;
                    value
                };
                drop(counter);
                buf[..8].copy_from_slice(&value.to_le_bytes());
                return Ok(8);
            }
            drop(counter);
            if self.nonblock {
                return Err(VfsError::WouldBlock);
            }
            sched::sleep_micros(1000);
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if buf.len() < 8 {
            return Err(VfsError::InvalidArgument);
        }
        let value = u64::from_le_bytes(buf[..8].try_into().unwrap());
        if value == u64::MAX {
            return Err(VfsError::InvalidArgument);
        }
        loop {
            let mut counter = self.counter.lock();
            if counter
                .checked_add(value)
                .is_some_and(|next| next <= MAX_COUNTER)
            {
                *counter += value;
                return Ok(8);
            }
            drop(counter);
            if self.nonblock {
                return Err(VfsError::WouldBlock);
            }
            sched::sleep_micros(1000);
        }
    }

    fn poll(&self) -> u32 {
        let counter = *self.counter.lock();
        let mut events = 0;
        if counter > 0 {
            events |= 0x01;
        }
        if counter < MAX_COUNTER {
            events |= 0x04;
        }
        events
    }
}

pub fn eventfd2(initval: u32, flags: u32) -> i64 {
    if flags & !(EFD_SEMAPHORE | O_CLOEXEC | O_NONBLOCK) != 0 {
        return -abi::EINVAL;
    }
    let nonblock = flags & O_NONBLOCK != 0;
    let inode = Arc::new(EventFd {
        counter: SpinLock::new(initval as u64),
        semaphore: flags & EFD_SEMAPHORE != 0,
        nonblock,
        ino: vfs::tree::ino(),
    });
    let file = Arc::new(OpenFile::new(
        inode,
        String::from("anon_inode:[eventfd]"),
        abi::O_RDWR | if nonblock { O_NONBLOCK } else { 0 },
    ));
    let fd = match vfs::current_fd_table().lock().alloc_fd(
        0,
        FdEntry {
            file,
            cloexec: flags & O_CLOEXEC != 0,
        },
    ) {
        Ok(fd) => fd,
        Err(error) => return error.to_errno(),
    };
    fd as i64
}
