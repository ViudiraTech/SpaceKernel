/*
 *       tools/subsystem_model_test.rs
 *       Adversarial ELF parsing regression tests using the kernel parser
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
extern crate alloc;
#[path="../src/user/elf_format.rs"]mod elf;
fn image()->Vec<u8>{let mut d=vec![0;4096];d[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");d[16..18].copy_from_slice(&2u16.to_le_bytes());d[18..20].copy_from_slice(&62u16.to_le_bytes());d[20..24].copy_from_slice(&1u32.to_le_bytes());d[24..32].copy_from_slice(&0x400080u64.to_le_bytes());d[32..40].copy_from_slice(&64u64.to_le_bytes());d[52..54].copy_from_slice(&64u16.to_le_bytes());d[54..56].copy_from_slice(&56u16.to_le_bytes());d[56..58].copy_from_slice(&1u16.to_le_bytes());d[64..68].copy_from_slice(&1u32.to_le_bytes());d[68..72].copy_from_slice(&5u32.to_le_bytes());d[80..88].copy_from_slice(&0x400000u64.to_le_bytes());d[96..104].copy_from_slice(&4096u64.to_le_bytes());d[104..112].copy_from_slice(&8192u64.to_le_bytes());d[112..120].copy_from_slice(&4096u64.to_le_bytes());d}
fn parse(d:&[u8])->Result<elf::Plan,&'static str>{elf::parse(d,62,0x10000000,1<<47)}
#[test]fn valid_bss_and_phdr(){let p=parse(&image()).unwrap();assert_eq!(p.pages.len(),2);assert_eq!(p.phdr,Some(0x400040));}
#[test]fn all_truncations_fail_without_unsafe_reads(){let d=image();for n in 0..d.len(){assert!(parse(&d[..n]).is_err(),"accepted truncation {n}");}}
#[test]fn header_stride_cannot_hide_fixed_size_read(){for size in [0u16,1,8,55,57,65535]{let mut d=image();d[54..56].copy_from_slice(&size.to_le_bytes());assert!(parse(&d).is_err());}}
#[test]fn range_overflow_is_rejected(){for off in [24usize,32,72,80,96,104]{let mut d=image();d[off..off+8].copy_from_slice(&u64::MAX.to_le_bytes());assert!(parse(&d).is_err(),"offset {off}");}}
#[test]fn user_kernel_boundary(){let mut d=image();d[80..88].copy_from_slice(&0xffff800000000000u64.to_le_bytes());assert!(parse(&d).is_err());}
#[test]fn pie_entry_and_segments_share_bias(){let mut d=image();d[16..18].copy_from_slice(&3u16.to_le_bytes());let p=parse(&d).unwrap();assert_eq!(p.entry,0x10400080);assert_eq!(p.phdr,Some(0x10400040));}
#[test]fn entry_must_be_executable(){let mut d=image();d[68..72].copy_from_slice(&4u32.to_le_bytes());assert!(parse(&d).is_err());}
#[test]fn wx_policy(){let mut d=image();d[68..72].copy_from_slice(&7u32.to_le_bytes());assert!(parse(&d).is_err());}
#[test]fn dynamic_elf_without_interp_rejected_when_dt_needed(){
    let mut d=image();
    d[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN
    d[56..58].copy_from_slice(&2u16.to_le_bytes()); // 2 phdrs
    // Seg 1: PT_DYNAMIC at offset 512, vaddr 0x400200, filesz 32, memsz 32
    let ph=120usize;
    d[ph..ph+4].copy_from_slice(&elf::PT_DYNAMIC.to_le_bytes());
    d[ph+4..ph+8].copy_from_slice(&4u32.to_le_bytes());
    d[ph+8..ph+16].copy_from_slice(&512u64.to_le_bytes());
    d[ph+16..ph+24].copy_from_slice(&0x400200u64.to_le_bytes());
    d[ph+32..ph+40].copy_from_slice(&32u64.to_le_bytes());
    d[ph+40..ph+48].copy_from_slice(&32u64.to_le_bytes());
    d[ph+48..ph+56].copy_from_slice(&8u64.to_le_bytes());
    // Static PIE with only DT_NULL (tag 0) succeeds:
    assert!(parse(&d).is_ok());
    // Dynamic binary/library with DT_NEEDED (tag 1) and no PT_INTERP is rejected:
    d[512..520].copy_from_slice(&1u64.to_le_bytes());
    assert!(parse(&d).is_err());
}
#[test]fn pt_phdr_and_relro_validation(){
    let mut d=image();
    d[56..58].copy_from_slice(&2u16.to_le_bytes());
    let ph=120usize;
    // PT_PHDR with wrong offset is rejected
    d[ph..ph+4].copy_from_slice(&elf::PT_PHDR.to_le_bytes());
    d[ph+4..ph+8].copy_from_slice(&4u32.to_le_bytes());
    d[ph+8..ph+16].copy_from_slice(&128u64.to_le_bytes());
    d[ph+16..ph+24].copy_from_slice(&0x400080u64.to_le_bytes());
    d[ph+32..ph+40].copy_from_slice(&112u64.to_le_bytes());
    d[ph+40..ph+48].copy_from_slice(&112u64.to_le_bytes());
    d[ph+48..ph+56].copy_from_slice(&8u64.to_le_bytes());
    assert!(parse(&d).is_err());
    // PT_PHDR matching phoff=64 and vaddr=0x400040 succeeds
    d[ph+8..ph+16].copy_from_slice(&64u64.to_le_bytes());
    d[ph+16..ph+24].copy_from_slice(&0x400040u64.to_le_bytes());
    assert!(parse(&d).is_ok());
}
#[test]fn deterministic_malformed_corpus_never_panics(){let original=image();let mut seed=0x391u64;for _ in 0..20000{let mut d=original.clone();for _ in 0..4{seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;let n=seed as usize%120;d[n]=seed as u8;}let _=parse(&d);}}
