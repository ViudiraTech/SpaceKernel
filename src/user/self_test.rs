/*
 *       src/user/self_test.rs
 *       Real page-table permissions, copies and rollback verification
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use super::{UserAddressSpace, elf, elf_format};
pub fn run() {
    let free = crate::mm::pmm::stats().free;
    {
        let mut s = UserAddressSpace::new().unwrap();
        assert!(
            s.map_zeroed(UserAddressSpace::user_limit(), 4096, 3)
                .is_err()
        );
        s.map_zeroed(0x10000, 8192, 3).unwrap();
        s.write_bytes(0x10fff, b"cross-page").unwrap();
        let mut bytes = [0; 10];
        s.read_bytes(0x10fff, &mut bytes).unwrap();
        assert_eq!(&bytes, b"cross-page");
        assert!(s.validate(0x11fff, 2, false).is_err());
        assert!(s.validate(u64::MAX, 2, false).is_err());
        s.protect(0x11000, 4096, 1).unwrap();
        assert!(s.write_bytes(0x10fff, b"no partial write").is_err());
        let mut first = [0];
        s.read_bytes(0x10fff, &mut first).unwrap();
        assert_eq!(first[0], b'c');
        s.protect(0x11000, 4096, 0).unwrap();
        assert!(s.read_bytes(0x11000, &mut first).is_err());
        s.protect(0x11000, 4096, 3).unwrap();
        s.unmap(0x11000, 4096).unwrap();
        assert!(s.read_bytes(0x11000, &mut first).is_err());
        assert!(s.map_zeroed(0x10000, 4096, 3).is_err());
        s.unmap(0x10000, 4096).unwrap();
        let data = super::test_binaries::TEST_USER_ELF;
        let loaded = elf::load_elf(&mut s, data).unwrap();
        assert!(s.validate(loaded.entry, 1, false).is_ok());
        let mut bad = data.to_vec();
        bad[54..56].fill(0);
        let before = s.resident_pages();
        assert!(elf::load_elf(&mut s, &bad).is_err());
        assert_eq!(before, s.resident_pages());

        // Construct an ELF with PT_LOAD (text + data), PT_GNU_RELRO, and PT_TLS
        let mut custom = alloc::vec![0u8; 12288];
        custom[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        custom[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        let machine = u16::from_le_bytes([data[18], data[19]]);
        custom[18..20].copy_from_slice(&machine.to_le_bytes());
        custom[20..24].copy_from_slice(&1u32.to_le_bytes());
        custom[24..32].copy_from_slice(&0x3000_0000u64.to_le_bytes()); // entry
        custom[32..40].copy_from_slice(&64u64.to_le_bytes()); // phoff
        custom[52..54].copy_from_slice(&64u16.to_le_bytes());
        custom[54..56].copy_from_slice(&56u16.to_le_bytes());
        custom[56..58].copy_from_slice(&4u16.to_le_bytes()); // 4 phdrs
        // Seg 0: PT_LOAD RX at 0x3000_0000 (offset 4096, size 4096)
        let mut ph = 64usize;
        custom[ph..ph + 4].copy_from_slice(&elf_format::PT_LOAD.to_le_bytes());
        custom[ph + 4..ph + 8].copy_from_slice(&5u32.to_le_bytes());
        custom[ph + 8..ph + 16].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 16..ph + 24].copy_from_slice(&0x3000_0000u64.to_le_bytes());
        custom[ph + 32..ph + 40].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 40..ph + 48].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 48..ph + 56].copy_from_slice(&4096u64.to_le_bytes());
        // Seg 1: PT_LOAD RW at 0x3000_1000 (offset 8192, filesz 4096, memsz 8192)
        ph += 56;
        custom[ph..ph + 4].copy_from_slice(&elf_format::PT_LOAD.to_le_bytes());
        custom[ph + 4..ph + 8].copy_from_slice(&6u32.to_le_bytes());
        custom[ph + 8..ph + 16].copy_from_slice(&8192u64.to_le_bytes());
        custom[ph + 16..ph + 24].copy_from_slice(&0x3000_1000u64.to_le_bytes());
        custom[ph + 32..ph + 40].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 40..ph + 48].copy_from_slice(&8192u64.to_le_bytes());
        custom[ph + 48..ph + 56].copy_from_slice(&4096u64.to_le_bytes());
        // Seg 2: PT_GNU_RELRO covering 0x3000_1000..0x3000_2000
        ph += 56;
        custom[ph..ph + 4].copy_from_slice(&elf_format::PT_GNU_RELRO.to_le_bytes());
        custom[ph + 4..ph + 8].copy_from_slice(&4u32.to_le_bytes());
        custom[ph + 8..ph + 16].copy_from_slice(&8192u64.to_le_bytes());
        custom[ph + 16..ph + 24].copy_from_slice(&0x3000_1000u64.to_le_bytes());
        custom[ph + 32..ph + 40].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 40..ph + 48].copy_from_slice(&4096u64.to_le_bytes());
        custom[ph + 48..ph + 56].copy_from_slice(&1u64.to_le_bytes());
        // Seg 3: PT_TLS inside Seg 1 (offset 8192, vaddr 0x3000_1000, filesz 8, memsz 16, align 8)
        ph += 56;
        custom[ph..ph + 4].copy_from_slice(&elf_format::PT_TLS.to_le_bytes());
        custom[ph + 4..ph + 8].copy_from_slice(&4u32.to_le_bytes());
        custom[ph + 8..ph + 16].copy_from_slice(&8192u64.to_le_bytes());
        custom[ph + 16..ph + 24].copy_from_slice(&0x3000_1000u64.to_le_bytes());
        custom[ph + 32..ph + 40].copy_from_slice(&8u64.to_le_bytes());
        custom[ph + 40..ph + 48].copy_from_slice(&16u64.to_le_bytes());
        custom[ph + 48..ph + 56].copy_from_slice(&8u64.to_le_bytes());
        custom[8192..8200].copy_from_slice(&0xdead_beef_cafe_babeu64.to_le_bytes());

        let mut s2 = UserAddressSpace::new().unwrap();
        let loaded2 = elf::load_elf(&mut s2, &custom).unwrap();
        assert_ne!(loaded2.tls_tp, 0);
        // RELRO page 0x3000_1000 must now be read-only and blocked from write escalation
        assert!(s2.validate(0x3000_1000, 8, false).is_ok());
        assert!(s2.validate(0x3000_1000, 8, true).is_err());
        assert!(s2.protect(0x3000_1000, 4096, 3).is_err());
        // Non-RELRO BSS page 0x3000_2000 remains writable
        assert!(s2.validate(0x3000_2000, 8, true).is_ok());
    }
    {
        // CoW fork: both sides start on one physical page, each side's first
        // write gets a private copy, and dropping either side returns the
        // retained frame to the PMM.
        let free_before = crate::mm::pmm::stats().free;
        let mut parent = UserAddressSpace::new().unwrap();
        parent.map_zeroed(0x7000_0000, 8192, 3).unwrap();
        parent.write_bytes(0x7000_0000, b"parent-cow").unwrap();
        let mut child = parent.fork_copy().unwrap();
        let mut bytes = [0u8; 10];
        child.read_bytes(0x7000_0000, &mut bytes).unwrap();
        assert_eq!(&bytes, b"parent-cow");
        parent.write_bytes(0x7000_0000, b"parent-new").unwrap();
        child.read_bytes(0x7000_0000, &mut bytes).unwrap();
        assert_eq!(&bytes, b"parent-cow");
        parent.read_bytes(0x7000_0000, &mut bytes).unwrap();
        assert_eq!(&bytes, b"parent-new");
        child.write_bytes(0x7000_0000, b"child-new").unwrap();
        child.read_bytes(0x7000_0000, &mut bytes[..9]).unwrap();
        assert_eq!(&bytes[..9], b"child-new");
        parent.read_bytes(0x7000_0000, &mut bytes).unwrap();
        assert_eq!(&bytes, b"parent-new");
        drop(child);
        parent.write_bytes(0x7000_0000, b"parent-end").unwrap();
        parent.read_bytes(0x7000_0000, &mut bytes).unwrap();
        assert_eq!(&bytes, b"parent-end");
        drop(parent);
        assert_eq!(
            crate::mm::pmm::stats().free,
            free_before,
            "CoW fork leaked physical frames"
        );
    }
    assert_eq!(
        crate::mm::pmm::stats().free,
        free,
        "user address space leaks physical frames"
    );
    crate::self_test::verbose_log(format_args!("user VM/ELF boundary self-test passed"));
}

pub fn syscalls() {
    use crate::{
        sync::SpinLock,
        syscall::{dispatch, numbers::*},
        vfs::{self, abi},
    };
    use alloc::sync::Arc;
    let task = crate::sched::current().unwrap();
    let old = task.address_space();
    let scratch = Arc::new(SpinLock::new(UserAddressSpace::new().unwrap()));
    scratch.lock().map_zeroed(0x10000, 16384, 3).unwrap();
    task.lock().set_address_space(scratch.clone());
    assert_eq!(
        dispatch(SYS_WRITE, 1, UserAddressSpace::user_limit(), 8, 0, 0, 0),
        -abi::EFAULT
    );
    let address = dispatch(SYS_MMAP, 0, 8192, 3, 0x22, u64::MAX, 0);
    assert!(address > 0);
    scratch
        .lock()
        .write_bytes(address as u64 + 4095, b"mapped cross page")
        .unwrap();
    assert_eq!(dispatch(SYS_MPROTECT, address as u64, 8192, 0, 0, 0, 0), 0);
    assert_eq!(
        dispatch(SYS_WRITE, 1, address as u64, 1, 0, 0, 0),
        -abi::EFAULT
    );
    assert_eq!(dispatch(SYS_MPROTECT, address as u64, 8192, 3, 0, 0, 0), 0);
    assert_eq!(dispatch(SYS_MUNMAP, address as u64, 8192, 0, 0, 0, 0), 0);
    assert!(scratch.lock().validate(address as u64, 1, false).is_err());
    let base = dispatch(SYS_BRK, 0, 0, 0, 0, 0, 0) as u64;
    assert_eq!(
        dispatch(SYS_BRK, base + 8192, 0, 0, 0, 0, 0) as u64,
        base + 8192
    );
    assert!(scratch.lock().validate(base, 8192, true).is_ok());
    assert_eq!(dispatch(SYS_BRK, base, 0, 0, 0, 0, 0) as u64, base);

    // Verify MAP_FIXED failure rollback preserves existing mapping
    let keep = dispatch(SYS_MMAP, 0, 4096, 3, 0x22, u64::MAX, 0) as u64;
    assert!(keep >= 0x4000_0000);
    scratch.lock().write_bytes(keep, b"keep-alive").unwrap();
    // MAP_FIXED with invalid fd (-1) and non-anonymous flags (0x12) must fail before unmapping `keep`
    assert_eq!(
        dispatch(SYS_MMAP, keep, 4096, 3, 0x12, u64::MAX, 0),
        -abi::EBADF
    );
    let mut check_keep = [0u8; 10];
    scratch.lock().read_bytes(keep, &mut check_keep).unwrap();
    assert_eq!(&check_keep, b"keep-alive");
    assert_eq!(dispatch(SYS_MUNMAP, keep, 4096, 0, 0, 0, 0), 0);

    // Verify pipe2 flags, O_NONBLOCK, poll, and readv/writev edge cases
    assert_eq!(
        dispatch(SYS_PIPE2, 0x10000, 0xffff, 0, 0, 0, 0),
        -abi::EINVAL
    );
    assert_eq!(
        dispatch(SYS_PIPE2, 0x10000, abi::O_NONBLOCK as u64, 0, 0, 0, 0),
        0
    );
    let mut fds = [0i32; 2];
    scratch
        .lock()
        .read_bytes(0x10000, unsafe {
            core::slice::from_raw_parts_mut(fds.as_mut_ptr() as *mut u8, 8)
        })
        .unwrap();
    assert_eq!(
        dispatch(SYS_READ, fds[0] as u64, 0x10100, 4, 0, 0, 0),
        -abi::EAGAIN
    );
    assert_eq!(dispatch(SYS_READV, fds[0] as u64, 0, 0, 0, 0, 0), 0);
    assert_eq!(
        dispatch(SYS_READV, fds[0] as u64, 0, 1, 0, 0, 0),
        -abi::EFAULT
    );
    assert_eq!(dispatch(SYS_CLOSE, fds[0] as u64, 0, 0, 0, 0, 0), 0);
    assert_eq!(dispatch(SYS_CLOSE, fds[1] as u64, 0, 0, 0, 0, 0), 0);

    // eventfd2: counter, semaphore mode, nonblocking EAGAIN and poll readiness
    assert_eq!(
        dispatch(SYS_EVENTFD2, 0, 0xffff_ffff, 0, 0, 0, 0),
        -abi::EINVAL
    );
    let efd = dispatch(SYS_EVENTFD2, 0, 0, 0, 0, 0, 0);
    assert!(efd >= 0);
    scratch
        .lock()
        .write_bytes(0x10200, &5u64.to_le_bytes())
        .unwrap();
    assert_eq!(dispatch(SYS_WRITE, efd as u64, 0x10200, 8, 0, 0, 0), 8);
    assert_eq!(dispatch(SYS_READ, efd as u64, 0x10208, 8, 0, 0, 0), 8);
    let mut event_value = [0u8; 8];
    scratch
        .lock()
        .read_bytes(0x10208, &mut event_value)
        .unwrap();
    assert_eq!(u64::from_le_bytes(event_value), 5);
    #[cfg(target_arch = "x86_64")]
    {
        scratch
            .lock()
            .write_bytes(0x10200, &3u64.to_le_bytes())
            .unwrap();
        assert_eq!(dispatch(SYS_WRITE, efd as u64, 0x10200, 8, 0, 0, 0), 8);
        scratch
            .lock()
            .write_bytes(0x10210, &(efd as i32).to_le_bytes())
            .unwrap();
        scratch
            .lock()
            .write_bytes(0x10214, &1u16.to_le_bytes())
            .unwrap();
        assert_eq!(dispatch(SYS_POLL, 0x10210, 1, 1000, 0, 0, 0), 1);
    }
    assert_eq!(dispatch(SYS_CLOSE, efd as u64, 0, 0, 0, 0, 0), 0);

    let empty = dispatch(SYS_EVENTFD2, 0, abi::O_NONBLOCK as u64, 0, 0, 0, 0);
    assert!(empty >= 0);
    assert_eq!(
        dispatch(SYS_READ, empty as u64, 0x10220, 8, 0, 0, 0),
        -abi::EAGAIN
    );
    assert_eq!(dispatch(SYS_CLOSE, empty as u64, 0, 0, 0, 0, 0), 0);

    let sem = dispatch(SYS_EVENTFD2, 3, 1, 0, 0, 0, 0);
    assert!(sem >= 0);
    assert_eq!(dispatch(SYS_READ, sem as u64, 0x10228, 8, 0, 0, 0), 8);
    scratch
        .lock()
        .read_bytes(0x10228, &mut event_value)
        .unwrap();
    assert_eq!(u64::from_le_bytes(event_value), 1);
    assert_eq!(dispatch(SYS_CLOSE, sem as u64, 0, 0, 0, 0, 0), 0);

    // Verify getrandom matches hardware RNG support without hanging
    let gr = dispatch(SYS_GETRANDOM, 0x10200, 16, 0, 0, 0, 0);
    if crate::random::supported() {
        assert_eq!(gr, 16);
    } else {
        assert_eq!(gr, -abi::EAGAIN);
    }

    vfs::write_file("/tmp/invalid-exec", b"not ELF").unwrap();
    vfs::lookup_abs("/tmp/invalid-exec", true)
        .unwrap()
        .0
        .chmod(0o755)
        .unwrap();
    scratch
        .lock()
        .write_bytes(0x10000, b"/tmp/invalid-exec\0")
        .unwrap();
    let root = scratch.lock().root_physical();
    assert_eq!(dispatch(SYS_EXECVE, 0x10000, 0, 0, 0, 0, 0), -8);
    assert_eq!(task.address_space().unwrap().lock().root_physical(), root);
    vfs::unlink_abs("/tmp/invalid-exec", false).unwrap();
    task.lock().address_space = old.clone();
    if let Some(old) = old {
        old.lock().activate();
    } else {
        UserAddressSpace::deactivate();
    }
    drop(scratch);
    crate::self_test::verbose_log(format_args!(
        "user mmap/mprotect/brk/EFAULT/exec rollback self-test passed"
    ));
}
