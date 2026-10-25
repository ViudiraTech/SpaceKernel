/*
 *       src/syscall/process.rs
 *       Transactional execve, polling, clocks and random ABI boundaries
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use crate::{
    arch, sched,
    sync::SpinLock,
    user::{UserAddressSpace, elf, stack, uaccess},
    vfs::{self, VfsError, abi},
};
use alloc::{string::String, sync::Arc, vec::Vec};
fn strings(pointer: u64) -> Result<Vec<String>, VfsError> {
    let mut result = Vec::new();
    let mut size = 0;
    if pointer == 0 {
        return Ok(result);
    }
    for i in 0..4096 {
        let address = pointer.checked_add(i * 8).ok_or(VfsError::Fault)?;
        let string = uaccess::read_value::<u64>(address)?;
        if string == 0 {
            return Ok(result);
        }
        let value = uaccess::string(string, 65536)?;
        size += value.len() + 1;
        if size > 128 * 1024 {
            return Err(VfsError::NoSpace);
        }
        result.push(value);
    }
    Err(VfsError::NoSpace)
}
pub fn execve(path: u64, argv: u64, envp: u64) -> i64 {
    let prepared = (|| -> Result<_, VfsError> {
        let path = uaccess::string(path, 4096)?;
        let cwd = String::from(vfs::current_fd_table().lock().cwd());
        let path = vfs::normalize_path(&cwd, &path);
        let (inode, _) = vfs::lookup_abs(&path, true)?;
        let meta = inode.metadata()?;
        if meta.kind != vfs::InodeKind::Regular || meta.mode & 0o111 == 0 {
            return Err(VfsError::PermissionDenied);
        }
        let argv = strings(argv)?;
        let env = strings(envp)?;
        let data = vfs::read_file(&path)?;
        let mut space = UserAddressSpace::new().map_err(|_| VfsError::NoMemory)?;
        let loaded = elf::load_elf(&mut space, &data).map_err(|_| VfsError::ExecFormat)?;
        let args: Vec<_> = argv.iter().map(|s| s.as_str()).collect();
        let environment: Vec<_> = env.iter().map(|s| s.as_str()).collect();
        let sp = stack::setup(&mut space, &loaded, &path, &args, &environment)
            .map_err(|_| VfsError::NoSpace)?;
        Ok((
            Arc::new(SpinLock::new(space)),
            loaded.initial_pc,
            sp,
            loaded.tls_tp,
        ))
    })();
    let (space, pc, sp, tls_tp) = match prepared {
        Ok(p) => p,
        Err(e) => return e.to_errno(),
    };
    let task = match sched::current() {
        Some(t) => t,
        None => return -abi::ESRCH,
    };
    let old = task.address_space();
    // No fallible operation remains after descriptor/image publication.
    vfs::current_fd_table().lock().close_on_exec();
    arch::disable_interrupts();
    {
        let mut t = task.lock();
        t.set_address_space(space.clone());
        if let Some(top) = t.kernel_stack_top() {
            arch::set_kernel_stack(crate::smp::current_cpu(), top as u64);
        }
        // SAFETY: this CPU exclusively owns the running task, IRQs are disabled.
        unsafe {
            if let Some(state) = (&mut *t.fpu_state.get()).as_mut() {
                state.reset().expect("valid task FPU reset");
                crate::fpu::restore_current(state).expect("valid task FPU restore");
            }
        }
    }
    space.lock().activate();
    drop(old);
    unsafe {
        crate::user::process::install_tls(tls_tp);
        let _ = crate::fpu::enable_user();
        arch::enter_user_mode(pc, sp)
    }
}
pub fn poll(pointer: u64, count: u64, timeout_ms: i64) -> i64 {
    let result = (|| -> Result<usize, VfsError> {
        let count = usize::try_from(count).map_err(|_| VfsError::InvalidArgument)?;
        if count > 1024 {
            return Err(VfsError::InvalidArgument);
        }
        let mut bytes = uaccess::read_vec(pointer, count * 8, 8192)?;
        uaccess::check(pointer, bytes.len(), true)?;
        let deadline = if timeout_ms < 0 {
            u64::MAX
        } else {
            crate::time::uptime_micros().saturating_add((timeout_ms as u64).saturating_mul(1000))
        };
        loop {
            let mut ready = 0;
            for item in bytes.chunks_exact_mut(8) {
                let fd = i32::from_le_bytes(item[..4].try_into().unwrap());
                let requested = u16::from_le_bytes(item[4..6].try_into().unwrap()) as u32;
                let available = if fd < 0 {
                    0
                } else {
                    match vfs::current_fd_table().lock().get(fd as usize) {
                        Some(e) => e.file.inode.poll(),
                        None => 0x20,
                    }
                };
                let events = available & (requested | 8 | 16 | 0x20);
                item[6..8].copy_from_slice(&(events as u16).to_le_bytes());
                ready += usize::from(events != 0);
            }
            if ready != 0 || crate::time::uptime_micros() >= deadline {
                uaccess::write(pointer, &bytes)?;
                return Ok(ready);
            }
            sched::sleep_micros(1000);
        }
    })();
    match result {
        Ok(n) => n as i64,
        Err(e) => e.to_errno(),
    }
}
pub fn ppoll(pointer: u64, count: u64, timeout: u64, sigmask: u64) -> i64 {
    if sigmask != 0 {
        return -abi::ENOSYS;
    }
    let millis = if timeout == 0 {
        -1
    } else {
        let ts = match uaccess::read_value::<[u64; 2]>(timeout) {
            Ok(t) => t,
            Err(e) => return e.to_errno(),
        };
        if ts[0] > i64::MAX as u64 || ts[1] >= 1_000_000_000 {
            return -abi::EINVAL;
        }
        ts[0]
            .saturating_mul(1000)
            .saturating_add(ts[1].div_ceil(1_000_000))
            .min(i64::MAX as u64) as i64
    };
    poll(pointer, count, millis)
}
pub fn clock_gettime(clock: u32, pointer: u64) -> i64 {
    if !matches!(clock, 1 | 4 | 7) {
        return -abi::EINVAL;
    }
    let us = crate::time::uptime_micros();
    match uaccess::write_value(pointer, &[us / 1_000_000, us % 1_000_000 * 1000]) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}
pub fn getrandom(pointer: u64, count: usize, flags: u32) -> i64 {
    if flags & !3 != 0 {
        return -abi::EINVAL;
    }
    let length = count.min(65536);
    if let Err(e) = uaccess::check(pointer, length, true) {
        return e.to_errno();
    }
    if length == 0 {
        return 0;
    }
    if !crate::random::supported() {
        return VfsError::WouldBlock.to_errno();
    }
    let mut bytes = alloc::vec![0; length];
    for _ in 0..64 {
        match crate::random::fill(&mut bytes) {
            Ok(()) => {
                return match uaccess::write(pointer, &bytes) {
                    Ok(()) => length as i64,
                    Err(e) => e.to_errno(),
                };
            }
            Err(VfsError::WouldBlock) if flags & 1 == 0 => sched::sleep_micros(1000),
            Err(e) => return e.to_errno(),
        }
    }
    VfsError::WouldBlock.to_errno()
}

/// Return the parent task id of the current process.
pub fn getppid() -> i64 {
    crate::sched::current()
        .map(|task| task.stats().ppid as i64)
        .unwrap_or(0)
}

const WNOHANG: u32 = 1;
const WUNTRACED: u32 = 2;
const WCONTINUED: u32 = 8;

/// Linux wait4 subset used by the first init/shell milestone.  `rusage` and
/// stopped/continued states are rejected with ENOSYS rather than faked.
pub fn wait4(pid: i64, status: u64, options: u32, rusage: u64) -> i64 {
    if rusage != 0 || options & !(WNOHANG | WUNTRACED | WCONTINUED) != 0 {
        return -abi::ENOSYS;
    }
    let Some(current) = crate::sched::current() else {
        return -abi::ESRCH;
    };
    let parent = current.stats().tid;
    let pgid = current.stats().tgid;
    loop {
        for zombie in crate::sched::zombies() {
            let stats = zombie.stats();
            if stats.ppid != parent {
                continue;
            }
            let matches = if pid == -1 {
                true
            } else if pid == 0 {
                stats.tgid == pgid
            } else if pid > 0 {
                stats.tid == pid as u64
            } else {
                stats.tgid == pid.unsigned_abs()
            };
            if !matches {
                continue;
            }
            let Some(zombie) = crate::sched::take_zombie(stats.tid) else {
                continue;
            };
            let code = zombie.stats().exit_code;
            if status != 0 {
                let encoded = (code & 0xff) << 8;
                if let Err(error) = uaccess::write_value(status, &encoded) {
                    return error.to_errno();
                }
            }
            return stats.tid as i64;
        }
        if !crate::sched::has_children(parent) {
            return -abi::ECHILD;
        }
        if options & WNOHANG != 0 {
            return 0;
        }
        crate::sched::sleep_micros(1000);
    }
}

#[cfg(target_arch = "x86_64")]
pub fn fork() -> i64 {
    clone(17, 0, 0, 0, 0)
}

/// Process-only clone path.  Threads (CLONE_VM/THREAD/SIGHAND/FILES/FS) are
/// rejected until the shared-resource model is implemented; returning ENOSYS
/// is deliberate and visible to user space.
#[cfg(target_arch = "x86_64")]
pub fn clone(flags: u64, stack: u64, _parent_tid: u64, _child_tid: u64, _tls: u64) -> i64 {
    if flags & !0xff != 0 {
        return -abi::ENOSYS;
    }
    let Some(parent) = crate::sched::current() else {
        return -abi::ESRCH;
    };
    let Some(parent_space) = parent.address_space() else {
        return -abi::EINVAL;
    };
    let Some(context) = crate::arch::syscall::capture_user_context() else {
        return -abi::EINVAL;
    };
    let files = Arc::new(SpinLock::new(parent.files().lock().clone()));
    let mut space = parent_space.lock();
    let child_space = match space.fork_copy() {
        Ok(space) => space,
        Err(_) => return -abi::ENOMEM,
    };
    drop(space);
    let mut child_context = context;
    if stack != 0 {
        child_context.rsp = stack;
    }
    let child_space = Arc::new(SpinLock::new(child_space));
    let child_context = alloc::boxed::Box::new(child_context);
    let result = crate::sched::spawn_with_fds(
        move || {
            if let Some(current) = crate::sched::current() {
                current.lock().set_address_space(child_space.clone());
            }
            child_space.lock().activate();
            // SAFETY: the child owns this COW address space and the captured
            // user context; the child task never returns to kernel mode here.
            unsafe { crate::arch::syscall::enter_user_context(&child_context) }
        },
        0,
        crate::sched::CpuMask::all(),
        Some(files),
    );
    match result {
        Ok(tid) => tid as i64,
        Err(_) => -abi::ENOMEM,
    }
}
