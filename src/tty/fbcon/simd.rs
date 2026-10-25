/*
 *
 *       src/tty/fbcon/simd.rs
 *       SSE2 streaming acceleration and bulk memory operations for fbcon
 *
 *       2026/10/11 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Acceleration routines for framebuffer console operations.
//!
//! Ordinary kernel code uses a soft-float ABI. When hardware FP/SIMD state
//! is managed and borrowed via `fpu::with_kernel`, x86_64 can use 128-bit
//! non-temporal streaming stores to achieve maximum VRAM throughput without
//! polluting the CPU data cache. Pure 64-bit integer fallbacks are provided
//! for other architectures or when FPU borrowing is inactive.

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spacekernel_fpsimd_fb_clear(dst: *mut u8, bytes: usize, pattern32: u32) {
    let pattern64 = (pattern32 as u64) | ((pattern32 as u64) << 32);
    let mut ptr = dst;
    let mut remaining = bytes;

    // Align destination to 16-byte boundary using 32-bit volatile writes
    while (ptr as usize & 0xf) != 0 && remaining >= 4 {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u32, pattern32);
            ptr = ptr.add(4);
        }
        remaining -= 4;
    }

    // Unrolled 64-byte chunks with streaming non-temporal stores
    let chunks_64 = remaining / 64;
    if chunks_64 > 0 {
        unsafe {
            core::arch::asm!(
                "movq xmm0, {pattern}",
                "punpcklqdq xmm0, xmm0",
                "2:",
                "movntdq [{ptr}], xmm0",
                "movntdq [{ptr} + 16], xmm0",
                "movntdq [{ptr} + 32], xmm0",
                "movntdq [{ptr} + 48], xmm0",
                "add {ptr}, 64",
                "dec {count}",
                "jnz 2b",
                "sfence",
                pattern = in(reg) pattern64,
                ptr = inout(reg) ptr => ptr,
                count = inout(reg) chunks_64 => _,
                out("xmm0") _,
                options(nostack)
            );
        }
        remaining %= 64;
    }

    // Remaining 16-byte chunks
    let chunks_16 = remaining / 16;
    if chunks_16 > 0 {
        unsafe {
            core::arch::asm!(
                "movq xmm0, {pattern}",
                "punpcklqdq xmm0, xmm0",
                "3:",
                "movntdq [{ptr}], xmm0",
                "add {ptr}, 16",
                "dec {count}",
                "jnz 3b",
                "sfence",
                pattern = in(reg) pattern64,
                ptr = inout(reg) ptr => ptr,
                count = inout(reg) chunks_16 => _,
                out("xmm0") _,
                options(nostack)
            );
        }
        remaining %= 16;
    }

    // Trailing 64-bit and 32-bit words
    while remaining >= 8 {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u64, pattern64);
            ptr = ptr.add(8);
        }
        remaining -= 8;
    }
    while remaining >= 4 {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u32, pattern32);
            ptr = ptr.add(4);
        }
        remaining -= 4;
    }
    while remaining > 0 {
        unsafe {
            core::ptr::write_volatile(ptr, pattern32 as u8);
            ptr = ptr.add(1);
        }
        remaining -= 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spacekernel_fpsimd_fb_copy(dst: *mut u8, src: *const u8, bytes: usize) {
    let mut d = dst;
    let mut s = src;
    let mut remaining = bytes;

    // Scrolling moves scanlines forward (dst < src).
    // Align destination to 16-byte boundary for movntdq
    while (d as usize & 0xf) != 0 && remaining >= 8 {
        unsafe {
            core::ptr::write_volatile(d as *mut u64, core::ptr::read_volatile(s as *const u64));
            d = d.add(8);
            s = s.add(8);
        }
        remaining -= 8;
    }

    // 64-byte burst copy: movdqu loads and movntdq streaming stores
    let chunks_64 = remaining / 64;
    if chunks_64 > 0 {
        unsafe {
            core::arch::asm!(
                "2:",
                "movdqu xmm0, [{src}]",
                "movdqu xmm1, [{src} + 16]",
                "movdqu xmm2, [{src} + 32]",
                "movdqu xmm3, [{src} + 48]",
                "movntdq [{dst}], xmm0",
                "movntdq [{dst} + 16], xmm1",
                "movntdq [{dst} + 32], xmm2",
                "movntdq [{dst} + 48], xmm3",
                "add {src}, 64",
                "add {dst}, 64",
                "dec {count}",
                "jnz 2b",
                "sfence",
                src = inout(reg) s => s,
                dst = inout(reg) d => d,
                count = inout(reg) chunks_64 => _,
                out("xmm0") _,
                out("xmm1") _,
                out("xmm2") _,
                out("xmm3") _,
                options(nostack)
            );
        }
        remaining %= 64;
    }

    // Remaining 16-byte chunks
    let chunks_16 = remaining / 16;
    if chunks_16 > 0 {
        unsafe {
            core::arch::asm!(
                "3:",
                "movdqu xmm0, [{src}]",
                "movntdq [{dst}], xmm0",
                "add {src}, 16",
                "add {dst}, 16",
                "dec {count}",
                "jnz 3b",
                "sfence",
                src = inout(reg) s => s,
                dst = inout(reg) d => d,
                count = inout(reg) chunks_16 => _,
                out("xmm0") _,
                options(nostack)
            );
        }
        remaining %= 16;
    }

    // Trailing quadwords and bytes
    while remaining >= 8 {
        unsafe {
            core::ptr::write_volatile(d as *mut u64, core::ptr::read_volatile(s as *const u64));
            d = d.add(8);
            s = s.add(8);
        }
        remaining -= 8;
    }
    while remaining > 0 {
        unsafe {
            core::ptr::write_volatile(d, core::ptr::read_volatile(s));
            d = d.add(1);
            s = s.add(1);
        }
        remaining -= 1;
    }
}

/// Architecture-independent fast 64-bit integer fill.
pub unsafe fn fast_clear(dst: *mut u8, mut bytes: usize, pattern32: u32) {
    let pattern64 = (pattern32 as u64) | ((pattern32 as u64) << 32);
    let mut ptr = dst;

    // Align to 8 bytes if needed
    while (ptr as usize & 0x7) != 0 && bytes >= 4 {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u32, pattern32);
            ptr = ptr.add(4);
        }
        bytes -= 4;
    }

    // Bulk 64-bit quadword writes
    let words = bytes / 8;
    for _ in 0..words {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u64, pattern64);
            ptr = ptr.add(8);
        }
    }
    bytes %= 8;

    while bytes >= 4 {
        unsafe {
            core::ptr::write_volatile(ptr as *mut u32, pattern32);
            ptr = ptr.add(4);
        }
        bytes -= 4;
    }
    while bytes > 0 {
        unsafe {
            core::ptr::write_volatile(ptr, pattern32 as u8);
            ptr = ptr.add(1);
        }
        bytes -= 1;
    }
}

/// Architecture-independent fast 64-bit integer forward copy.
pub unsafe fn fast_copy(dst: *mut u8, src: *const u8, mut bytes: usize) {
    let mut d = dst;
    let mut s = src;

    // Align to 8 bytes if needed
    while (d as usize & 0x7) != 0 && bytes >= 1 {
        unsafe {
            core::ptr::write_volatile(d, core::ptr::read_volatile(s));
            d = d.add(1);
            s = s.add(1);
        }
        bytes -= 1;
    }

    let words = bytes / 8;
    for _ in 0..words {
        unsafe {
            core::ptr::write_volatile(d as *mut u64, core::ptr::read_volatile(s as *const u64));
            d = d.add(8);
            s = s.add(8);
        }
    }
    bytes %= 8;

    while bytes > 0 {
        unsafe {
            core::ptr::write_volatile(d, core::ptr::read_volatile(s));
            d = d.add(1);
            s = s.add(1);
        }
        bytes -= 1;
    }
}
