/*
 *
 *       src/syscall/system.rs
 *       Linux UTS namespace system calls
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! `uname(2)`, `sethostname(2)` and `setdomainname(2)`, backed by the kernel
//! identity module so user-visible UTS data has a single source of truth.

use crate::{spk, user::uaccess, vfs::abi};

pub fn uname(pointer: u64) -> i64 {
    let bytes = spk::utsname().to_bytes();
    match uaccess::write(pointer, &bytes) {
        Ok(()) => 0,
        Err(error) => error.to_errno(),
    }
}

fn privileged() -> bool {
    crate::sched::current().is_some_and(|task| task.credentials().euid == 0)
}

pub fn sethostname(pointer: u64, length: u64) -> i64 {
    if !privileged() {
        return -abi::EPERM;
    }
    if length > 64 {
        return -abi::EINVAL;
    }
    let bytes = match uaccess::read_vec(pointer, length as usize, 64) {
        Ok(bytes) => bytes,
        Err(error) => return error.to_errno(),
    };
    match spk::set_hostname(&bytes) {
        Ok(()) => 0,
        Err(_) => -abi::EINVAL,
    }
}

pub fn setdomainname(pointer: u64, length: u64) -> i64 {
    if !privileged() {
        return -abi::EPERM;
    }
    if length > 64 {
        return -abi::EINVAL;
    }
    let bytes = match uaccess::read_vec(pointer, length as usize, 64) {
        Ok(bytes) => bytes,
        Err(error) => return error.to_errno(),
    };
    match spk::set_domainname(&bytes) {
        Ok(()) => 0,
        Err(_) => -abi::EINVAL,
    }
}
