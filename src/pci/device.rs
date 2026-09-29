use super::{Address, ConfigSpace, PciError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    Io { base: u32 },
    Memory32 { base: u32, prefetchable: bool },
    Memory64 { base: u64, prefetchable: bool },
    Unsupported { raw: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BridgeBuses {
    pub primary: u8,
    pub secondary: u8,
    pub subordinate: u8,
}

/// A snapshot of firmware-assigned resources. BAR sizes are deliberately not
/// guessed: sizing requires a destructive config write after driver quiescence.
#[derive(Clone, Copy, Debug)]
pub struct DeviceInfo {
    pub address: Address,
    pub vendor_id: u16,
    pub device_id: u16,
    pub revision: u8,
    pub class: u8,
    pub subclass: u8,
    pub programming_interface: u8,
    pub header_type: u8,
    pub multifunction: bool,
    pub subsystem_vendor_id: Option<u16>,
    pub subsystem_device_id: Option<u16>,
    pub interrupt_pin: u8,
    pub interrupt_line: u8,
    pub bars: [Option<Bar>; 6],
    pub bridge: Option<BridgeBuses>,
}

impl DeviceInfo {
    pub(super) fn read(
        address: Address,
        config: &ConfigSpace<'_>,
    ) -> Result<Option<Self>, PciError> {
        let identity = config.read_u32(0x00)?;
        let vendor_id = identity as u16;
        if vendor_id == 0xffff || vendor_id == 0 {
            return Ok(None);
        }
        let class = config.read_u32(0x08)?;
        let header = config.read_u8(0x0e)?;
        let header_type = header & 0x7f;
        let subsystem = if header_type == 0 {
            Some(config.read_u32(0x2c)?)
        } else {
            None
        };
        let bridge = if header_type == 1 {
            let buses = config.read_u32(0x18)?;
            Some(BridgeBuses {
                primary: buses as u8,
                secondary: (buses >> 8) as u8,
                subordinate: (buses >> 16) as u8,
            })
        } else {
            None
        };
        let mut bars = [None; 6];
        let bar_count = match header_type {
            0 => 6,
            1 => 2,
            _ => 0,
        };
        let mut index = 0;
        while index < bar_count {
            let raw = config.read_u32(0x10 + index as u16 * 4)?;
            if raw == 0 || raw == u32::MAX {
                index += 1;
                continue;
            }
            bars[index] = Some(if raw & 1 != 0 {
                Bar::Io { base: raw & !3 }
            } else {
                let prefetchable = raw & 8 != 0;
                match (raw >> 1) & 3 {
                    0 => Bar::Memory32 {
                        base: raw & !15,
                        prefetchable,
                    },
                    2 if index + 1 < bar_count => {
                        let high = config.read_u32(0x10 + (index + 1) as u16 * 4)?;
                        index += 1;
                        Bar::Memory64 {
                            base: (u64::from(high) << 32) | u64::from(raw & !15),
                            prefetchable,
                        }
                    }
                    _ => Bar::Unsupported { raw },
                }
            });
            index += 1;
        }
        Ok(Some(Self {
            address,
            vendor_id,
            device_id: (identity >> 16) as u16,
            revision: class as u8,
            programming_interface: (class >> 8) as u8,
            subclass: (class >> 16) as u8,
            class: (class >> 24) as u8,
            header_type,
            multifunction: header & 0x80 != 0,
            subsystem_vendor_id: subsystem.map(|value| value as u16),
            subsystem_device_id: subsystem.map(|value| (value >> 16) as u16),
            interrupt_pin: config.read_u8(0x3d)?,
            interrupt_line: config.read_u8(0x3c)?,
            bars,
            bridge,
        }))
    }
}
