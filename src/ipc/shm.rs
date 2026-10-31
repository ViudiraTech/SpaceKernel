/*
 *
 *       src/ipc/shm.rs
 *       System V shared memory segments (shmget, shmat, shmdt, shmctl)
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Each segment owns a memfd, so its pages stay shared across fork and exec
//! boundaries. X11's MIT-SHM relies on two details implemented here: a segment
//! marked `IPC_RMID` stays usable until the last attachment leaves, and root may
//! attach any segment regardless of mode.

use super::memfd::MemFd;
use crate::{
    sync::SpinLock,
    user::{
        address_space::{PROT_EXEC, PROT_READ, PROT_WRITE, align_up},
        uaccess,
    },
    vfs::{Inode, VfsError, abi},
};
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};

const IPC_CREAT: u32 = 0o1000;
const IPC_EXCL: u32 = 0o2000;
const IPC_PRIVATE: i32 = 0;
const IPC_RMID: u32 = 0;
const IPC_SET: u32 = 1;
const IPC_STAT: u32 = 2;
const IPC_INFO: u32 = 3;
const SHM_STAT: u32 = 13;
const SHM_INFO: u32 = 14;
const SHM_STAT_ANY: u32 = 15;
const SHM_LOCK: u32 = 11;
const SHM_UNLOCK: u32 = 12;
const SHM_RDONLY: u32 = 0o10000;
const SHM_RND: u32 = 0o20000;
const SHM_EXEC: u32 = 0o100000;
const SHM_REMAP: u32 = 0o40000;
const SHM_DEST: u16 = 0o1000;
const SHMLBA: u64 = 4096;
const SHMMAX: u64 = 16 * 1024 * 1024 * 1024;
const SHMMNI: u32 = 4096;

struct Segment {
    key: i32,
    memory: Arc<MemFd>,
    size: u64,
    uid: u32,
    gid: u32,
    cuid: u32,
    cgid: u32,
    mode: u16,
    attachments: u32,
    removed: bool,
    atime: i64,
    dtime: i64,
    ctime: i64,
    lpid: u32,
    cpid: u32,
    sequence: u16,
}

struct Registry {
    segments: BTreeMap<i32, Segment>,
    /// `(address space root, attach address) -> (shmid, length)` for `shmdt`.
    attached: BTreeMap<(u64, u64), (i32, u64)>,
    next_index: u32,
    sequence: u16,
}

static REGISTRY: SpinLock<Registry> = SpinLock::new(Registry {
    segments: BTreeMap::new(),
    attached: BTreeMap::new(),
    next_index: 0,
    sequence: 0,
});

fn now() -> i64 {
    (crate::time::realtime_micros() / 1_000_000) as i64
}

fn creds() -> crate::sched::Credentials {
    crate::sched::current()
        .map(|task| task.credentials())
        .unwrap_or_default()
}

fn tgid() -> u32 {
    crate::sched::current().map_or(0, |task| task.stats().tgid as u32)
}

fn allowed(segment: &Segment, wanted: u16) -> bool {
    let id = creds();
    if id.euid == 0 {
        return true;
    }
    let bits = if id.euid == segment.uid || id.euid == segment.cuid {
        segment.mode >> 6
    } else if id.egid == segment.gid || id.egid == segment.cgid {
        segment.mode >> 3
    } else {
        segment.mode
    };
    bits & wanted & 7 == wanted & 7
}

pub fn shmget(key: i32, size: u64, flags: u32) -> i64 {
    let mode = (flags & 0o777) as u16;
    let registry = REGISTRY.lock();
    if key != IPC_PRIVATE
        && let Some((&id, segment)) = registry
            .segments
            .iter()
            .find(|(_, segment)| segment.key == key && !segment.removed)
    {
        if flags & (IPC_CREAT | IPC_EXCL) == IPC_CREAT | IPC_EXCL {
            return -abi::EEXIST;
        }
        if size > segment.size {
            return -abi::EINVAL;
        }
        if !allowed(segment, (flags & 0o7) as u16) && !allowed(segment, mode >> 6) {
            return -abi::EACCES;
        }
        return i64::from(id);
    }
    if key != IPC_PRIVATE && flags & IPC_CREAT == 0 {
        return -abi::ENOENT;
    }
    if size == 0 || size > SHMMAX {
        return -abi::EINVAL;
    }
    if registry.segments.len() as u32 >= SHMMNI {
        return -abi::ENOSPC;
    }
    drop(registry);
    let Some(rounded) = align_up(size) else {
        return -abi::EINVAL;
    };
    let memory = match MemFd::create(String::from("SYSV"), 0) {
        Ok((memory, _)) => memory,
        Err(error) => return error.to_errno(),
    };
    if let Err(error) = Inode::truncate(memory.as_ref(), rounded) {
        return error.to_errno();
    }
    let id = creds();
    let mut registry = REGISTRY.lock();
    registry.sequence = registry.sequence.wrapping_add(1);
    let index = registry.next_index % SHMMNI;
    registry.next_index = registry.next_index.wrapping_add(1);
    let sequence = registry.sequence;
    let shmid = ((u32::from(sequence) << 12) | index) as i32 & 0x7FFF_FFFF;
    registry.segments.insert(
        shmid,
        Segment {
            key,
            memory,
            size,
            uid: id.euid,
            gid: id.egid,
            cuid: id.euid,
            cgid: id.egid,
            mode,
            attachments: 0,
            removed: false,
            atime: 0,
            dtime: 0,
            ctime: now(),
            lpid: 0,
            cpid: tgid(),
            sequence,
        },
    );
    i64::from(shmid)
}

pub fn shmat(shmid: i32, address: u64, flags: u32) -> i64 {
    if flags & !(SHM_RDONLY | SHM_RND | SHM_EXEC | SHM_REMAP) != 0 {
        return -abi::EINVAL;
    }
    let mut prot = PROT_READ;
    if flags & SHM_RDONLY == 0 {
        prot |= PROT_WRITE;
    }
    if flags & SHM_EXEC != 0 {
        prot |= PROT_EXEC;
    }
    let (memory, length) = {
        let mut registry = REGISTRY.lock();
        let Some(segment) = registry.segments.get_mut(&shmid) else {
            return -abi::EINVAL;
        };
        let wanted = 4 | if flags & SHM_RDONLY == 0 { 2 } else { 0 };
        if !allowed(segment, wanted) {
            return -abi::EACCES;
        }
        segment.attachments += 1;
        (segment.memory.clone(), align_up(segment.size).unwrap_or(0))
    };
    let result = (|| -> Result<(u64, u64), VfsError> {
        let space = uaccess::current_space()?;
        let mut space = space.lock();
        let target = if address == 0 {
            space.find_free(0, length).map_err(|_| VfsError::NoMemory)?
        } else {
            let aligned = if flags & SHM_RND != 0 {
                address & !(SHMLBA - 1)
            } else {
                address
            };
            if aligned & (SHMLBA - 1) != 0 {
                return Err(VfsError::InvalidArgument);
            }
            if space.range_in_use(aligned, aligned + length) {
                if flags & SHM_REMAP == 0 {
                    return Err(VfsError::InvalidArgument);
                }
                space.unmap(aligned, length)?;
            }
            aligned
        };
        Inode::mmap(memory.as_ref(), &mut space, target, length, 0, prot)?;
        Ok((target, space.root_physical()))
    })();
    let mut registry = REGISTRY.lock();
    match result {
        Ok((target, root)) => {
            registry.attached.insert((root, target), (shmid, length));
            if let Some(segment) = registry.segments.get_mut(&shmid) {
                segment.atime = now();
                segment.lpid = tgid();
            }
            target as i64
        }
        Err(error) => {
            release_attachment(&mut registry, shmid);
            error.to_errno()
        }
    }
}

fn release_attachment(registry: &mut Registry, shmid: i32) {
    let destroy = if let Some(segment) = registry.segments.get_mut(&shmid) {
        segment.attachments = segment.attachments.saturating_sub(1);
        segment.dtime = now();
        segment.lpid = tgid();
        segment.removed && segment.attachments == 0
    } else {
        false
    };
    if destroy {
        registry.segments.remove(&shmid);
    }
}

pub fn shmdt(address: u64) -> i64 {
    let space = match uaccess::current_space() {
        Ok(space) => space,
        Err(error) => return error.to_errno(),
    };
    let mut space = space.lock();
    let root = space.root_physical();
    let mut registry = REGISTRY.lock();
    let Some((shmid, length)) = registry.attached.remove(&(root, address)) else {
        return -abi::EINVAL;
    };
    let _ = space.unmap(address, length);
    release_attachment(&mut registry, shmid);
    0
}

/// `struct shmid64_ds` on the 64-bit Linux ABIs, 112 bytes.
fn encode_ds(segment: &Segment, shmid: i32) -> [u8; 112] {
    let mut raw = [0u8; 112];
    let mut put32 = |offset: usize, value: u32| {
        raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    put32(0, segment.key as u32);
    put32(4, segment.uid);
    put32(8, segment.gid);
    put32(12, segment.cuid);
    put32(16, segment.cgid);
    let mode = segment.mode | if segment.removed { SHM_DEST } else { 0 };
    put32(20, u32::from(mode));
    put32(24, u32::from(segment.sequence));
    let _ = shmid;
    raw[48..56].copy_from_slice(&segment.size.to_le_bytes());
    raw[56..64].copy_from_slice(&segment.atime.to_le_bytes());
    raw[64..72].copy_from_slice(&segment.dtime.to_le_bytes());
    raw[72..80].copy_from_slice(&segment.ctime.to_le_bytes());
    raw[80..84].copy_from_slice(&segment.cpid.to_le_bytes());
    raw[84..88].copy_from_slice(&segment.lpid.to_le_bytes());
    raw[88..96].copy_from_slice(&u64::from(segment.attachments).to_le_bytes());
    raw
}

pub fn shmctl(shmid: i32, command: u32, buffer: u64) -> i64 {
    let command = command & !0o400; // IPC_64 is the only layout on 64-bit.
    let mut registry = REGISTRY.lock();
    match command {
        IPC_INFO | SHM_INFO => {
            let highest = registry
                .segments
                .keys()
                .copied()
                .max()
                .map_or(0, |id| (id as u32) & 0xFFF);
            let output: Vec<u8> = if command == IPC_INFO {
                [SHMMAX, SHMMAX, 1, u64::from(SHMMNI), 4096, 1 << 40]
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .chain([0u8; 32])
                    .collect()
            } else {
                let pages: u64 = registry
                    .segments
                    .values()
                    .map(|segment| segment.size.div_ceil(4096))
                    .sum();
                let mut raw = Vec::with_capacity(48);
                raw.extend_from_slice(&(registry.segments.len() as u32).to_le_bytes());
                raw.extend_from_slice(&[0u8; 4]);
                raw.extend_from_slice(&pages.to_le_bytes());
                raw.extend_from_slice(&pages.to_le_bytes());
                raw.extend_from_slice(&[0u8; 24]);
                raw
            };
            drop(registry);
            return match uaccess::write(buffer, &output) {
                Ok(()) => i64::from(highest),
                Err(error) => error.to_errno(),
            };
        }
        _ => {}
    }
    let Some(segment) = registry.segments.get_mut(&shmid) else {
        return -abi::EINVAL;
    };
    let id = creds();
    let owner = id.euid == 0 || id.euid == segment.uid || id.euid == segment.cuid;
    match command {
        IPC_STAT | SHM_STAT | SHM_STAT_ANY => {
            if command != SHM_STAT_ANY && !allowed(segment, 4) {
                return -abi::EACCES;
            }
            let raw = encode_ds(segment, shmid);
            drop(registry);
            match uaccess::write(buffer, &raw) {
                Ok(()) => i64::from(if command == IPC_STAT { 0 } else { shmid }),
                Err(error) => error.to_errno(),
            }
        }
        IPC_SET => {
            if !owner {
                return -abi::EPERM;
            }
            let (uid, gid, mode) = {
                drop(registry);
                let raw: [u32; 6] = match uaccess::read_value(buffer) {
                    Ok(raw) => raw,
                    Err(error) => return error.to_errno(),
                };
                (raw[1], raw[2], raw[5])
            };
            let mut registry = REGISTRY.lock();
            let Some(segment) = registry.segments.get_mut(&shmid) else {
                return -abi::EINVAL;
            };
            segment.uid = uid;
            segment.gid = gid;
            segment.mode = (mode & 0o777) as u16;
            segment.ctime = now();
            0
        }
        IPC_RMID => {
            if !owner {
                return -abi::EPERM;
            }
            segment.removed = true;
            segment.ctime = now();
            if segment.attachments == 0 {
                registry.segments.remove(&shmid);
            }
            0
        }
        SHM_LOCK | SHM_UNLOCK => {
            if owner {
                0
            } else {
                -abi::EPERM
            }
        }
        _ => -abi::EINVAL,
    }
}

/// Keep attachment counts right when a process image disappears without shmdt.
pub fn release_address_space(root: u64) {
    let mut registry = REGISTRY.lock();
    let keys: Vec<(u64, u64)> = registry
        .attached
        .range((root, 0)..=(root, u64::MAX))
        .map(|(&key, _)| key)
        .collect();
    for key in keys {
        if let Some((shmid, _)) = registry.attached.remove(&key) {
            release_attachment(&mut registry, shmid);
        }
    }
}
