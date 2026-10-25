/*
 *       src/syscall/memory.rs
 *       Linux memory syscalls with per-process page ownership
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

use crate::{
    user::{
        address_space::{UserAddressSpace, align_up},
        uaccess,
    },
    vfs::{VfsError, abi},
};
const MAP_SHARED: u32 = 1;
const MAP_PRIVATE: u32 = 2;
const MAP_FIXED: u32 = 0x10;
const MAP_ANONYMOUS: u32 = 0x20;
const MAP_FIXED_NOREPLACE: u32 = 0x100000;

pub fn mmap(hint: u64, length: u64, prot: u32, flags: u32, fd: i32, offset: u64) -> i64 {
    let result = (|| -> Result<u64, VfsError> {
        let length = align_up(length)
            .filter(|&n| n != 0 && n <= 256 * 1024 * 1024)
            .ok_or(VfsError::InvalidArgument)?;
        if prot & !7 != 0
            || offset & 4095 != 0
            || flags & 3 == 0
            || flags & 3 == 3
            || flags & !(MAP_SHARED | MAP_PRIVATE | MAP_FIXED | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE)
                != 0
        {
            return Err(VfsError::InvalidArgument);
        }
        let file = if flags & MAP_ANONYMOUS == 0 {
            let f = super::get_open_file(fd)?.file;
            if !f.can_read() {
                return Err(VfsError::PermissionDenied);
            }
            if flags & MAP_SHARED != 0 {
                if prot & 2 != 0 && !f.can_write() {
                    return Err(VfsError::PermissionDenied);
                }
            } else {
                offset
                    .checked_add(length)
                    .ok_or(VfsError::InvalidArgument)?;
            }
            Some(f)
        } else {
            if flags & MAP_SHARED != 0 {
                return Err(VfsError::NotSupported);
            }
            None
        };
        let space = uaccess::current_space()?;
        let mut space = space.lock();
        let fixed = flags & (MAP_FIXED | MAP_FIXED_NOREPLACE) != 0;
        let address = if fixed {
            if hint & 4095 != 0 || !UserAddressSpace::valid_range(hint, length as usize) {
                return Err(VfsError::InvalidArgument);
            }
            if flags & MAP_FIXED_NOREPLACE != 0
                && space
                    .maps()
                    .iter()
                    .any(|&(s, e, _)| hint < e && hint + length > s)
            {
                return Err(VfsError::AlreadyExists);
            }
            if flags & MAP_FIXED != 0 {
                space.unmap(hint, length)?;
            }
            hint
        } else {
            space.find_free(hint, length)?
        };
        if let Some(file) = file {
            if flags & MAP_SHARED != 0 {
                file.inode.mmap(&mut space, address, length, offset, prot)?;
                if !file.can_write() {
                    space.restrict_permissions(address, length, 5)?;
                }
            } else {
                space
                    .map_zeroed(address, length, prot)
                    .map_err(|_| VfsError::NoMemory)?;
                let mut buffer = [0u8; 4096];
                for done in (0..length).step_by(4096) {
                    let source = offset.checked_add(done).ok_or(VfsError::InvalidArgument)?;
                    match file.inode.read_at(source, &mut buffer) {
                        Ok(count) => space.initialize(address + done, &buffer[..count])?,
                        Err(e) => {
                            let _ = space.unmap(address, length);
                            return Err(e);
                        }
                    }
                }
            }
        } else {
            space
                .map_zeroed(address, length, prot)
                .map_err(|_| VfsError::NoMemory)?;
        }
        Ok(address)
    })();
    match result {
        Ok(address) => address as i64,
        Err(e) => e.to_errno(),
    }
}

pub fn munmap(address: u64, length: u64) -> i64 {
    if length == 0 {
        return -abi::EINVAL;
    }
    match uaccess::current_space().and_then(|s| s.lock().unmap(address, length)) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

pub fn mprotect(address: u64, length: u64, prot: u32) -> i64 {
    if length == 0 {
        return 0;
    }
    match uaccess::current_space().and_then(|s| s.lock().protect(address, length, prot)) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}
