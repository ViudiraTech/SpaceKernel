use core::sync::atomic::{AtomicUsize, Ordering};

pub const SERIAL_PHYS: u64 = 0x1000_0000;
static SERIAL: AtomicUsize = AtomicUsize::new(0);

pub fn serial_init(virtual_address: u64) {
    SERIAL.store(virtual_address as usize, Ordering::Release);
}

pub fn serial_write(byte: u8) {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: the VMM maps the QEMU virt 16550 UART as device memory.
    unsafe {
        while ((base + 5) as *const u8).read_volatile() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        (base as *mut u8).write_volatile(byte);
    }
}

pub fn serial_try_read() -> Option<u8> {
    let base = SERIAL.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    // SAFETY: the same 16550 device page used by serial_write is mapped.
    unsafe {
        (((base + 5) as *const u8).read_volatile() & 1 != 0)
            .then(|| (base as *const u8).read_volatile())
    }
}
