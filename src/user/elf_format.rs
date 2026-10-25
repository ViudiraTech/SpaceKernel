/*
 *       src/user/elf_format.rs
 *       Bounds-checked ELF64 parsing and immutable load planning
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

//! Pure parsing: no pointer casts, allocation of physical pages or mutations.
use alloc::{collections::BTreeMap, string::String, vec::Vec};
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
pub const PT_GNU_RELRO: u32 = 0x6474e552;
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub address: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}
pub struct Plan {
    pub entry: u64,
    pub bias: u64,
    pub phoff: usize,
    pub phnum: u16,
    pub phdr: Option<u64>,
    pub interpreter: Option<String>,
    pub has_dynamic: bool,
    pub segments: Vec<Segment>,
    pub pages: BTreeMap<u64, u32>,
    pub end: u64,
    pub tls: Option<Segment>,
    pub relro: Option<(u64, u64)>,
}
fn u16_at(d: &[u8], o: usize) -> Result<u16, &'static str> {
    Ok(u16::from_le_bytes(
        d.get(o..o + 2).ok_or("truncated ELF")?.try_into().unwrap(),
    ))
}
fn u32_at(d: &[u8], o: usize) -> Result<u32, &'static str> {
    Ok(u32::from_le_bytes(
        d.get(o..o + 4).ok_or("truncated ELF")?.try_into().unwrap(),
    ))
}
fn u64_at(d: &[u8], o: usize) -> Result<u64, &'static str> {
    Ok(u64::from_le_bytes(
        d.get(o..o + 8).ok_or("truncated ELF")?.try_into().unwrap(),
    ))
}
fn end(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_add(b).ok_or("ELF range overflow")
}
fn up(n: u64) -> Result<u64, &'static str> {
    Ok(end(n, 4095)? & !4095)
}

pub fn parse(
    data: &[u8],
    machine: u16,
    dynamic_bias: u64,
    user_limit: u64,
) -> Result<Plan, &'static str> {
    if data.len() < 64
        || &data[..4] != b"\x7fELF"
        || data[4..7] != [2, 1, 1]
        || u16_at(data, 18)? != machine
        || u32_at(data, 20)? != 1
        || u16_at(data, 52)? != 64
        || u16_at(data, 54)? != 56
    {
        return Err("invalid ELF64 header or architecture");
    }
    let kind = u16_at(data, 16)?;
    if kind != 2 && kind != 3 {
        return Err("unsupported ELF file type");
    }
    let flags = u32_at(data, 48)?;
    if machine != 243 && flags != 0 || machine == 243 && flags & !0x1f != 0 {
        return Err("unsupported ELF architecture flags");
    }
    let phnum = u16_at(data, 56)?;
    if phnum == 0 || phnum > 1024 {
        return Err("invalid ELF program header count");
    }
    let phoff = usize::try_from(u64_at(data, 32)?).map_err(|_| "ELF offset overflow")?;
    if phoff < 64
        || phoff
            .checked_add(phnum as usize * 56)
            .is_none_or(|e| e > data.len())
    {
        return Err("ELF program headers outside file");
    }
    let mut raw = Vec::new();
    let mut alignment = 4096;
    for i in 0..phnum as usize {
        let o = phoff + i * 56;
        let s = Segment {
            kind: u32_at(data, o)?,
            flags: u32_at(data, o + 4)?,
            offset: u64_at(data, o + 8)?,
            address: u64_at(data, o + 16)?,
            filesz: u64_at(data, o + 32)?,
            memsz: u64_at(data, o + 40)?,
            align: u64_at(data, o + 48)?,
        };
        if end(s.offset, s.filesz)? > data.len() as u64 {
            return Err("ELF segment outside file");
        }
        if matches!(s.kind, PT_LOAD | PT_TLS) {
            if s.filesz > s.memsz
                || s.flags & !7 != 0
                || s.align > 1
                    && (!s.align.is_power_of_two()
                        || s.align > 2 * 1024 * 1024
                        || s.address % s.align != s.offset % s.align)
            {
                return Err("invalid ELF segment geometry");
            }
        }
        if s.kind == PT_LOAD {
            if s.address % 4096 != s.offset % 4096 {
                return Err("ELF page alignment mismatch");
            }
            alignment = alignment.max(s.align);
        }
        raw.push(s);
    }
    let bias = if kind == 3 {
        dynamic_bias & !(alignment - 1)
    } else {
        0
    };
    let entry = end(u64_at(data, 24)?, bias)?;
    if entry < 4096 || entry >= user_limit {
        return Err("invalid ELF entry");
    }
    let mut plan = Plan {
        entry,
        bias,
        phoff,
        phnum,
        phdr: None,
        interpreter: None,
        has_dynamic: false,
        segments: Vec::new(),
        pages: BTreeMap::new(),
        end: 0,
        tls: None,
        relro: None,
    };
    let mut pt_phdr = None;
    let mut dynamic_seg = None;
    for mut s in raw {
        s.address = end(s.address, bias)?;
        if s.kind == PT_PHDR {
            if pt_phdr.is_some()
                || s.filesz != phnum as u64 * 56
                || s.memsz != s.filesz
                || s.offset != phoff as u64
            {
                return Err("invalid ELF PT_PHDR");
            }
            pt_phdr = Some(s.address);
        }
        if s.kind == PT_DYNAMIC {
            if dynamic_seg.is_some() || s.filesz > s.memsz || s.filesz % 16 != 0 {
                return Err("invalid ELF PT_DYNAMIC");
            }
            plan.has_dynamic = true;
            dynamic_seg = Some(s);
        }
        if s.kind == PT_INTERP {
            if plan.interpreter.is_some() || s.filesz < 2 || s.filesz > 4096 {
                return Err("invalid ELF interpreter length");
            }
            let bytes = &data[s.offset as usize..(s.offset + s.filesz) as usize];
            if bytes.last() != Some(&0) || bytes[..bytes.len() - 1].contains(&0) || bytes[0] != b'/'
            {
                return Err("invalid ELF interpreter path");
            }
            plan.interpreter = Some(String::from(
                core::str::from_utf8(&bytes[..bytes.len() - 1])
                    .map_err(|_| "non UTF8 interpreter path")?,
            ));
        }
        if s.kind == 0x6474e551 && s.flags & 1 != 0 {
            return Err("executable ELF stacks prohibited");
        }
        if matches!(s.kind, PT_LOAD | PT_TLS | PT_GNU_RELRO) {
            let high = end(s.address, s.memsz)?;
            if s.address < 4096 || high > user_limit || s.memsz > 256 * 1024 * 1024 {
                return Err("ELF segment outside user range");
            }
            if s.kind == PT_LOAD && s.memsz != 0 {
                if plan.segments.iter().any(|p: &Segment| {
                    p.kind == PT_LOAD
                        && p.memsz != 0
                        && p.address < high
                        && s.address < p.address + p.memsz
                }) {
                    return Err("conflicting ELF segment overlap");
                }
                let prot = if s.flags & 4 != 0 { 1 } else { 0 }
                    | if s.flags & 2 != 0 { 2 } else { 0 }
                    | if s.flags & 1 != 0 { 4 } else { 0 };
                for p in (s.address & !4095..up(high)?).step_by(4096) {
                    let permission = plan.pages.entry(p).or_insert(0);
                    *permission |= prot;
                    if *permission & 6 == 6 {
                        return Err("writable executable ELF page prohibited");
                    }
                    if plan.pages.len() > 65536 {
                        return Err("ELF image exceeds resource limit");
                    }
                }
                let phend = phoff as u64 + phnum as u64 * 56;
                if s.offset <= phoff as u64 && phend <= s.offset + s.filesz {
                    plan.phdr = Some(s.address + phoff as u64 - s.offset);
                }
                plan.end = plan.end.max(up(high)?);
            }
            if s.kind == PT_TLS {
                if plan.tls.is_some() {
                    return Err("duplicate ELF TLS template");
                }
                plan.tls = Some(s);
            }
            if s.kind == PT_GNU_RELRO {
                if plan.relro.is_some() {
                    return Err("duplicate ELF RELRO");
                }
                plan.relro = Some((s.address, high));
            }
        }
        plan.segments.push(s);
    }
    if let Some(expected_phdr) = pt_phdr {
        if plan.phdr != Some(expected_phdr) {
            return Err("ELF PT_PHDR not covered by PT_LOAD");
        }
    }
    if let Some(d) = dynamic_seg {
        if !plan.segments.iter().any(|s| {
            s.kind == PT_LOAD
                && d.address >= s.address
                && d.address + d.memsz <= s.address + s.memsz
        }) {
            return Err("ELF PT_DYNAMIC outside load image");
        }
        if kind == 3 && plan.interpreter.is_none() {
            let dyn_bytes = &data[d.offset as usize..(d.offset + d.filesz) as usize];
            for entry_bytes in dyn_bytes.chunks_exact(16) {
                let tag = u64::from_le_bytes(entry_bytes[..8].try_into().unwrap());
                if tag == 0 {
                    break;
                }
                if tag == 1 {
                    return Err("dynamic ELF requires PT_INTERP");
                }
            }
        }
    }
    if !plan.segments.iter().any(|s| {
        s.kind == PT_LOAD && s.flags & 1 != 0 && entry >= s.address && entry < s.address + s.memsz
    }) {
        return Err("ELF entry outside executable segment");
    }
    if let Some(t) = plan.tls {
        if !plan.segments.iter().any(|s| {
            s.kind == PT_LOAD
                && t.address >= s.address
                && t.address + t.memsz <= s.address + s.memsz
        }) {
            return Err("ELF TLS outside load image");
        }
    }
    if let Some((low, high)) = plan.relro {
        if low < high
            && !(low & !4095..up(high)?)
                .step_by(4096)
                .all(|p| plan.pages.contains_key(&p))
        {
            return Err("RELRO outside ELF image");
        }
    }
    Ok(plan)
}
