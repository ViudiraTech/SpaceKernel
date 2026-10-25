/*
 *       src/user/elf.rs
 *       Transactional ELF64 mapping and interpreter handoff
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use super::{
    address_space::UserAddressSpace,
    elf_format::{self, PT_LOAD, Plan, Segment},
};
use alloc::vec::Vec;
#[cfg(target_arch = "x86_64")]
const MACHINE: u16 = 62;
#[cfg(target_arch = "aarch64")]
const MACHINE: u16 = 183;
#[cfg(target_arch = "riscv64")]
const MACHINE: u16 = 243;

pub struct LoadedElf {
    pub entry: u64,
    pub initial_pc: u64,
    pub phdr: u64,
    pub phnum: u16,
    pub interpreter_base: u64,
    pub tls: Option<Segment>,
    pub tls_tp: u64,
    pub relro: Option<(u64, u64)>,
}
fn map_plan(
    space: &mut UserAddressSpace,
    data: &[u8],
    plan: &Plan,
    allocated: &mut Vec<u64>,
) -> Result<u64, &'static str> {
    for (&address, &prot) in &plan.pages {
        space.map_zeroed(address, 4096, prot)?;
        allocated.push(address);
    }
    for s in &plan.segments {
        if s.kind == PT_LOAD && s.filesz != 0 {
            space
                .initialize(
                    s.address,
                    &data[s.offset as usize..(s.offset + s.filesz) as usize],
                )
                .map_err(|_| "ELF initialization failed")?;
        }
    }
    if let Some(phdr) = plan.phdr {
        return Ok(phdr);
    }
    let size = plan.phnum as u64 * 56;
    let address = space
        .find_free(0x6000_0000, size)
        .map_err(|_| "no room for PHDR")?;
    let length = super::address_space::align_up(size).ok_or("PHDR size overflow")?;
    space.map_zeroed(address, length, 1)?;
    allocated.extend((address..address + length).step_by(4096));
    space
        .initialize(address, &data[plan.phoff..plan.phoff + size as usize])
        .map_err(|_| "PHDR initialization failed")?;
    Ok(address)
}
fn bias(base: u64) -> u64 {
    let mut bytes = [0; 8];
    if crate::random::fill(&mut bytes).is_ok() {
        base + (u64::from_le_bytes(bytes) & 0x0ffe_0000)
    } else {
        base
    }
}
pub fn load_elf(space: &mut UserAddressSpace, data: &[u8]) -> Result<LoadedElf, &'static str> {
    let main = elf_format::parse(
        data,
        MACHINE,
        bias(0x1000_0000),
        UserAddressSpace::user_limit(),
    )?;
    // Parse both images before publishing a single page.
    let interpreter = if let Some(path) = &main.interpreter {
        let bytes = crate::vfs::read_file(path).map_err(|_| "ELF interpreter unavailable")?;
        let plan = elf_format::parse(
            &bytes,
            MACHINE,
            bias(UserAddressSpace::user_limit() / 2),
            UserAddressSpace::user_limit(),
        )?;
        if plan.interpreter.is_some() {
            return Err("recursive ELF interpreter");
        }
        if plan.pages.keys().any(|p| main.pages.contains_key(p)) {
            return Err("interpreter overlaps executable");
        }
        Some((bytes, plan))
    } else {
        None
    };
    if main.pages.keys().any(|&p| space.contains(p)) {
        return Err("ELF overlaps existing mapping");
    }
    let mut allocated = Vec::new();
    let result = (|| {
        let phdr = map_plan(space, data, &main, &mut allocated)?;
        let (initial_pc, interpreter_base) = if let Some((bytes, plan)) = &interpreter {
            map_plan(space, bytes, plan, &mut allocated)?;
            (plan.entry, plan.bias)
        } else {
            (main.entry, 0)
        };
        let mut tls_tp = 0;
        if interpreter.is_none() {
            if !main.has_dynamic {
                if let Some((low, high)) = main.relro {
                    if low < high {
                        let start = low & !4095;
                        let end =
                            super::address_space::align_up(high).ok_or("RELRO size overflow")?;
                        space
                            .protect(start, end - start, 1)
                            .map_err(|_| "RELRO protection failed")?;
                        space
                            .restrict_permissions(start, end - start, 1)
                            .map_err(|_| "RELRO restriction failed")?;
                    }
                }
            }
            if let Some(tls) = main.tls {
                let align = tls.align.max(8);
                let tcb_size = 16u64.max(align);
                let tls_memsz = tls
                    .memsz
                    .checked_add(align - 1)
                    .ok_or("TLS size overflow")?
                    & !(align - 1);
                let total = tls_memsz
                    .checked_add(tcb_size)
                    .and_then(super::address_space::align_up)
                    .ok_or("TLS size overflow")?;
                let base = space
                    .find_free(0x5000_0000, total)
                    .map_err(|_| "no room for TLS")?;
                space.map_zeroed(base, total, 3)?;
                allocated.extend((base..base + total).step_by(4096));
                #[cfg(target_arch = "x86_64")]
                let (data_start, tp) = (base + tls_memsz - tls.memsz, base + tls_memsz);
                #[cfg(target_arch = "aarch64")]
                let (data_start, tp) = (base + tcb_size, base);
                #[cfg(target_arch = "riscv64")]
                let (data_start, tp) = (base + tcb_size, base + tcb_size);
                if tls.filesz != 0 {
                    space
                        .initialize(
                            data_start,
                            &data[tls.offset as usize..(tls.offset + tls.filesz) as usize],
                        )
                        .map_err(|_| "TLS initialization failed")?;
                }
                #[cfg(target_arch = "x86_64")]
                space
                    .initialize(tp, &tp.to_le_bytes())
                    .map_err(|_| "TLS TCB initialization failed")?;
                tls_tp = tp;
            }
        }
        Ok(LoadedElf {
            entry: main.entry,
            initial_pc,
            phdr,
            phnum: main.phnum,
            interpreter_base,
            tls: main.tls,
            tls_tp,
            relro: main.relro,
        })
    })();
    if result.is_err() {
        for page in allocated {
            let _ = space.unmap(page, 4096);
        }
    } else {
        space.brk_base = main.end;
        space.brk = main.end;
    }
    result
}
