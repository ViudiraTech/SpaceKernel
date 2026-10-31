/*
 *
 *       src/input/virtio.rs
 *       VirtIO input device driver (keyboard, tablet, mouse) over modern PCI
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! VirtIO 1.3 section 5.8. The device speaks evdev natively: its config space
//! describes capabilities with the same type and code numbers Linux uses, and
//! each event-queue buffer carries one `{type, code, value}` record.

use super::{AbsInfo, BUS_VIRTUAL, Capabilities, EV_ABS, EV_KEY, EV_REL, EV_SYN, InputDevice};
use crate::{
    mm::{pmm, vmm},
    pci::{self, Bar, DeviceInfo},
    sync::SpinLock,
    vfs::VfsError,
};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{Ordering, fence};

const QUEUE_SIZE: usize = 64;
const EVENT_SIZE: usize = 8;
const PCI_DEVICE_INPUT: u16 = 0x1052;

const CFG_ID_NAME: u8 = 0x01;
const CFG_ID_SERIAL: u8 = 0x02;
const CFG_ID_DEVIDS: u8 = 0x03;
const CFG_PROP_BITS: u8 = 0x10;
const CFG_EV_BITS: u8 = 0x11;
const CFG_ABS_INFO: u8 = 0x12;

fn rd8(address: u64) -> u8 {
    // SAFETY: address lies in a mapped virtio capability window.
    unsafe { (address as *const u8).read_volatile() }
}
fn rd16(address: u64) -> u16 {
    // SAFETY: as above.
    u16::from_le(unsafe { (address as *const u16).read_volatile() })
}
fn rd32(address: u64) -> u32 {
    // SAFETY: as above.
    u32::from_le(unsafe { (address as *const u32).read_volatile() })
}
fn wr8(address: u64, value: u8) {
    // SAFETY: as above.
    unsafe { (address as *mut u8).write_volatile(value) }
}
fn wr16(address: u64, value: u16) {
    // SAFETY: as above.
    unsafe { (address as *mut u16).write_volatile(value.to_le()) }
}
fn wr32(address: u64, value: u32) {
    // SAFETY: as above.
    unsafe { (address as *mut u32).write_volatile(value.to_le()) }
}
fn wr64(address: u64, value: u64) {
    wr32(address, value as u32);
    wr32(address + 4, (value >> 32) as u32);
}

/// Physically contiguous, zeroed, permanently owned DMA pages.
struct DmaPages {
    physical: u64,
    virtual_address: u64,
}

impl DmaPages {
    fn new(pages: usize) -> Result<Self, VfsError> {
        let physical = pmm::alloc(pages).ok_or(VfsError::NoMemory)?;
        let virtual_address = pmm::phys_to_virt(physical) as u64;
        // SAFETY: exclusively owned, freshly allocated frames.
        unsafe { core::ptr::write_bytes(virtual_address as *mut u8, 0, pages * 4096) };
        Ok(Self {
            physical,
            virtual_address,
        })
    }
}

/// Queue of device-writable event buffers.
struct EventQueue {
    ring: DmaPages,
    buffers: DmaPages,
    size: usize,
    available: u16,
    used: u16,
    notify: u64,
    index: u16,
}

// SAFETY: the raw addresses name permanently owned DMA memory and MMIO.
unsafe impl Send for EventQueue {}

impl EventQueue {
    const AVAIL: u64 = 4096;
    const USED: u64 = 8192;

    fn new(common: u64, notify: u64, multiplier: u32, index: u16) -> Result<Self, VfsError> {
        wr16(common + 22, index);
        let maximum = rd16(common + 24) as usize;
        if maximum < 2 {
            return Err(VfsError::NoDevice);
        }
        let size = QUEUE_SIZE.min(1usize << maximum.ilog2());
        let ring = DmaPages::new(3)?;
        let buffers = DmaPages::new(1)?;
        wr16(common + 24, size as u16);
        wr16(common + 26, 0);
        if rd16(common + 26) == 0xffff {
            return Err(VfsError::IoError);
        }
        wr64(common + 32, ring.physical);
        wr64(common + 40, ring.physical + Self::AVAIL);
        wr64(common + 48, ring.physical + Self::USED);
        let offset = u64::from(rd16(common + 30)) * u64::from(multiplier);
        wr16(common + 28, 1);
        let mut queue = Self {
            ring,
            buffers,
            size,
            available: 0,
            used: 0,
            notify: notify + offset,
            index,
        };
        for slot in 0..size {
            queue.write_descriptor(slot);
            queue.post(slot as u16);
        }
        fence(Ordering::SeqCst);
        wr16(queue.notify, index);
        Ok(queue)
    }

    fn write_descriptor(&self, slot: usize) {
        let base = self.ring.virtual_address + slot as u64 * 16;
        let buffer = self.buffers.physical + (slot * EVENT_SIZE) as u64;
        // SAFETY: slot < size <= QUEUE_SIZE, so the entry lies in the first ring page.
        unsafe {
            (base as *mut u64).write_volatile(buffer.to_le());
            ((base + 8) as *mut u32).write_volatile((EVENT_SIZE as u32).to_le());
            ((base + 12) as *mut u16).write_volatile(2u16.to_le()); // VRING_DESC_F_WRITE
            ((base + 14) as *mut u16).write_volatile(0);
        }
    }

    fn post(&mut self, slot: u16) {
        let avail = self.ring.virtual_address + Self::AVAIL;
        let entry = avail + 4 + (self.available as usize % self.size * 2) as u64;
        // SAFETY: the available ring lives in the second owned ring page.
        unsafe { (entry as *mut u16).write_volatile(slot.to_le()) };
        self.available = self.available.wrapping_add(1);
        fence(Ordering::SeqCst);
        wr16(avail + 2, self.available);
    }

    /// Hand each completed event to `sink`, then repost its buffer.
    fn drain(&mut self, mut sink: impl FnMut(u16, u16, i32)) {
        let used = self.ring.virtual_address + Self::USED;
        let mut reposted = false;
        loop {
            fence(Ordering::Acquire);
            let index = rd16(used + 2);
            if index == self.used {
                break;
            }
            let entry = used + 4 + (self.used as usize % self.size * 8) as u64;
            let slot = rd32(entry) as usize;
            self.used = self.used.wrapping_add(1);
            if slot >= self.size {
                continue;
            }
            let record = self.buffers.virtual_address + (slot * EVENT_SIZE) as u64;
            sink(rd16(record), rd16(record + 2), rd32(record + 4) as i32);
            self.post(slot as u16);
            reposted = true;
        }
        if reposted {
            fence(Ordering::SeqCst);
            wr16(self.notify, self.index);
        }
    }
}

struct VirtioInput {
    isr: u64,
    queue: SpinLock<EventQueue>,
    device: Arc<InputDevice>,
}

static DEVICES: SpinLock<Vec<Arc<VirtioInput>>> = SpinLock::new(Vec::new());

fn interrupt(_vector: u32) {
    let devices = DEVICES.lock().clone();
    for entry in devices {
        let _cause = rd8(entry.isr);
        let mut queue = entry.queue.lock();
        let device = entry.device.clone();
        queue.drain(|kind, code, value| {
            // EV_SYN frames pass through verbatim to keep device packet boundaries.
            if kind == EV_SYN {
                device.sync();
            } else {
                device.report(kind, code, value);
            }
        });
    }
}

fn bar_span(device: &DeviceInfo, index: usize) -> Result<u64, VfsError> {
    match device.bars.get(index).copied().flatten() {
        Some(Bar::Memory32 { base, .. }) if base != 0 => Ok(u64::from(base)),
        Some(Bar::Memory64 { base, .. }) if base != 0 => Ok(base),
        _ => Err(VfsError::NoDevice),
    }
}

struct Windows {
    common: u64,
    notify: u64,
    isr: u64,
    config: u64,
    multiplier: u32,
}

fn map_windows(device: &DeviceInfo, caps: &[pci::Capability]) -> Result<Windows, VfsError> {
    let mut windows = Windows {
        common: 0,
        notify: 0,
        isr: 0,
        config: 0,
        multiplier: 0,
    };
    let io = |_: crate::pci::PciError| VfsError::IoError;
    for cap in caps.iter().filter(|cap| cap.id == 9) {
        let cfg = pci::config_for(device.address).map_err(io)?;
        let offset = cap.offset as u16;
        let kind = cfg.read_u8(offset + 3).map_err(io)?;
        if !matches!(kind, 1..=4) {
            continue;
        }
        let bar = cfg.read_u8(offset + 4).map_err(io)? as usize;
        let within = u64::from(cfg.read_u32(offset + 8).map_err(io)?);
        let length = u64::from(cfg.read_u32(offset + 12).map_err(io)?);
        if length == 0 || length > 1024 * 1024 {
            continue;
        }
        let base = bar_span(device, bar)?;
        let mapped =
            vmm::map_device_range(base + within, length as usize).map_err(|_| VfsError::IoError)?;
        match kind {
            1 => windows.common = mapped,
            2 => {
                windows.notify = mapped;
                windows.multiplier = cfg.read_u32(offset + 16).map_err(io)?;
            }
            3 => windows.isr = mapped,
            4 => windows.config = mapped,
            _ => {}
        }
    }
    if windows.common == 0 || windows.notify == 0 || windows.isr == 0 || windows.config == 0 {
        return Err(VfsError::NoDevice);
    }
    Ok(windows)
}

/// Select a config field and return its payload.
fn config_read(config: u64, select: u8, subsel: u8) -> Vec<u8> {
    wr8(config, select);
    wr8(config + 1, subsel);
    fence(Ordering::SeqCst);
    let size = rd8(config + 2) as usize;
    (0..size.min(128))
        .map(|index| rd8(config + 8 + index as u64))
        .collect()
}

fn config_string(config: u64, select: u8) -> String {
    let bytes = config_read(config, select, 0);
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn describe(config: u64, device: &DeviceInfo, position: usize) -> Capabilities {
    let name = config_string(config, CFG_ID_NAME);
    let name = if name.is_empty() {
        String::from("VirtIO Input")
    } else {
        name
    };
    let ids = config_read(config, CFG_ID_DEVIDS, 0);
    let word = |index: usize| -> u16 {
        ids.get(index..index + 2)
            .map_or(0, |raw| u16::from_le_bytes([raw[0], raw[1]]))
    };
    let phys = format!("virtio{}/input0", position);
    let mut caps = Capabilities::new(&name, &phys, BUS_VIRTUAL, word(2), word(4));
    caps.version = word(6);
    caps.uniq = config_string(config, CFG_ID_SERIAL);
    let _ = device;
    for (index, byte) in config_read(config, CFG_PROP_BITS, 0).iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                caps.properties.set(index * 8 + bit);
            }
        }
    }
    for event in 1u16..=0x1f {
        let bits = config_read(config, CFG_EV_BITS, event as u8);
        if bits.is_empty() {
            continue;
        }
        caps.ev.set(event as usize);
        let map = match event {
            EV_KEY => Some(&mut caps.key),
            EV_REL => Some(&mut caps.rel),
            EV_ABS => Some(&mut caps.abs),
            super::EV_MSC => Some(&mut caps.msc),
            super::EV_LED => Some(&mut caps.led),
            super::EV_SW => Some(&mut caps.sw),
            super::EV_SND => Some(&mut caps.snd),
            _ => None,
        };
        if let Some(map) = map {
            for (index, byte) in bits.iter().enumerate() {
                for bit in 0..8 {
                    if byte & (1 << bit) != 0 {
                        map.set(index * 8 + bit);
                    }
                }
            }
        }
    }
    if caps.ev.get(EV_ABS as usize) {
        for axis in 0..=super::ABS_MAX {
            if caps.abs.get(axis) {
                let raw = config_read(config, CFG_ABS_INFO, axis as u8);
                if raw.len() >= 20 {
                    let field = |index: usize| {
                        i32::from_le_bytes(raw[index * 4..index * 4 + 4].try_into().unwrap())
                    };
                    caps.abs_info[axis] = AbsInfo {
                        value: 0,
                        minimum: field(0),
                        maximum: field(1),
                        fuzz: field(2),
                        flat: field(3),
                        resolution: field(4),
                    };
                }
            }
        }
    }
    // Absolute pointing devices are direct pointers to libinput only when the
    // host says so; QEMU's tablet does not, and it needs the pointer property.
    if caps.ev.get(EV_ABS as usize) {
        caps.properties.set(0);
    }
    // QEMU's virtio-tablet reuses the multitouch button table, so EV_KEY
    // advertises BTN_TOUCH even though the tablet only ever emits mouse
    // buttons and ABS_X/ABS_Y. libudev-zero checks BTN_TOUCH before BTN_MOUSE
    // and would tag that pointer as a touchscreen (libinput then complains
    // about pointer events on a device without CAP_POINTER). Linux's own
    // input_id calls this an absolute mouse, so expose exactly the buttons the
    // device can emit: keep BTN_TOUCH only for real multitouch or direct
    // touch devices, and keep pen/stylus tool bits for tablets.
    const ABS_X: usize = 0x00;
    const ABS_Y: usize = 0x01;
    const ABS_MT_POSITION_X: usize = 0x35;
    const ABS_MT_POSITION_Y: usize = 0x36;
    const BTN_TOOL_FINGER: usize = 0x145;
    const BTN_TOOL_QUINTTAP: usize = 0x148;
    const BTN_TOOL_DOUBLETAP: usize = 0x14d;
    const BTN_TOOL_TRIPLETAP: usize = 0x14e;
    const BTN_TOOL_QUADTAP: usize = 0x14f;
    const BTN_TOUCH: usize = 0x14a;
    const BTN_MOUSE: usize = 0x110;
    const INPUT_PROP_DIRECT: usize = 0x01;
    let absolute_pointer = caps.ev.get(EV_ABS as usize)
        && caps.abs.get(ABS_X)
        && caps.abs.get(ABS_Y)
        && !(caps.abs.get(ABS_MT_POSITION_X) && caps.abs.get(ABS_MT_POSITION_Y))
        && !caps.properties.get(INPUT_PROP_DIRECT);
    let mouse_buttons = (BTN_MOUSE..BTN_MOUSE + 8).any(|code| caps.key.get(code));
    let pen = caps.key.get(0x140) || caps.key.get(0x14b);
    if absolute_pointer && mouse_buttons && !pen {
        for code in [
            BTN_TOUCH,
            BTN_TOOL_FINGER,
            BTN_TOOL_QUINTTAP,
            BTN_TOOL_DOUBLETAP,
            BTN_TOOL_TRIPLETAP,
            BTN_TOOL_QUADTAP,
        ] {
            caps.key.clear(code);
        }
    }
    caps
}

fn enable_msix(device: &DeviceInfo, caps: &[pci::Capability]) -> Result<(), VfsError> {
    let io = |_: crate::pci::PciError| VfsError::IoError;
    let cap = caps
        .iter()
        .find(|cap| cap.id == 0x11)
        .ok_or(VfsError::NotSupported)?;
    let mut cfg = pci::config_for(device.address).map_err(io)?;
    let control = cfg.read_u16(cap.offset as u16 + 2).map_err(io)?;
    let table = cfg.read_u32(cap.offset as u16 + 4).map_err(io)?;
    let base = bar_span(device, (table & 7) as usize)?;
    let mapped =
        vmm::map_device_range(base + u64::from(table & !7), 16).map_err(|_| VfsError::IoError)?;
    let message = crate::arch::apic::request_msi(interrupt).map_err(|_| VfsError::IoError)?;
    cfg.write_u16(cap.offset as u16 + 2, control | 0xC000)
        .map_err(io)?;
    wr32(mapped + 12, 1);
    wr64(mapped, message.address);
    wr32(mapped + 8, message.data);
    fence(Ordering::SeqCst);
    wr32(mapped + 12, 0);
    cfg.write_u16(cap.offset as u16 + 2, (control | 0x8000) & !0x4000)
        .map_err(io)?;
    Ok(())
}

fn probe(device: &DeviceInfo, position: usize) -> Result<(), VfsError> {
    let caps = pci::standard_capabilities(device.address).map_err(|_| VfsError::IoError)?;
    let windows = map_windows(device, &caps)?;
    {
        let mut cfg = pci::config_for(device.address).map_err(|_| VfsError::IoError)?;
        let command = cfg.read_u16(4).map_err(|_| VfsError::IoError)?;
        cfg.write_u16(4, (command | 6) & !0x400)
            .map_err(|_| VfsError::IoError)?;
    }
    let common = windows.common;
    wr8(common + 20, 0);
    let deadline = crate::time::uptime_micros() + 1_000_000;
    while rd8(common + 20) != 0 {
        if crate::time::uptime_micros() > deadline {
            return Err(VfsError::TimedOut);
        }
        core::hint::spin_loop();
    }
    wr8(common + 20, 3);
    wr32(common, 1);
    if rd32(common + 4) & 1 == 0 {
        return Err(VfsError::NotSupported);
    }
    wr32(common + 8, 0);
    wr32(common + 12, 0);
    wr32(common + 8, 1);
    wr32(common + 12, 1); // VIRTIO_F_VERSION_1
    wr8(common + 20, 11);
    if rd8(common + 20) & 8 == 0 {
        return Err(VfsError::NotSupported);
    }
    let description = describe(windows.config, device, position);
    let name = description.name.clone();
    enable_msix(device, &caps)?;
    wr16(common + 16, 0);
    let queue = EventQueue::new(common, windows.notify, windows.multiplier, 0)?;
    let input = super::register(description)?;
    DEVICES.lock().push(Arc::new(VirtioInput {
        isr: windows.isr,
        queue: SpinLock::new(queue),
        device: input,
    }));
    wr8(common + 20, 15);
    crate::kinfo!("input: virtio-input '{}' at {}", name, device.address);
    Ok(())
}

pub fn init() {
    for (position, device) in crate::pci::devices()
        .iter()
        .filter(|device| device.vendor_id == 0x1af4 && device.device_id == PCI_DEVICE_INPUT)
        .enumerate()
    {
        if let Err(error) = probe(device, position) {
            crate::kwarn!("input: virtio-input {} failed: {:?}", device.address, error);
        }
    }
}
