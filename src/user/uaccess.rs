/*
 *       src/user/uaccess.rs
 *       Checked user copies through retained physical mappings
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

//! User pointers are never dereferenced. Copies hold the address-space lock and
//! use retained HHDM pages, so unmap cannot race validation and copying.

use super::UserAddressSpace;
use crate::{sched, vfs::VfsError};
use alloc::{string::String, vec, vec::Vec};

/// Types whose bytes are a stable ABI representation.
/// # Safety
/// All bit patterns must be valid, and values must contain no uninitialized padding.
pub unsafe trait UserPod: Copy {}
macro_rules! pod { ($($t:ty),*) => { $(unsafe impl UserPod for $t {})* }; }
pod!(u8, u16, u32, u64, i32, i64);
unsafe impl<T: UserPod, const N: usize> UserPod for [T; N] {}
unsafe impl UserPod for crate::vfs::abi::IoVec {}
unsafe impl UserPod for crate::vfs::abi::WinSize {}
unsafe impl UserPod for crate::vfs::abi::LinuxStat {}
unsafe impl UserPod for crate::vfs::abi::LinuxStatFs {}

pub fn current_space() -> Result<alloc::sync::Arc<crate::sync::SpinLock<UserAddressSpace>>, VfsError>
{
    sched::current()
        .and_then(|t| t.lock().address_space())
        .ok_or(VfsError::Fault)
}

#[cfg(feature = "boot-self-test")]
static KERNEL_TEST_ACCESS: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Enable trusted kernel buffers only during the single-CPU, pre-scheduler tests.
/// # Safety
/// Every syscall pointer passed by `test` must point into a live kernel object.
/// No user task or secondary CPU may run until this scope returns.
#[cfg(feature = "boot-self-test")]
pub unsafe fn kernel_test_scope(test: impl FnOnce()) {
    use core::sync::atomic::Ordering;
    assert!(sched::current().is_none());
    assert!(!KERNEL_TEST_ACCESS.swap(true, Ordering::AcqRel));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            KERNEL_TEST_ACCESS.store(false, Ordering::Release);
        }
    }
    let _reset = Reset;
    test();
}

fn kernel_test_access() -> bool {
    #[cfg(feature = "boot-self-test")]
    {
        return KERNEL_TEST_ACCESS.load(core::sync::atomic::Ordering::Acquire);
    }
    #[cfg(not(feature = "boot-self-test"))]
    {
        false
    }
}

pub fn check(address: u64, length: usize, write: bool) -> Result<(), VfsError> {
    if length == 0 {
        return Ok(());
    }
    if kernel_test_access() {
        return if address != 0 && address.checked_add(length as u64).is_some() {
            Ok(())
        } else {
            Err(VfsError::Fault)
        };
    }
    current_space()?.lock().validate(address, length, write)
}

pub fn read(address: u64, dst: &mut [u8]) -> Result<(), VfsError> {
    if dst.is_empty() {
        return Ok(());
    }
    if kernel_test_access() {
        check(address, dst.len(), false)?;
        // SAFETY: kernel_test_scope requires live kernel buffers from the test.
        unsafe {
            core::ptr::copy_nonoverlapping(address as *const u8, dst.as_mut_ptr(), dst.len())
        };
        return Ok(());
    }
    current_space()?.lock().read_bytes(address, dst)
}

pub fn write(address: u64, src: &[u8]) -> Result<(), VfsError> {
    if src.is_empty() {
        return Ok(());
    }
    if kernel_test_access() {
        check(address, src.len(), true)?;
        // SAFETY: same trusted, pre-scheduler test scope as read().
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), address as *mut u8, src.len()) };
        return Ok(());
    }
    current_space()?.lock().write_bytes(address, src)
}

pub fn read_value<T: UserPod>(address: u64) -> Result<T, VfsError> {
    let mut value = core::mem::MaybeUninit::<T>::uninit();
    // SAFETY: UserPod admits all bit patterns; read initializes every byte.
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(value.as_mut_ptr() as *mut u8, core::mem::size_of::<T>())
    };
    read(address, bytes)?;
    Ok(unsafe { value.assume_init() })
}

pub fn write_value<T: UserPod>(address: u64, value: &T) -> Result<(), VfsError> {
    // SAFETY: UserPod forbids uninitialized padding in the ABI representation.
    let bytes = unsafe {
        core::slice::from_raw_parts(value as *const T as *const u8, core::mem::size_of::<T>())
    };
    write(address, bytes)
}

pub fn read_vec(address: u64, length: usize, limit: usize) -> Result<Vec<u8>, VfsError> {
    if length > limit {
        return Err(VfsError::InvalidArgument);
    }
    let mut bytes = vec![0; length];
    read(address, &mut bytes)?;
    Ok(bytes)
}

pub fn string(address: u64, limit: usize) -> Result<String, VfsError> {
    let mut result = Vec::new();
    for index in 0..limit {
        let byte = read_value::<u8>(address.checked_add(index as u64).ok_or(VfsError::Fault)?)?;
        if byte == 0 {
            return String::from_utf8(result).map_err(|_| VfsError::InvalidArgument);
        }
        result.push(byte);
    }
    Err(VfsError::NameTooLong)
}
