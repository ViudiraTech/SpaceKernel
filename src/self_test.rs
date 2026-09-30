/*
 *
 *       src/self_test.rs
 *       Boot-time memory, CPU, interrupt and firmware validation
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use alloc::{boxed::Box, vec::Vec};

pub fn run() {
    use crate::{kinfo, mm, printk, tty};

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
    let no_echo = tty::Termios {
        echo: false,
        ..tty::Termios::default()
    };
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

    test_fdt();
    crate::pci::self_test();
    crate::fpu::self_test();
    test_timer_interrupt();

    // Test rejection before any page tables or device mappings are published.
    assert_eq!(
        mm::vmm::map_device_range(u64::MAX, 2),
        Err(mm::vmm::MapError::OutOfRange)
    );
    assert_eq!(
        mm::vmm::map_device_range(0, 0),
        Err(mm::vmm::MapError::OutOfRange)
    );
    assert_eq!(
        mm::vmm::map(region, 1, mapped_frame, false),
        Err(mm::vmm::MapError::OutOfRange)
    );
    assert_eq!(
        mm::vmm::map(region, 0, mapped_frame + 1, false),
        Err(mm::vmm::MapError::Unaligned)
    );
    assert_eq!(
        mm::vmm::translate(region.base() + 7),
        Some(mapped_frame + 7)
    );
    for (levels, bits) in [(3, 39), (4, 48)] {
        let geometry = crate::arch::paging::Geometry {
            levels,
            virtual_bits: bits,
            physical_bits: 56,
        };
        assert_eq!(geometry.slot_base(256), !((1u64 << (bits - 1)) - 1));
        assert_eq!(
            geometry
                .slot_base(510)
                .checked_add(geometry.slot_bytes())
                .unwrap(),
            geometry.slot_base(511)
        );
    }

    let sequence = printk::next_sequence();
    kinfo!("PMM/VMM/SLAB/TTY/IRQ/FDT self-test passed");
    let record = printk::read(sequence).expect("printk record missing");
    assert_eq!(record.sequence, sequence);
}

/// Verify an actual interrupt entry/return while the FP gate is closed. This
/// catches accidental SIMD use by Rust handlers and missing PPI programming.
fn test_timer_interrupt() {
    use crate::arch;
    #[cfg(any(target_arch = "x86_64", target_arch = "riscv64"))]
    use crate::irq;
    use core::sync::atomic::{AtomicBool, Ordering};
    static HIT: AtomicBool = AtomicBool::new(false);
    fn handler(_irq: u32) {
        #[cfg(target_arch = "aarch64")]
        crate::arch::gic::timer::cancel(); // Deassert the level source before EOI.
        HIT.store(true, Ordering::Release);
    }
    #[cfg(target_arch = "x86_64")]
    {
        if arch::apic::local_id().is_none() {
            return;
        }
        irq::register(0xf0, handler).unwrap();
        arch::apic::timer::arm_apic_ticks(10000).unwrap();
    }
    #[cfg(target_arch = "aarch64")]
    {
        if !arch::gic::ready() {
            return;
        }
        arch::gic::request(arch::gic::timer::TIMER_INTID, handler, false).unwrap();
        arch::gic::timer::arm_after_micros(1000).unwrap();
    }
    #[cfg(target_arch = "riscv64")]
    {
        irq::register(arch::clint::TIMER_IRQ, handler).unwrap();
        arch::clint::arm_after_micros(1000).unwrap();
    }
    arch::enable_interrupts();
    let mut received = false;
    for _ in 0..10_000_000 {
        if HIT.load(Ordering::Acquire) {
            received = true;
            break;
        }
        core::hint::spin_loop();
    }
    arch::disable_interrupts();
    #[cfg(target_arch = "x86_64")]
    {
        arch::apic::timer::cancel();
        irq::unregister(0xf0).unwrap();
    }
    #[cfg(target_arch = "aarch64")]
    {
        arch::gic::timer::cancel();
        arch::gic::release(arch::gic::timer::TIMER_INTID).unwrap();
    }
    #[cfg(target_arch = "riscv64")]
    {
        arch::clint::cancel();
        irq::unregister(arch::clint::TIMER_IRQ).unwrap();
    }
    assert!(
        received,
        "timer did not reach the architecture interrupt handler"
    );
    assert!(!arch::fpu::is_enabled());
    crate::kinfo!("hardware timer IRQ self-test passed");
}

struct DtbBuilder {
    struct_data: Vec<u8>,
    string_data: Vec<u8>,
    rsv_data: Vec<u8>,
}

impl DtbBuilder {
    fn new() -> Self {
        Self {
            struct_data: Vec::new(),
            string_data: Vec::new(),
            rsv_data: Vec::new(),
        }
    }

    fn add_reservation(&mut self, addr: u64, size: u64) {
        self.rsv_data.extend_from_slice(&addr.to_be_bytes());
        self.rsv_data.extend_from_slice(&size.to_be_bytes());
    }

    fn begin_node(&mut self, name: &str) {
        self.struct_data.extend_from_slice(&1u32.to_be_bytes()); // FDT_BEGIN_NODE
        self.struct_data.extend_from_slice(name.as_bytes());
        self.struct_data.push(0);
        while !self.struct_data.len().is_multiple_of(4) {
            self.struct_data.push(0);
        }
    }

    fn end_node(&mut self) {
        self.struct_data.extend_from_slice(&2u32.to_be_bytes()); // FDT_END_NODE
    }

    fn prop_bytes(&mut self, name: &str, val: &[u8]) {
        let name_off = self.string_data.len() as u32;
        self.string_data.extend_from_slice(name.as_bytes());
        self.string_data.push(0);

        self.struct_data.extend_from_slice(&3u32.to_be_bytes()); // FDT_PROP
        self.struct_data
            .extend_from_slice(&(val.len() as u32).to_be_bytes());
        self.struct_data.extend_from_slice(&name_off.to_be_bytes());
        self.struct_data.extend_from_slice(val);
        while !self.struct_data.len().is_multiple_of(4) {
            self.struct_data.push(0);
        }
    }

    fn prop_u32(&mut self, name: &str, val: u32) {
        self.prop_bytes(name, &val.to_be_bytes());
    }

    fn prop_str(&mut self, name: &str, val: &str) {
        let mut b = Vec::from(val.as_bytes());
        b.push(0);
        self.prop_bytes(name, &b);
    }

    fn build(mut self) -> Vec<u8> {
        self.struct_data.extend_from_slice(&9u32.to_be_bytes()); // FDT_END
        self.add_reservation(0, 0); // terminator

        let header_size = 40usize;
        let rsv_offset = header_size;
        let struct_offset = rsv_offset + self.rsv_data.len();
        let strings_offset = struct_offset + self.struct_data.len();
        let total_size = strings_offset + self.string_data.len();

        let mut out = Vec::with_capacity(total_size);
        out.extend_from_slice(&0xd00d_feedu32.to_be_bytes());
        out.extend_from_slice(&(total_size as u32).to_be_bytes());
        out.extend_from_slice(&(struct_offset as u32).to_be_bytes());
        out.extend_from_slice(&(strings_offset as u32).to_be_bytes());
        out.extend_from_slice(&(rsv_offset as u32).to_be_bytes());
        out.extend_from_slice(&17u32.to_be_bytes()); // version
        out.extend_from_slice(&16u32.to_be_bytes()); // last_comp_version
        out.extend_from_slice(&0u32.to_be_bytes()); // boot_cpuid_phys
        out.extend_from_slice(&(self.string_data.len() as u32).to_be_bytes());
        out.extend_from_slice(&(self.struct_data.len() as u32).to_be_bytes());

        out.extend_from_slice(&self.rsv_data);
        out.extend_from_slice(&self.struct_data);
        out.extend_from_slice(&self.string_data);
        out
    }
}

fn test_fdt() {
    use crate::hardware::fdt::{Fdt, FdtError};

    let mut b = DtbBuilder::new();
    b.add_reservation(0x8000_0000, 0x1000);

    // root node
    b.begin_node("");
    b.prop_u32("#address-cells", 2);
    b.prop_u32("#size-cells", 2);
    b.prop_str("model", "SpaceKernel-Virt");
    b.prop_bytes("compatible", b"spacekernel,test\0qemu,virt\0");

    // chosen
    b.begin_node("chosen");
    b.prop_str("stdout-path", "/soc/uart@10000000");
    b.prop_str("bootargs", "console=ttyS0 root=/dev/ram0");
    b.end_node(); // chosen

    // memory@80000000
    b.begin_node("memory@80000000");
    b.prop_str("device_type", "memory");
    let mut mem_reg = Vec::new();
    mem_reg.extend_from_slice(&0x00000000_80000000u64.to_be_bytes());
    mem_reg.extend_from_slice(&0x00000000_20000000u64.to_be_bytes());
    b.prop_bytes("reg", &mem_reg);
    b.end_node(); // memory

    // soc bus
    b.begin_node("soc");
    b.prop_u32("#address-cells", 1);
    b.prop_u32("#size-cells", 1);
    b.prop_bytes("ranges", &[]); // 1:1 identity

    // gic interrupt controller
    b.begin_node("interrupt-controller@8000000");
    b.prop_str("compatible", "arm,gic-v3");
    b.prop_u32("phandle", 1);
    let mut gic_reg = Vec::new();
    gic_reg.extend_from_slice(&0x0800_0000u32.to_be_bytes());
    gic_reg.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    gic_reg.extend_from_slice(&0x080a_0000u32.to_be_bytes());
    gic_reg.extend_from_slice(&0x0020_0000u32.to_be_bytes());
    b.prop_bytes("reg", &gic_reg);
    b.end_node(); // interrupt-controller

    // uart@10000000
    b.begin_node("uart@10000000");
    b.prop_str("compatible", "ns16550a");
    b.prop_u32("phandle", 2);
    b.prop_u32("interrupt-parent", 1);
    let mut uart_reg = Vec::new();
    uart_reg.extend_from_slice(&0x1000_0000u32.to_be_bytes());
    uart_reg.extend_from_slice(&0x0000_1000u32.to_be_bytes());
    b.prop_bytes("reg", &uart_reg);
    b.end_node(); // uart

    // pcie@3f000000
    b.begin_node("pcie@3f000000");
    b.prop_str("compatible", "pci-host-ecam-generic");
    let mut pci_reg = Vec::new();
    pci_reg.extend_from_slice(&0x3f00_0000u32.to_be_bytes());
    pci_reg.extend_from_slice(&0x0100_0000u32.to_be_bytes());
    b.prop_bytes("reg", &pci_reg);
    let mut bus_range = Vec::new();
    bus_range.extend_from_slice(&0u32.to_be_bytes());
    bus_range.extend_from_slice(&15u32.to_be_bytes());
    b.prop_bytes("bus-range", &bus_range);
    b.end_node(); // pcie

    b.end_node(); // soc
    b.end_node(); // root

    let blob = b.build();
    let fdt = Fdt::from_slice(&blob).expect("FDT slice parse failed");

    // Test header & reservations
    assert_eq!(fdt.header().version, 17);
    let rsv: Vec<_> = fdt.memory_reservations().collect();
    assert_eq!(rsv.len(), 1);
    assert_eq!(rsv[0].address, 0x8000_0000);
    assert_eq!(rsv[0].size, 0x1000);

    // Test root & chosen
    let root = fdt.root().expect("root missing");
    assert_eq!(
        root.property("model").unwrap().as_str(),
        Some("SpaceKernel-Virt")
    );
    let chosen = fdt.chosen().expect("chosen missing");
    assert_eq!(chosen.stdout_path(), Some("/soc/uart@10000000"));
    assert_eq!(chosen.bootargs(), Some("console=ttyS0 root=/dev/ram0"));

    // Test hierarchical navigation
    let uart_by_path = fdt
        .find_node("/soc/uart@10000000")
        .expect("uart by full path");
    let uart_by_short = fdt.find_node("/soc/uart").expect("uart by short path");
    assert_eq!(uart_by_path.name, "uart@10000000");
    assert_eq!(uart_by_short.name_without_unit(), "uart");
    assert_eq!(uart_by_path.unit_address(), Some("10000000"));
    assert_eq!(uart_by_path.phandle(), Some(2));
    assert_eq!(uart_by_path.interrupt_parent(), Some(1));

    // Test parent cell inheritance and reg decoding
    // UART under soc (1 cell addr, 1 cell size) -> 0x1000_0000, 0x1000
    let uart_regs: Vec<_> = uart_by_path.reg().unwrap().collect();
    assert_eq!(uart_regs.len(), 1);
    assert_eq!(uart_regs[0].address, 0x1000_0000);
    assert_eq!(uart_regs[0].size, 0x1000);

    // Memory under root (2 cell addr, 2 cell size) -> 0x8000_0000, 0x2000_0000
    let mem_regions: Vec<_> = fdt.memory_regions().collect();
    assert_eq!(mem_regions.len(), 1);
    assert_eq!(mem_regions[0].address, 0x8000_0000);
    assert_eq!(mem_regions[0].size, 0x2000_0000);

    // Test phandle lookup
    let gic_by_phandle = fdt.find_by_phandle(1).expect("find_by_phandle 1");
    assert!(gic_by_phandle.is_compatible("arm,gic-v3"));
    let gic_regs: Vec<_> = gic_by_phandle.reg().unwrap().collect();
    assert_eq!(gic_regs.len(), 2);
    assert_eq!(gic_regs[0].address, 0x0800_0000);
    assert_eq!(gic_regs[0].size, 0x10000);
    assert_eq!(gic_regs[1].address, 0x080a_0000);
    assert_eq!(gic_regs[1].size, 0x200000);

    // Test find_compatible for PCIe
    let pci_node = fdt
        .find_compatible("pci-host-ecam-generic")
        .expect("pcie node");
    let pci_reg = pci_node.reg().unwrap().next().unwrap();
    assert_eq!(pci_reg.address, 0x3f00_0000);
    assert_eq!(pci_reg.size, 0x0100_0000);

    // Test error cases
    assert_eq!(Fdt::from_slice(&[0u8; 10]), Err(FdtError::Length));
    let mut bad_magic = blob.clone();
    bad_magic[0..4].copy_from_slice(&0xdead_beefu32.to_be_bytes());
    assert_eq!(Fdt::from_slice(&bad_magic), Err(FdtError::Magic));
}
