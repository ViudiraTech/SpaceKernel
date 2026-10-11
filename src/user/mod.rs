/*
 *
 *       src/user/mod.rs
 *       User space process model, ELF execution and Linux ABI verification
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! User space support, isolated address spaces, ELF loading, and process lifecycle.

pub mod address_space;
pub mod elf;
pub mod process;
pub mod stack;
pub mod test_binaries;

pub use address_space::UserAddressSpace;
pub use process::spawn_user_elf;

use crate::kinfo;

/// Execute the architecture-specific built-in user ELF test binary and verify
/// that it exits with code 42 via the Linux ABI.
pub fn run_self_test() -> Result<(), &'static str> {
    kinfo!("Starting user mode Linux ABI self-test...");
    let tid = spawn_user_elf(test_binaries::TEST_USER_ELF, "user_test")?;
    kinfo!("Spawned user test task (tid={})", tid);

    // Wait for the user task to finish and check its exit code
    let mut loops = 0;
    while crate::syscall::last_user_exit_code() != 42 {
        crate::sched::sleep_micros(10_000);
        loops += 1;
        if loops > 1000 {
            return Err("user mode self-test timed out waiting for exit code 42");
        }
    }
    kinfo!("user self-test passed: exit code 42 verified");
    Ok(())
}
