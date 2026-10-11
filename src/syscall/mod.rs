/*
 *
 *       src/syscall/mod.rs
 *       Architecture-independent Linux ABI system call dispatcher
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Linux ABI system call dispatcher.

pub mod numbers;

use crate::{
    kinfo, kwarn, sched,
    tty::{self, ReadError, TtyDevice},
};
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
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> i64 {
    match num {
        SYS_READ => sys_read(arg0 as i32, arg1 as *mut u8, arg2 as usize),
        SYS_WRITE => sys_write(arg0 as i32, arg1 as *const u8, arg2 as usize),
        SYS_GETPID => sys_getpid(),
        SYS_SCHED_YIELD => sys_sched_yield(),
        SYS_NANOSLEEP => sys_nanosleep(arg0),
        SYS_EXIT | SYS_EXIT_GROUP => sys_exit(arg0 as i32),
        SYS_SET_TID_ADDRESS => sys_set_tid_address(arg0),
        SYS_BRK => sys_brk(arg0),
        #[cfg(target_arch = "x86_64")]
        SYS_ARCH_PRCTL => sys_arch_prctl(arg0 as u32, arg1),
        _ => {
            kwarn!("unimplemented syscall: num={}", num);
            -38 // -ENOSYS
        }
    }
}

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    if fd != 0 || buf.is_null() || count == 0 {
        return -9; // -EBADF or -EINVAL
    }
    let limit = count.min(1024);
    let mut temp = [0u8; 1024];
    match tty::read(TtyDevice::Serial0, &mut temp[..limit]) {
        Ok(read_bytes) => {
            unsafe {
                core::ptr::copy_nonoverlapping(temp.as_ptr(), buf, read_bytes);
            }
            read_bytes as i64
        }
        Err(ReadError::WouldBlock) => -11, // -EAGAIN
        Err(_) => -5,                      // -EIO
    }
}

fn sys_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    if (fd != 1 && fd != 2) || buf.is_null() {
        return -9; // -EBADF
    }
    if count == 0 {
        return 0;
    }
    let limit = count.min(4096);
    let mut temp = [0u8; 4096];
    unsafe {
        core::ptr::copy_nonoverlapping(buf, temp.as_mut_ptr(), limit);
    }
    tty::write(TtyDevice::Serial0, &temp[..limit]);
    tty::write(TtyDevice::Foreground, &temp[..limit]);
    limit as i64
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
        return -14; // -EFAULT
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
    // Advance brk to page aligned address
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
        _ => -22, // -EINVAL
    }
}
