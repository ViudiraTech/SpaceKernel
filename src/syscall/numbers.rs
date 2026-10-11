/*
 *
 *       src/syscall/numbers.rs
 *       Architecture-specific Linux system call numbers
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Linux ABI system call numbers.

#[cfg(target_arch = "x86_64")]
pub const SYS_READ: u64 = 0;
#[cfg(target_arch = "x86_64")]
pub const SYS_WRITE: u64 = 1;
#[cfg(target_arch = "x86_64")]
pub const SYS_MMAP: u64 = 9;
#[cfg(target_arch = "x86_64")]
pub const SYS_MUNMAP: u64 = 11;
#[cfg(target_arch = "x86_64")]
pub const SYS_BRK: u64 = 12;
#[cfg(target_arch = "x86_64")]
pub const SYS_SCHED_YIELD: u64 = 24;
#[cfg(target_arch = "x86_64")]
pub const SYS_NANOSLEEP: u64 = 35;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETPID: u64 = 39;
#[cfg(target_arch = "x86_64")]
pub const SYS_EXIT: u64 = 60;
#[cfg(target_arch = "x86_64")]
pub const SYS_ARCH_PRCTL: u64 = 158;
#[cfg(target_arch = "x86_64")]
pub const SYS_SET_TID_ADDRESS: u64 = 218;
#[cfg(target_arch = "x86_64")]
pub const SYS_EXIT_GROUP: u64 = 231;

#[cfg(not(target_arch = "x86_64"))]
pub const SYS_READ: u64 = 63;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_WRITE: u64 = 64;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EXIT: u64 = 93;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EXIT_GROUP: u64 = 94;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SET_TID_ADDRESS: u64 = 96;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_NANOSLEEP: u64 = 101;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SCHED_YIELD: u64 = 124;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETPID: u64 = 172;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_BRK: u64 = 214;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MUNMAP: u64 = 215;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MMAP: u64 = 222;
