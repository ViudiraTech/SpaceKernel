/*
 *
 *       src/user/test_binaries.rs
 *       Embedded test user ELF binaries for cross-architecture verification
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Built-in test user programs executing via Linux ABI.

#[cfg(target_arch = "x86_64")]
pub const TEST_USER_ELF: &[u8] = include_bytes!("../../user/test/test_x86_64.elf");

#[cfg(target_arch = "aarch64")]
pub const TEST_USER_ELF: &[u8] = include_bytes!("../../user/test/test_aarch64.elf");

#[cfg(target_arch = "riscv64")]
pub const TEST_USER_ELF: &[u8] = include_bytes!("../../user/test/test_riscv64.elf");
