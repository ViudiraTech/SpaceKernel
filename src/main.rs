#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

extern crate alloc;

mod arch;
mod boot;
mod hardware;
mod mm;
pub mod printk;
#[cfg(feature = "boot-self-test")]
mod self_test;
mod sync;
mod time;
pub mod tty;

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    arch::disable_interrupts();
    #[cfg(target_arch = "x86_64")]
    arch::serial_init(0);
    let info = boot::read();
    mm::pmm::init(boot::memory_map(), info.hhdm);
    mm::vmm::init();
    #[cfg(not(target_arch = "x86_64"))]
    {
        let uart = mm::vmm::map_device_page(arch::SERIAL_PHYS).expect("UART mapping failed");
        arch::serial_init(uart);
    }
    let prepared_cpus = arch::init_exceptions();
    time::init();
    tty::init();
    kinfo!("SpaceKernel: Limine entry");
    kinfo!("exception state prepared for {} CPUs", prepared_cpus);
    kinfo!("kernel_cmdline: {}", boot::cmdline());
    let console = tty::status();
    kinfo!(
        "console: serial={} fbcon={} tty{} {}x{}",
        console.serial,
        console.framebuffer,
        console.active_vt,
        console.columns,
        console.rows
    );
    if console.invalid_argument {
        kwarn!("invalid console= argument ignored");
    }
    if console.framebuffer_unavailable {
        kwarn!("framebuffer console unavailable; using ttyS0");
    }
    kinfo!(
        "HHDM={:#x} kernel={:#x} phys={:#x}",
        info.hhdm,
        info.kernel_virtual,
        info.kernel_physical
    );
    let stats = mm::pmm::stats();
    kinfo!("PMM: {} free / {} frames", stats.free, stats.frames);
    kinfo!("VMM root={:#x}", mm::vmm::root().unwrap());
    kinfo!(
        "RSDP={:?} DTB={:?}",
        boot::rsdp_address(),
        boot::dtb_address()
    );
    hardware::report();
    #[cfg(feature = "boot-self-test")]
    self_test::run();
    kinfo!("BOOT_OK");
    loop {
        arch::halt();
    }
}

#[alloc_error_handler]
fn allocation_error(layout: core::alloc::Layout) -> ! {
    panic!("kernel heap allocation failed: {layout:?}")
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    arch::disable_interrupts();
    printk::emergency(format_args!("PANIC: {info}"));
    loop {
        arch::halt();
    }
}
