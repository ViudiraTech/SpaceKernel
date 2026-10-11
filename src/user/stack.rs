/*
 *
 *       src/user/stack.rs
 *       Linux System V ABI user process stack setup with auxv and arguments
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! System V ABI user stack initialization.

use super::address_space::UserAddressSpace;
use crate::mm::pmm;

#[cfg(target_arch = "riscv64")]
pub const USER_STACK_TOP: u64 = 0x0000_003f_ffff_0000;

#[cfg(not(target_arch = "riscv64"))]
pub const USER_STACK_TOP: u64 = 0x0000_7fff_ffff_0000;
pub const USER_STACK_PAGES: usize = 16; // 64 KiB user stack

// ELF Auxiliary Vector Types
const AT_NULL: u64 = 0;
const AT_PAGESZ: u64 = 6;
const AT_ENTRY: u64 = 9;
const AT_RANDOM: u64 = 25;

pub fn setup_user_stack(
    address_space: &mut UserAddressSpace,
    entry: u64,
    program_name: &str,
) -> Result<u64, &'static str> {
    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES as u64) * 4096;

    // Allocate and map physical frames for the user stack
    let mut top_frame_virt = 0 as *mut u8;
    for i in 0..USER_STACK_PAGES {
        let page_va = stack_base + (i as u64) * 4096;
        let frame = pmm::alloc(1).ok_or("out of memory for user stack frame")?;
        let virt = pmm::phys_to_virt(frame) as *mut u8;
        unsafe {
            core::ptr::write_bytes(virt, 0, 4096);
        }
        if i == USER_STACK_PAGES - 1 {
            top_frame_virt = virt;
        }
        address_space.map_page(page_va, frame, true, false)?;
    }

    // Now format the initial System V stack frame at the top of the stack
    // Frame layout (pushed from top to bottom):
    // [strings]
    // [AT_NULL, 0]
    // [AT_ENTRY, entry]
    // [AT_PAGESZ, 4096]
    // [AT_RANDOM, rand_ptr]
    // [NULL] (envp terminator)
    // [NULL] (argv terminator)
    // [argv[0]] -> ptr to prog_name
    // [argc] = 1

    let mut sp = USER_STACK_TOP;

    // 1. Push 16-byte random bytes for AT_RANDOM
    sp -= 16;
    let rand_va = sp;
    let rand_offset_in_top_page = (sp % 4096) as usize;
    unsafe {
        let rand_dst = top_frame_virt.add(rand_offset_in_top_page);
        core::ptr::copy_nonoverlapping(b"SpaceKernelRandom".as_ptr(), rand_dst, 16);
    }

    // 2. Push program name string
    let name_bytes = program_name.as_bytes();
    let name_len = name_bytes.len() + 1; // with null terminator
    sp -= name_len as u64;
    let name_va = sp;
    let name_offset = (sp % 4096) as usize;
    unsafe {
        let dst = top_frame_virt.add(name_offset);
        core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), dst, name_bytes.len());
        *dst.add(name_bytes.len()) = 0; // null terminate
    }

    // Align stack pointer to 16 bytes
    sp &= !15;

    // 3. Prepare auxv, envp, argv, argc
    let mut stack_values = [0u64; 32];
    let mut idx = 0;

    // argc
    stack_values[idx] = 1;
    idx += 1;
    // argv[0]
    stack_values[idx] = name_va;
    idx += 1;
    // argv terminator (NULL)
    stack_values[idx] = 0;
    idx += 1;
    // envp terminator (NULL)
    stack_values[idx] = 0;
    idx += 1;

    // auxv pairs
    stack_values[idx] = AT_PAGESZ;
    idx += 1;
    stack_values[idx] = 4096;
    idx += 1;

    stack_values[idx] = AT_ENTRY;
    idx += 1;
    stack_values[idx] = entry;
    idx += 1;

    stack_values[idx] = AT_RANDOM;
    idx += 1;
    stack_values[idx] = rand_va;
    idx += 1;

    stack_values[idx] = AT_NULL;
    idx += 1;
    stack_values[idx] = 0;
    idx += 1;

    let payload_bytes = idx * 8;
    sp -= payload_bytes as u64;
    sp &= !15; // 16-byte align

    let payload_offset = (sp % 4096) as usize;
    unsafe {
        let dst = top_frame_virt.add(payload_offset) as *mut u64;
        core::ptr::copy_nonoverlapping(stack_values.as_ptr(), dst, idx);
    }

    Ok(sp)
}
