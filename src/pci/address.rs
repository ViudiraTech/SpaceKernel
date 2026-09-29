use core::fmt;

use super::PciError;

/// A PCI function's location. The segment is a PCI domain, not a CPU or NUMA id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
}

impl Address {
    pub fn new(segment: u16, bus: u8, device: u8, function: u8) -> Result<Self, PciError> {
        if device >= 32 || function >= 8 {
            return Err(PciError::InvalidAddress);
        }
        Ok(Self {
            segment,
            bus,
            device,
            function,
        })
    }

    pub const fn segment(self) -> u16 {
        self.segment
    }
    pub const fn bus(self) -> u8 {
        self.bus
    }
    pub const fn device(self) -> u8 {
        self.device
    }
    pub const fn function(self) -> u8 {
        self.function
    }

    pub(super) const fn function_index(self) -> usize {
        (self.device as usize) * 8 + self.function as usize
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04x}:{:02x}:{:02x}.{}",
            self.segment, self.bus, self.device, self.function
        )
    }
}
