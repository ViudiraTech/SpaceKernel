//! Boot-time assertions that exercise the actual allocator and TTY interfaces.

pub fn run() {
    use crate::{kinfo, mm, printk, tty};
    use alloc::{boxed::Box, vec::Vec};

    let before = mm::pmm::stats().free;
    let frame = mm::pmm::alloc(2).expect("PMM allocation failed");
    // SAFETY: these pages were just allocated and are writable through HHDM.
    unsafe { core::ptr::write_bytes(mm::pmm::phys_to_virt(frame) as *mut u8, 0xa5, 8192) }
    mm::pmm::free(frame, 2).expect("PMM free failed");
    assert_eq!(mm::pmm::stats().free, before);

    let region = mm::vmm::reserve(1).expect("VMM reservation failed");
    assert_eq!(region.pages(), 1);
    let mapped_frame = mm::pmm::alloc(1).expect("VMM backing frame failed");
    mm::vmm::map(region, 0, mapped_frame, false).expect("VMM map failed");
    assert_eq!(mm::vmm::translate(region.base()), Some(mapped_frame));
    assert_eq!(
        mm::vmm::map(region, 0, mapped_frame, false),
        Err(mm::vmm::MapError::AlreadyMapped)
    );
    // SAFETY: VMM mapped the fresh writable frame at this virtual address.
    unsafe { (region.base() as *mut u64).write_volatile(0x1234_5678_9abc_def0) }
    assert_eq!(
        unsafe { (mm::pmm::phys_to_virt(mapped_frame) as *const u64).read_volatile() },
        0x1234_5678_9abc_def0
    );

    let small = Box::new([0x5au8; 128]);
    assert!(small.iter().all(|byte| *byte == 0x5a));
    let mut large = Vec::with_capacity(8192);
    large.resize(8192, 0xa5u8);
    assert!(large.iter().all(|byte| *byte == 0xa5));
    drop(small);
    drop(large);
    let mut line = tty::LineDiscipline::new();
    assert_eq!(
        line.feed(b'a'),
        tty::InputEvent::Echo {
            bytes: [b'a', 0, 0],
            length: 1
        }
    );
    assert_eq!(line.feed(b'\n'), tty::InputEvent::LineReady);
    let mut input = [0; 4];
    assert_eq!(line.read(&mut input), Ok(2));
    assert_eq!(&input[..2], b"a\n");
    let mut no_echo = tty::Termios::default();
    no_echo.echo = false;
    tty::set_termios(tty::TtyDevice::Virtual(2), no_echo).expect("TTY termios failed");
    assert_eq!(
        tty::receive(tty::TtyDevice::Virtual(2), b'x'),
        tty::InputEvent::None
    );
    assert_eq!(
        tty::receive(tty::TtyDevice::Virtual(2), b'\n'),
        tty::InputEvent::LineReady
    );
    assert_eq!(tty::read(tty::TtyDevice::Virtual(2), &mut input), Ok(2));
    assert_eq!(&input[..2], b"x\n");

    // IRQ subsystem and controller self-test
    static IRQ_HIT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    fn test_irq_handler(irq: u32) {
        if irq == 42 {
            IRQ_HIT.store(true, core::sync::atomic::Ordering::Release);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::gic::request(42, test_irq_handler, false).expect("GIC request failed");
        assert_eq!(crate::irq::count(42), Some(0));
        crate::irq::dispatch(42);
        assert!(IRQ_HIT.load(core::sync::atomic::Ordering::Acquire));
        assert_eq!(crate::irq::count(42), Some(1));
        crate::arch::gic::release(42).expect("GIC release failed");
    }
    #[cfg(target_arch = "riscv64")]
    {
        crate::arch::plic::request(42, test_irq_handler).expect("PLIC request failed");
        assert_eq!(crate::irq::count(42), Some(0));
        crate::irq::dispatch(42);
        assert!(IRQ_HIT.load(core::sync::atomic::Ordering::Acquire));
        assert_eq!(crate::irq::count(42), Some(1));
        crate::arch::plic::release(42).expect("PLIC release failed");
    }
    #[cfg(target_arch = "x86_64")]
    {
        crate::irq::register(42, test_irq_handler).expect("IRQ register failed");
        assert_eq!(crate::irq::count(42), Some(0));
        crate::irq::dispatch(42);
        assert!(IRQ_HIT.load(core::sync::atomic::Ordering::Acquire));
        assert_eq!(crate::irq::count(42), Some(1));
        crate::irq::unregister(42).expect("IRQ unregister failed");
    }

    let sequence = printk::next_sequence();
    kinfo!("PMM/VMM/SLAB/TTY/IRQ self-test passed");
    let record = printk::read(sequence).expect("printk record missing");
    assert_eq!(record.sequence, sequence);
}
