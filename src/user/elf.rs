/*
 *
 *       src/user/elf.rs
 *       Robust 64-bit static ELF binary parser and address space loader
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! 64-bit static ELF binary parser and address space loader.

use super::address_space::UserAddressSpace;
use crate::mm::pmm;

const EI_MAG0: usize = 0;
const EI_MAG1: usize = 1;
const EI_MAG2: usize = 2;
const EI_MAG3: usize = 3;
const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;

const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;

#[cfg(target_arch = "x86_64")]
const EXPECTED_MACHINE: u16 = 62; // EM_X86_64
#[cfg(target_arch = "aarch64")]
const EXPECTED_MACHINE: u16 = 183; // EM_AARCH64
#[cfg(target_arch = "riscv64")]
const EXPECTED_MACHINE: u16 = 243; // EM_RISCV

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1 << 0;
const PF_W: u32 = 1 << 1;
const PF_R: u32 = 1 << 2;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub struct LoadedElf {
    pub entry: u64,
}

pub fn load_elf(
    address_space: &mut UserAddressSpace,
    data: &[u8],
) -> Result<LoadedElf, &'static str> {
    if data.len() < core::mem::size_of::<Elf64Header>() {
        return Err("ELF data smaller than header");
    }

    // SAFETY: data length checked above; Elf64Header is plain data
    let header = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Elf64Header) };

    if header.e_ident[EI_MAG0] != 0x7f
        || header.e_ident[EI_MAG1] != b'E'
        || header.e_ident[EI_MAG2] != b'L'
        || header.e_ident[EI_MAG3] != b'F'
    {
        return Err("invalid ELF magic");
    }

    if header.e_ident[EI_CLASS] != ELFCLASS64 {
        return Err("not a 64-bit ELF");
    }

    if header.e_ident[EI_DATA] != ELFDATA2LSB {
        return Err("not a little-endian ELF");
    }

    if header.e_type != ET_EXEC && header.e_type != ET_DYN {
        return Err("unsupported ELF type (must be EXEC or DYN)");
    }

    if header.e_machine != EXPECTED_MACHINE {
        return Err("ELF machine architecture mismatch");
    }

    let phoff = header.e_phoff as usize;
    let phentsize = header.e_phentsize as usize;
    let phnum = header.e_phnum as usize;

    if phoff + phnum * phentsize > data.len() {
        return Err("ELF program headers exceed file boundary");
    }

    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        let ph = unsafe {
            core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Elf64ProgramHeader)
        };

        if ph.p_type == PT_LOAD {
            load_segment(address_space, data, &ph)?;
        }
    }

    Ok(LoadedElf {
        entry: header.e_entry,
    })
}

fn load_segment(
    address_space: &mut UserAddressSpace,
    data: &[u8],
    ph: &Elf64ProgramHeader,
) -> Result<(), &'static str> {
    if ph.p_memsz == 0 {
        return Ok(());
    }

    if ph.p_filesz > ph.p_memsz {
        return Err("PT_LOAD filesz > memsz");
    }

    let file_start = ph.p_offset as usize;
    let file_end = file_start + ph.p_filesz as usize;
    if file_end > data.len() {
        return Err("PT_LOAD segment exceeds file data length");
    }

    let vaddr_start = ph.p_vaddr;
    let vaddr_end = vaddr_start + ph.p_memsz;

    let page_start = vaddr_start & !4095;
    let page_end = (vaddr_end + 4095) & !4095;
    let pages = ((page_end - page_start) / 4096) as usize;

    let writable = (ph.p_flags & PF_W) != 0;
    let executable = (ph.p_flags & PF_X) != 0;

    for i in 0..pages {
        let page_va = page_start + (i as u64) * 4096;
        let frame = pmm::alloc(1).ok_or("out of memory for ELF segment frame")?;
        let frame_ptr = pmm::phys_to_virt(frame) as *mut u8;

        // Zero the entire physical frame first (handles BSS)
        unsafe {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }

        // Copy matching segment data if any overlaps this page
        let seg_page_start = page_va.max(vaddr_start);
        let seg_page_end = (page_va + 4096).min(vaddr_start + ph.p_filesz);

        if seg_page_start < seg_page_end {
            let offset_in_segment = (seg_page_start - vaddr_start) as usize;
            let copy_len = (seg_page_end - seg_page_start) as usize;
            let offset_in_page = (seg_page_start - page_va) as usize;

            let src =
                &data[file_start + offset_in_segment..file_start + offset_in_segment + copy_len];
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    frame_ptr.add(offset_in_page),
                    copy_len,
                );
            }
        }

        address_space.map_page(page_va, frame, writable, executable)?;
    }

    Ok(())
}
