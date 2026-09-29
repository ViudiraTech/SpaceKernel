use alloc::{
    collections::{BTreeSet, VecDeque},
    vec::Vec,
};

use super::{Address, DeviceInfo, PciCore, PciError};

/// Follow firmware-configured secondary buses without renumbering live bridges.
pub(super) fn scan(core: &PciCore) -> Result<Vec<DeviceInfo>, PciError> {
    let mut pending = VecDeque::new();
    let mut visited = BTreeSet::new();
    let mut devices = Vec::new();
    for window in &core.windows {
        let segment = window.segment();
        pending.push_back((segment.group, segment.start_bus));
    }
    while let Some((segment, bus)) = pending.pop_front() {
        if !visited.insert((segment, bus)) {
            continue;
        }
        for slot in 0..32 {
            let address = Address::new(segment, bus, slot, 0)?;
            let Some(first) = inspect(core, address)? else {
                continue;
            };
            let functions = if first.multifunction { 8 } else { 1 };
            enqueue_bridge(core, &first, &mut pending);
            devices.push(first);
            for function in 1..functions {
                let address = Address::new(segment, bus, slot, function)?;
                if let Some(device) = inspect(core, address)? {
                    enqueue_bridge(core, &device, &mut pending);
                    devices.push(device);
                }
            }
        }
    }
    Ok(devices)
}

fn inspect(core: &PciCore, address: Address) -> Result<Option<DeviceInfo>, PciError> {
    let config = core.config(address)?;
    DeviceInfo::read(address, &config)
}

fn enqueue_bridge(core: &PciCore, device: &DeviceInfo, pending: &mut VecDeque<(u16, u8)>) {
    let Some(bridge) = device.bridge else { return };
    // Do not follow unconfigured or inconsistent bridges. Scanning an arbitrary
    // bus from a corrupt register could trigger unsupported host accesses.
    if bridge.primary != device.address.bus()
        || bridge.secondary == 0
        || bridge.secondary > bridge.subordinate
    {
        return;
    }
    let child = Address::new(device.address.segment(), bridge.secondary, 0, 0)
        .expect("bounded bridge address");
    if core.window(child).is_some() {
        pending.push_back((child.segment(), child.bus()));
    }
}
