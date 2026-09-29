//! I/O APIC discovery and serialized redirection table programming.

use crate::{
    hardware::acpi::{Madt, MadtEntry},
    irq::IrqError,
    mm::vmm,
    sync::SpinLock,
};
use alloc::vec::Vec;

const IOREGSEL: usize = 0;
const IOWIN: usize = 0x10;
const REDIRECTION: u32 = 0x10;
const MASKED: u32 = 1 << 16;

#[derive(Clone, Copy)]
struct IoApic {
    id: u8,
    base: usize,
    gsi_base: u32,
    pins: u32,
}

struct State {
    controllers: Vec<IoApic>,
}
static STATE: SpinLock<Option<State>> = SpinLock::new(None);

impl IoApic {
    fn read(self, reg: u32) -> u32 {
        // SAFETY: MMIO is device mapped and STATE serializes index/window pairs.
        unsafe {
            (self.base.wrapping_add(IOREGSEL) as *mut u32).write_volatile(reg);
            (self.base.wrapping_add(IOWIN) as *const u32).read_volatile()
        }
    }
    fn write(self, reg: u32, value: u32) {
        // SAFETY: as above; register numbers are validated by pin count.
        unsafe {
            (self.base.wrapping_add(IOREGSEL) as *mut u32).write_volatile(reg);
            (self.base.wrapping_add(IOWIN) as *mut u32).write_volatile(value);
        }
    }
    fn pin(self, gsi: u32) -> Option<u32> {
        gsi.checked_sub(self.gsi_base)
            .filter(|pin| *pin < self.pins)
    }
    fn redir(self, pin: u32) -> u32 {
        REDIRECTION + pin * 2
    }
    fn mask(self, pin: u32) {
        let reg = self.redir(pin);
        self.write(reg, self.read(reg) | MASKED);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    High,
    Low,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Edge,
    Level,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Fixed(u8),
    Nmi,
}

pub fn init(madt: &Madt) -> Result<usize, IrqError> {
    let mut state = STATE.lock();
    if state.is_some() {
        return Err(IrqError::InUse);
    }
    let mut controllers: Vec<IoApic> = Vec::new();
    for entry in madt.entries() {
        if let MadtEntry::IoApic {
            id,
            address,
            gsi_base,
        } = entry.map_err(|_| IrqError::Invalid)?
        {
            if address == 0 || address & 0xfff != 0 {
                return Err(IrqError::Mapping);
            }
            let base =
                vmm::map_device_page(u64::from(address)).map_err(|_| IrqError::Mapping)? as usize;
            let mut chip = IoApic {
                id,
                base,
                gsi_base,
                pins: 0,
            };
            let version = chip.read(1);
            let pins = (version >> 16 & 0xff) + 1;
            if pins == 0 || pins > 120 || gsi_base.checked_add(pins).is_none() {
                return Err(IrqError::Invalid);
            }
            chip.pins = pins;
            for other in &controllers {
                let end = gsi_base + pins;
                let other_end = other.gsi_base + other.pins;
                if gsi_base < other_end && other.gsi_base < end {
                    return Err(IrqError::Invalid);
                }
            }
            // Preserve firmware routing while ensuring no pin can fire before
            // its software handler and destination have been installed.
            for pin in 0..pins {
                chip.mask(pin);
            }
            controllers.push(chip);
        }
    }
    let count = controllers.len();
    *state = Some(State { controllers });
    Ok(count)
}

fn find(state: &State, gsi: u32) -> Result<(IoApic, u32), IrqError> {
    state
        .controllers
        .iter()
        .find_map(|chip| chip.pin(gsi).map(|pin| (*chip, pin)))
        .ok_or(IrqError::Invalid)
}

pub fn route(
    gsi: u32,
    delivery: Delivery,
    destination: u32,
    polarity: Polarity,
    trigger: Trigger,
) -> Result<(), IrqError> {
    if destination > 255 {
        return Err(IrqError::Unsupported);
    }
    let state = STATE.lock();
    let (chip, pin) = find(state.as_ref().ok_or(IrqError::NoController)?, gsi)?;
    let reg = chip.redir(pin);
    chip.mask(pin);
    let delivery_bits = match delivery {
        Delivery::Fixed(vector) if vector >= 32 && vector != 0xff => u32::from(vector),
        Delivery::Fixed(_) => return Err(IrqError::Invalid),
        Delivery::Nmi => 4 << 8,
    };
    let low = delivery_bits
        | if polarity == Polarity::Low {
            1 << 13
        } else {
            0
        }
        | if trigger == Trigger::Level {
            1 << 15
        } else {
            0
        }
        | MASKED;
    chip.write(reg + 1, destination << 24);
    chip.write(reg, low);
    Ok(())
}

pub fn set_mask(gsi: u32, masked: bool) -> Result<(), IrqError> {
    let state = STATE.lock();
    let (chip, pin) = find(state.as_ref().ok_or(IrqError::NoController)?, gsi)?;
    let reg = chip.redir(pin);
    let old = chip.read(reg);
    chip.write(reg, if masked { old | MASKED } else { old & !MASKED });
    Ok(())
}

pub fn count() -> usize {
    STATE
        .lock()
        .as_ref()
        .map_or(0, |state| state.controllers.len())
}

pub fn describe(gsi: u32) -> Option<(u8, u32)> {
    let state = STATE.lock();
    let (chip, pin) = find(state.as_ref()?, gsi).ok()?;
    Some((chip.id, pin))
}
