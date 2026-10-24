/*
 *
 *       src/syscall/mod.rs
 *       Architecture-independent Linux ABI system call dispatcher
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Linux ABI system call dispatcher integrated with the VFS and scheduler subsystems.

pub mod numbers;

use crate::{
    kinfo, kwarn, sched,
    vfs::{
        self, FdEntry, VfsError,
        abi::{self, IoVec, LinuxStat, LinuxStatFs, WinSize},
    },
};
use alloc::{string::String, vec};
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use numbers::*;

static LAST_USER_EXIT_CODE: AtomicI32 = AtomicI32::new(-1);
static USER_BRK: AtomicU64 = AtomicU64::new(0x0000_0000_2000_0000);

pub fn last_user_exit_code() -> i32 {
    LAST_USER_EXIT_CODE.load(Ordering::Acquire)
}

pub fn set_last_user_exit_code(code: i32) {
    LAST_USER_EXIT_CODE.store(code, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn spacekernel_syscall_c(
    num: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> i64 {
    dispatch(num, arg0, arg1, arg2, arg3, arg4, arg5)
}

pub fn dispatch(
    num: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> i64 {
    match num {
        // Core File I/O
        SYS_READ => sys_read(arg0 as i32, arg1 as *mut u8, arg2 as usize),
        SYS_WRITE => sys_write(arg0 as i32, arg1 as *const u8, arg2 as usize),
        SYS_OPENAT => sys_openat(arg0 as i32, arg1 as *const u8, arg2 as u32, arg3 as u32),
        SYS_CLOSE => sys_close(arg0 as i32),
        SYS_LSEEK => sys_lseek(arg0 as i32, arg1 as i64, arg2 as u32),
        SYS_PREAD64 => sys_pread64(arg0 as i32, arg1 as *mut u8, arg2 as usize, arg3),
        SYS_PWRITE64 => sys_pwrite64(arg0 as i32, arg1 as *const u8, arg2 as usize, arg3),
        SYS_READV => sys_readv(arg0 as i32, arg1 as *const IoVec, arg2 as usize),
        SYS_WRITEV => sys_writev(arg0 as i32, arg1 as *const IoVec, arg2 as usize),

        // Descriptor Management & Pipes
        SYS_DUP => sys_dup(arg0 as i32),
        SYS_DUP3 => sys_dup3(arg0 as i32, arg1 as i32, arg2 as u32),
        SYS_PIPE2 => sys_pipe2(arg0 as *mut i32, arg1 as u32),
        SYS_FCNTL => sys_fcntl(arg0 as i32, arg1 as u32, arg2),
        SYS_IOCTL => sys_ioctl(arg0 as i32, arg1, arg2),

        // Metadata, Directories & Links
        SYS_FSTAT => sys_fstat(arg0 as i32, arg1 as *mut LinuxStat),
        SYS_NEWFSTATAT => sys_newfstatat(
            arg0 as i32,
            arg1 as *const u8,
            arg2 as *mut LinuxStat,
            arg3 as u32,
        ),
        SYS_GETDENTS64 => sys_getdents64(arg0 as i32, arg1 as *mut u8, arg2 as usize),
        SYS_MKDIRAT => sys_mkdirat(arg0 as i32, arg1 as *const u8, arg2 as u32),
        SYS_UNLINKAT => sys_unlinkat(arg0 as i32, arg1 as *const u8, arg2 as u32),
        SYS_SYMLINKAT => sys_symlinkat(arg0 as *const u8, arg1 as i32, arg2 as *const u8),
        SYS_READLINKAT => sys_readlinkat(
            arg0 as i32,
            arg1 as *const u8,
            arg2 as *mut u8,
            arg3 as usize,
        ),
        SYS_FACCESSAT => sys_faccessat(arg0 as i32, arg1 as *const u8, arg2 as u32, arg3 as u32),
        SYS_FTRUNCATE => sys_ftruncate(arg0 as i32, arg1),
        SYS_GETCWD => sys_getcwd(arg0 as *mut u8, arg1 as usize),
        SYS_CHDIR => sys_chdir(arg0 as *const u8),

        // Filesystem & Sync
        SYS_FSYNC => sys_fsync(arg0 as i32),
        SYS_SYNC => {
            vfs::sync_all();
            0
        }
        SYS_STATFS => sys_statfs(arg0 as *const u8, arg1 as *mut LinuxStatFs),
        SYS_FSTATFS => sys_fstatfs(arg0 as i32, arg1 as *mut LinuxStatFs),
        SYS_MOUNT => sys_mount(arg0 as *const u8, arg1 as *const u8, arg2 as *const u8),
        SYS_UMOUNT2 => sys_umount2(arg0 as *const u8, arg1 as u32),

        // Legacy x86_64 non-*at syscalls
        #[cfg(target_arch = "x86_64")]
        SYS_OPEN => sys_openat(abi::AT_FDCWD, arg0 as *const u8, arg1 as u32, arg2 as u32),
        #[cfg(target_arch = "x86_64")]
        SYS_CREAT => sys_openat(
            abi::AT_FDCWD,
            arg0 as *const u8,
            abi::O_CREAT | abi::O_WRONLY | abi::O_TRUNC,
            arg1 as u32,
        ),
        #[cfg(target_arch = "x86_64")]
        SYS_STAT => sys_newfstatat(abi::AT_FDCWD, arg0 as *const u8, arg1 as *mut LinuxStat, 0),
        #[cfg(target_arch = "x86_64")]
        SYS_LSTAT => sys_newfstatat(
            abi::AT_FDCWD,
            arg0 as *const u8,
            arg1 as *mut LinuxStat,
            abi::AT_SYMLINK_NOFOLLOW,
        ),
        #[cfg(target_arch = "x86_64")]
        SYS_ACCESS => sys_faccessat(abi::AT_FDCWD, arg0 as *const u8, arg1 as u32, 0),
        #[cfg(target_arch = "x86_64")]
        SYS_DUP2 => sys_dup2(arg0 as i32, arg1 as i32),
        #[cfg(target_arch = "x86_64")]
        SYS_PIPE => sys_pipe2(arg0 as *mut i32, 0),
        #[cfg(target_arch = "x86_64")]
        SYS_MKDIR => sys_mkdirat(abi::AT_FDCWD, arg0 as *const u8, arg1 as u32),
        #[cfg(target_arch = "x86_64")]
        SYS_RMDIR => sys_unlinkat(abi::AT_FDCWD, arg0 as *const u8, abi::AT_REMOVEDIR),
        #[cfg(target_arch = "x86_64")]
        SYS_UNLINK => sys_unlinkat(abi::AT_FDCWD, arg0 as *const u8, 0),
        #[cfg(target_arch = "x86_64")]
        SYS_SYMLINK => sys_symlinkat(arg0 as *const u8, abi::AT_FDCWD, arg1 as *const u8),
        #[cfg(target_arch = "x86_64")]
        SYS_READLINK => sys_readlinkat(
            abi::AT_FDCWD,
            arg0 as *const u8,
            arg1 as *mut u8,
            arg2 as usize,
        ),

        // Process, Memory & Scheduling
        SYS_GETPID | SYS_GETTID => sys_getpid(),
        SYS_SCHED_YIELD => sys_sched_yield(),
        SYS_NANOSLEEP => sys_nanosleep(arg0),
        SYS_EXIT | SYS_EXIT_GROUP => sys_exit(arg0 as i32),
        SYS_SET_TID_ADDRESS => sys_set_tid_address(arg0),
        SYS_BRK => sys_brk(arg0),
        #[cfg(target_arch = "x86_64")]
        SYS_ARCH_PRCTL => sys_arch_prctl(arg0 as u32, arg1),
        _ => {
            kwarn!("unimplemented syscall: num={}", num);
            -abi::ENOSYS
        }
    }
}

fn read_c_string(ptr: *const u8, max_len: usize) -> Result<String, VfsError> {
    if ptr.is_null() {
        return Err(VfsError::Fault);
    }
    let mut bytes = alloc::vec::Vec::new();
    for i in 0..max_len {
        let b = unsafe { ptr.add(i).read_volatile() };
        if b == 0 {
            return core::str::from_utf8(&bytes)
                .map(String::from)
                .map_err(|_| VfsError::InvalidArgument);
        }
        bytes.push(b);
    }
    Err(VfsError::NameTooLong)
}

fn get_open_file(fd: i32) -> Result<FdEntry, VfsError> {
    if fd < 0 {
        return Err(VfsError::BadFileDescriptor);
    }
    let tbl = vfs::current_fd_table();
    let guard = tbl.lock();
    guard.get(fd as usize).ok_or(VfsError::BadFileDescriptor)
}

fn sys_openat(dirfd: i32, path_ptr: *const u8, flags: u32, mode: u32) -> i64 {
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let file = match vfs::open_abs(&abs_path, flags, mode) {
        Ok(f) => f,
        Err(e) => return e.to_errno(),
    };
    let cloexec = (flags & abi::O_CLOEXEC) != 0;
    let mut guard = tbl.lock();
    match guard.alloc_fd(0, FdEntry { file, cloexec }) {
        Ok(fd) => fd as i64,
        Err(e) => e.to_errno(),
    }
}

fn sys_close(fd: i32) -> i64 {
    if fd < 0 {
        return -abi::EBADF;
    }
    let tbl = vfs::current_fd_table();
    let res = tbl.lock().close(fd as usize);
    match res {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    if count == 0 {
        return 0;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let mut kbuf = vec![0u8; count.min(65536)];
    match entry.file.read(&mut kbuf) {
        Ok(n) => {
            unsafe {
                core::ptr::copy_nonoverlapping(kbuf.as_ptr(), buf, n);
            }
            n as i64
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    if count == 0 {
        return 0;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let limit = count.min(65536);
    let mut kbuf = vec![0u8; limit];
    unsafe {
        core::ptr::copy_nonoverlapping(buf, kbuf.as_mut_ptr(), limit);
    }
    match entry.file.write(&kbuf) {
        Ok(n) => n as i64,
        Err(e) => e.to_errno(),
    }
}

fn sys_lseek(fd: i32, offset: i64, whence: u32) -> i64 {
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    match entry.file.seek(offset, whence) {
        Ok(pos) => pos as i64,
        Err(e) => e.to_errno(),
    }
}

fn sys_pread64(fd: i32, buf: *mut u8, count: usize, pos: u64) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    if count == 0 {
        return 0;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let mut kbuf = vec![0u8; count.min(65536)];
    match entry.file.pread(pos, &mut kbuf) {
        Ok(n) => {
            unsafe {
                core::ptr::copy_nonoverlapping(kbuf.as_ptr(), buf, n);
            }
            n as i64
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_pwrite64(fd: i32, buf: *const u8, count: usize, pos: u64) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    if count == 0 {
        return 0;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let limit = count.min(65536);
    let mut kbuf = vec![0u8; limit];
    unsafe {
        core::ptr::copy_nonoverlapping(buf, kbuf.as_mut_ptr(), limit);
    }
    match entry.file.pwrite(pos, &kbuf) {
        Ok(n) => n as i64,
        Err(e) => e.to_errno(),
    }
}

fn sys_readv(fd: i32, iov_ptr: *const IoVec, iovcnt: usize) -> i64 {
    if iov_ptr.is_null() || iovcnt > 1024 {
        return -abi::EINVAL;
    }
    let mut total = 0i64;
    for i in 0..iovcnt {
        let iov = unsafe { iov_ptr.add(i).read_volatile() };
        if iov.iov_len == 0 {
            continue;
        }
        let n = sys_read(fd, iov.iov_base as *mut u8, iov.iov_len as usize);
        if n < 0 {
            return if total > 0 { total } else { n };
        }
        total += n;
        if (n as u64) < iov.iov_len {
            break;
        }
    }
    total
}

fn sys_writev(fd: i32, iov_ptr: *const IoVec, iovcnt: usize) -> i64 {
    if iov_ptr.is_null() || iovcnt > 1024 {
        return -abi::EINVAL;
    }
    let mut total = 0i64;
    for i in 0..iovcnt {
        let iov = unsafe { iov_ptr.add(i).read_volatile() };
        if iov.iov_len == 0 {
            continue;
        }
        let n = sys_write(fd, iov.iov_base as *const u8, iov.iov_len as usize);
        if n < 0 {
            return if total > 0 { total } else { n };
        }
        total += n;
        if (n as u64) < iov.iov_len {
            break;
        }
    }
    total
}

fn sys_dup(oldfd: i32) -> i64 {
    let entry = match get_open_file(oldfd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let res = tbl.lock().alloc_fd(
        0,
        FdEntry {
            file: entry.file,
            cloexec: false,
        },
    );
    match res {
        Ok(fd) => fd as i64,
        Err(e) => e.to_errno(),
    }
}

#[cfg(target_arch = "x86_64")]
fn sys_dup2(oldfd: i32, newfd: i32) -> i64 {
    if oldfd < 0 || newfd < 0 {
        return -abi::EBADF;
    }
    if oldfd == newfd {
        return match get_open_file(oldfd) {
            Ok(_) => newfd as i64,
            Err(e) => e.to_errno(),
        };
    }
    sys_dup3(oldfd, newfd, 0)
}

fn sys_dup3(oldfd: i32, newfd: i32, flags: u32) -> i64 {
    if oldfd < 0 || newfd < 0 || oldfd == newfd || (flags & !abi::O_CLOEXEC) != 0 {
        return -abi::EINVAL;
    }
    let entry = match get_open_file(oldfd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let cloexec = (flags & abi::O_CLOEXEC) != 0;
    let tbl = vfs::current_fd_table();
    let res = tbl.lock().insert_at(
        newfd as usize,
        FdEntry {
            file: entry.file,
            cloexec,
        },
    );
    match res {
        Ok(fd) => fd as i64,
        Err(e) => e.to_errno(),
    }
}

fn sys_pipe2(pipefd_ptr: *mut i32, flags: u32) -> i64 {
    if pipefd_ptr.is_null() {
        return -abi::EFAULT;
    }
    let cloexec = (flags & abi::O_CLOEXEC) != 0;
    let (reader, writer) = vfs::create_pipe_pair();
    let tbl = vfs::current_fd_table();
    let mut guard = tbl.lock();
    let rfd = match guard.alloc_fd(
        0,
        FdEntry {
            file: reader,
            cloexec,
        },
    ) {
        Ok(fd) => fd,
        Err(e) => return e.to_errno(),
    };
    let wfd = match guard.alloc_fd(
        0,
        FdEntry {
            file: writer,
            cloexec,
        },
    ) {
        Ok(fd) => fd,
        Err(e) => {
            let _ = guard.close(rfd);
            return e.to_errno();
        }
    };
    drop(guard);
    unsafe {
        pipefd_ptr.write_volatile(rfd as i32);
        pipefd_ptr.add(1).write_volatile(wfd as i32);
    }
    0
}

fn sys_fcntl(fd: i32, cmd: u32, arg: u64) -> i64 {
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    match cmd {
        abi::F_DUPFD | abi::F_DUPFD_CLOEXEC => {
            let cloexec = cmd == abi::F_DUPFD_CLOEXEC;
            let res = tbl.lock().alloc_fd(
                arg as usize,
                FdEntry {
                    file: entry.file,
                    cloexec,
                },
            );
            match res {
                Ok(new_fd) => new_fd as i64,
                Err(e) => e.to_errno(),
            }
        }
        abi::F_GETFD => {
            if entry.cloexec {
                abi::FD_CLOEXEC as i64
            } else {
                0
            }
        }
        abi::F_SETFD => {
            let cloexec = (arg as u32 & abi::FD_CLOEXEC) != 0;
            match tbl.lock().set_cloexec(fd as usize, cloexec) {
                Ok(()) => 0,
                Err(e) => e.to_errno(),
            }
        }
        abi::F_GETFL => entry.file.flags() as i64,
        abi::F_SETFL => {
            entry.file.set_flags(arg as u32);
            0
        }
        _ => -abi::EINVAL,
    }
}

fn sys_ioctl(fd: i32, cmd: u64, arg: u64) -> i64 {
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    if cmd == abi::TIOCGWINSZ && arg != 0 {
        let status = crate::tty::status();
        let ws = WinSize {
            ws_row: status.rows.max(25) as u16,
            ws_col: status.columns.max(80) as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            (arg as *mut WinSize).write_volatile(ws);
        }
        return 0;
    }
    match entry.file.inode.ioctl(cmd, arg) {
        Ok(val) => {
            if (cmd == abi::BLKGETSIZE64 || cmd == abi::BLKGETSIZE) && arg != 0 {
                unsafe {
                    (arg as *mut u64).write_volatile(val);
                }
                return 0;
            }
            if cmd == abi::BLKSSZGET && arg != 0 {
                unsafe {
                    (arg as *mut u32).write_volatile(val as u32);
                }
                return 0;
            }
            val as i64
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_fstat(fd: i32, stat_ptr: *mut LinuxStat) -> i64 {
    if stat_ptr.is_null() {
        return -abi::EFAULT;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    match entry.file.inode.metadata() {
        Ok(meta) => {
            let lstat = LinuxStat::from(meta);
            unsafe {
                stat_ptr.write_volatile(lstat);
            }
            0
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_newfstatat(dirfd: i32, path_ptr: *const u8, stat_ptr: *mut LinuxStat, flags: u32) -> i64 {
    if stat_ptr.is_null() {
        return -abi::EFAULT;
    }
    if (flags & abi::AT_EMPTY_PATH) != 0 && !path_ptr.is_null() {
        let b0 = unsafe { path_ptr.read_volatile() };
        if b0 == 0 {
            return sys_fstat(dirfd, stat_ptr);
        }
    }
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let follow = (flags & abi::AT_SYMLINK_NOFOLLOW) == 0;
    let (inode, _) = match vfs::lookup_abs(&abs_path, follow) {
        Ok(r) => r,
        Err(e) => return e.to_errno(),
    };
    match inode.metadata() {
        Ok(meta) => {
            let lstat = LinuxStat::from(meta);
            unsafe {
                stat_ptr.write_volatile(lstat);
            }
            0
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_getdents64(fd: i32, dirp: *mut u8, count: usize) -> i64 {
    if dirp.is_null() {
        return -abi::EFAULT;
    }
    if count < 32 {
        return -abi::EINVAL;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let mut kbuf = vec![0u8; count.min(32768)];
    match entry.file.getdents64(&mut kbuf) {
        Ok(written) => {
            if written > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(kbuf.as_ptr(), dirp, written);
                }
            }
            written as i64
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_mkdirat(dirfd: i32, path_ptr: *const u8, mode: u32) -> i64 {
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    match vfs::mkdir_abs(&abs_path, mode) {
        Ok(_) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_unlinkat(dirfd: i32, path_ptr: *const u8, flags: u32) -> i64 {
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let remove_dir = (flags & abi::AT_REMOVEDIR) != 0;
    match vfs::unlink_abs(&abs_path, remove_dir) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_symlinkat(target_ptr: *const u8, newdirfd: i32, linkpath_ptr: *const u8) -> i64 {
    let target = match read_c_string(target_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let link_str = match read_c_string(linkpath_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_link = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, newdirfd, &link_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    match vfs::symlink_abs(&target, &abs_link) {
        Ok(_) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_readlinkat(dirfd: i32, path_ptr: *const u8, buf: *mut u8, bufsiz: usize) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    if bufsiz == 0 {
        return -abi::EINVAL;
    }
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    match vfs::readlink_abs(&abs_path) {
        Ok(target) => {
            let bytes = target.as_bytes();
            let copy_len = bytes.len().min(bufsiz);
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, copy_len);
            }
            copy_len as i64
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_faccessat(dirfd: i32, path_ptr: *const u8, _mode: u32, flags: u32) -> i64 {
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, dirfd, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let follow = (flags & abi::AT_SYMLINK_NOFOLLOW) == 0;
    match vfs::lookup_abs(&abs_path, follow) {
        Ok(_) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_ftruncate(fd: i32, length: u64) -> i64 {
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    if !entry.file.can_write() {
        return -abi::EINVAL;
    }
    match entry.file.inode.truncate(length) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_getcwd(buf: *mut u8, size: usize) -> i64 {
    if buf.is_null() {
        return -abi::EFAULT;
    }
    let tbl = vfs::current_fd_table();
    let cwd = String::from(tbl.lock().cwd());
    let bytes = cwd.as_bytes();
    if bytes.len() + 1 > size {
        return -abi::ERANGE;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
        buf.add(bytes.len()).write_volatile(0);
    }
    (bytes.len() + 1) as i64
}

fn sys_chdir(path_ptr: *const u8) -> i64 {
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, abi::AT_FDCWD, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let (inode, final_path) = match vfs::lookup_abs(&abs_path, true) {
        Ok(r) => r,
        Err(e) => return e.to_errno(),
    };
    match inode.metadata() {
        Ok(meta) if meta.kind == vfs::InodeKind::Directory => {
            tbl.lock().set_cwd(final_path);
            0
        }
        Ok(_) => -abi::ENOTDIR,
        Err(e) => e.to_errno(),
    }
}

fn sys_fsync(fd: i32) -> i64 {
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    match entry.file.inode.sync() {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_statfs(path_ptr: *const u8, buf_ptr: *mut LinuxStatFs) -> i64 {
    if buf_ptr.is_null() {
        return -abi::EFAULT;
    }
    let path_str = match read_c_string(path_ptr, 512) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let tbl = vfs::current_fd_table();
    let abs_path = {
        let guard = tbl.lock();
        match vfs::resolve_at_path(&guard, abi::AT_FDCWD, &path_str) {
            Ok(p) => p,
            Err(e) => return e.to_errno(),
        }
    };
    let (mount, _) = match vfs::find_mount(&abs_path) {
        Ok(m) => m,
        Err(e) => return e.to_errno(),
    };
    match mount.fs.statfs() {
        Ok(sfs) => {
            unsafe {
                buf_ptr.write_volatile(LinuxStatFs::from(sfs));
            }
            0
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_fstatfs(fd: i32, buf_ptr: *mut LinuxStatFs) -> i64 {
    if buf_ptr.is_null() {
        return -abi::EFAULT;
    }
    let entry = match get_open_file(fd) {
        Ok(e) => e,
        Err(e) => return e.to_errno(),
    };
    let (mount, _) = match vfs::find_mount(&entry.file.abs_path) {
        Ok(m) => m,
        Err(e) => return e.to_errno(),
    };
    match mount.fs.statfs() {
        Ok(sfs) => {
            unsafe {
                buf_ptr.write_volatile(LinuxStatFs::from(sfs));
            }
            0
        }
        Err(e) => e.to_errno(),
    }
}

fn sys_mount(source_ptr: *const u8, target_ptr: *const u8, fstype_ptr: *const u8) -> i64 {
    let source = read_c_string(source_ptr, 256).unwrap_or_else(|_| String::from("none"));
    let target = match read_c_string(target_ptr, 256) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    let fstype = match read_c_string(fstype_ptr, 64) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    match vfs::mount_by_type(&source, &target, &fstype) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_umount2(target_ptr: *const u8, _flags: u32) -> i64 {
    let target = match read_c_string(target_ptr, 256) {
        Ok(s) => s,
        Err(e) => return e.to_errno(),
    };
    match vfs::umount(&target) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}

fn sys_getpid() -> i64 {
    sched::current().map(|t| t.stats().tid).unwrap_or(1) as i64
}

fn sys_sched_yield() -> i64 {
    sched::yield_now();
    0
}

fn sys_nanosleep(req_ptr: u64) -> i64 {
    if req_ptr == 0 {
        return -abi::EFAULT;
    }
    let timespec = req_ptr as *const [u64; 2];
    let (sec, nsec) = unsafe {
        let ts = *timespec;
        (ts[0], ts[1])
    };
    let micros = sec.saturating_mul(1_000_000).saturating_add(nsec / 1_000);
    sched::sleep_micros(micros);
    0
}

fn sys_exit(code: i32) -> i64 {
    kinfo!("Linux ABI: user process exited with code {}", code);
    set_last_user_exit_code(code);
    sched::exit(code)
}

fn sys_set_tid_address(_tidptr: u64) -> i64 {
    sys_getpid()
}

fn sys_brk(addr: u64) -> i64 {
    let current = USER_BRK.load(Ordering::Acquire);
    if addr == 0 || addr <= current {
        return current as i64;
    }
    let new_brk = (addr + 4095) & !4095;
    USER_BRK.store(new_brk, Ordering::Release);
    new_brk as i64
}

#[cfg(target_arch = "x86_64")]
fn sys_arch_prctl(code: u32, addr: u64) -> i64 {
    const ARCH_SET_FS: u32 = 0x1002;
    const ARCH_GET_FS: u32 = 0x1003;
    const IA32_FS_BASE: u32 = 0xc000_0100;

    match code {
        ARCH_SET_FS => {
            unsafe {
                core::arch::asm!(
                    "wrmsr",
                    in("ecx") IA32_FS_BASE,
                    in("eax") (addr as u32),
                    in("edx") ((addr >> 32) as u32),
                    options(nostack)
                );
            }
            0
        }
        ARCH_GET_FS => {
            let (low, high): (u32, u32);
            unsafe {
                core::arch::asm!(
                    "rdmsr",
                    in("ecx") IA32_FS_BASE,
                    out("eax") low,
                    out("edx") high,
                    options(nomem, nostack)
                );
            }
            let val = ((high as u64) << 32) | (low as u64);
            if addr != 0 {
                unsafe {
                    *(addr as *mut u64) = val;
                }
            }
            0
        }
        _ => -abi::EINVAL,
    }
}

#[cfg(feature = "boot-self-test")]
pub fn run_vfs_syscall_self_test() {
    // Exercise Linux ABI syscalls directly through `dispatch`
    let path = b"/mnt/syscall_test.txt\0";
    let fd = dispatch(
        SYS_OPENAT,
        abi::AT_FDCWD as i64 as u64,
        path.as_ptr() as u64,
        (abi::O_CREAT | abi::O_RDWR | abi::O_TRUNC) as u64,
        0o644,
        0,
        0,
    );
    assert!(fd >= 3, "syscall SYS_OPENAT failed: {}", fd);

    let payload = b"linux-abi-vfs-syscall-verified";
    let written = dispatch(
        SYS_WRITE,
        fd as u64,
        payload.as_ptr() as u64,
        payload.len() as u64,
        0,
        0,
        0,
    );
    assert_eq!(written, payload.len() as i64);

    let mut st = LinuxStat::default();
    let fstat_rc = dispatch(
        SYS_FSTAT,
        fd as u64,
        (&mut st as *mut LinuxStat) as u64,
        0,
        0,
        0,
        0,
    );
    assert_eq!(fstat_rc, 0);
    assert_eq!(st.st_size, payload.len() as i64);

    let pos = dispatch(SYS_LSEEK, fd as u64, 0, abi::SEEK_SET as u64, 0, 0, 0);
    assert_eq!(pos, 0);

    let mut read_buf = [0u8; 64];
    let read_n = dispatch(
        SYS_READ,
        fd as u64,
        read_buf.as_mut_ptr() as u64,
        payload.len() as u64,
        0,
        0,
        0,
    );
    assert_eq!(read_n, payload.len() as i64);
    assert_eq!(&read_buf[..payload.len()], payload);

    // Test getdents64 on /mnt
    let mnt_path = b"/mnt\0";
    let dir_fd = dispatch(
        SYS_OPENAT,
        abi::AT_FDCWD as i64 as u64,
        mnt_path.as_ptr() as u64,
        (abi::O_RDONLY | abi::O_DIRECTORY_GENERIC) as u64,
        0,
        0,
        0,
    );
    assert!(dir_fd >= 3);
    let mut dirent_buf = [0u8; 512];
    let dents_n = dispatch(
        SYS_GETDENTS64,
        dir_fd as u64,
        dirent_buf.as_mut_ptr() as u64,
        dirent_buf.len() as u64,
        0,
        0,
        0,
    );
    assert!(dents_n > 0, "SYS_GETDENTS64 returned {}", dents_n);
    assert_eq!(dispatch(SYS_CLOSE, dir_fd as u64, 0, 0, 0, 0, 0), 0);

    // Test pipe2, dup, close, unlinkat
    let mut pipefds = [0i32; 2];
    assert_eq!(
        dispatch(SYS_PIPE2, pipefds.as_mut_ptr() as u64, 0, 0, 0, 0, 0),
        0
    );
    let msg = b"pipe-ok";
    assert_eq!(
        dispatch(
            SYS_WRITE,
            pipefds[1] as u64,
            msg.as_ptr() as u64,
            msg.len() as u64,
            0,
            0,
            0
        ),
        msg.len() as i64
    );
    let mut pbuf = [0u8; 16];
    assert_eq!(
        dispatch(
            SYS_READ,
            pipefds[0] as u64,
            pbuf.as_mut_ptr() as u64,
            msg.len() as u64,
            0,
            0,
            0
        ),
        msg.len() as i64
    );
    assert_eq!(&pbuf[..msg.len()], msg);
    assert_eq!(dispatch(SYS_CLOSE, pipefds[0] as u64, 0, 0, 0, 0, 0), 0);
    assert_eq!(dispatch(SYS_CLOSE, pipefds[1] as u64, 0, 0, 0, 0, 0), 0);

    assert_eq!(dispatch(SYS_FSYNC, fd as u64, 0, 0, 0, 0, 0), 0);
    assert_eq!(dispatch(SYS_CLOSE, fd as u64, 0, 0, 0, 0, 0), 0);
    assert_eq!(
        dispatch(
            SYS_UNLINKAT,
            abi::AT_FDCWD as i64 as u64,
            path.as_ptr() as u64,
            0,
            0,
            0,
            0,
        ),
        0
    );

    kinfo!("Linux ABI VFS syscall self-test passed");
}
