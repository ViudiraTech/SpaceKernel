//! Bounds checked access to the firmware supplied flattened device tree.

const MAGIC: u32 = 0xd00d_feed;
const HEADER_SIZE: usize = 40;
const MAX_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdtError {
    Null,
    Magic,
    Length,
    Layout,
}

pub struct Fdt {
    bytes: &'static [u8],
}

impl Fdt {
    pub fn from_boot_address(address: usize) -> Result<Self, FdtError> {
        if address == 0 {
            return Err(FdtError::Null);
        }
        // SAFETY: Limine promises an address to an FDT header. The header is
        // read only; the full mapping is not exposed until its layout passes.
        let header = unsafe { core::slice::from_raw_parts(address as *const u8, HEADER_SIZE) };
        if be32(&header[0..4]) != MAGIC {
            return Err(FdtError::Magic);
        }
        let size = be32(&header[4..8]) as usize;
        if !(HEADER_SIZE..=MAX_SIZE).contains(&size) {
            return Err(FdtError::Length);
        }
        let structure = be32(&header[8..12]) as usize;
        let strings = be32(&header[12..16]) as usize;
        let reserve = be32(&header[16..20]) as usize;
        let strings_size = be32(&header[32..36]) as usize;
        let structure_size = be32(&header[36..40]) as usize;
        if structure % 4 != 0
            || reserve % 8 != 0
            || !inside(size, structure, structure_size)
            || !inside(size, strings, strings_size)
            || reserve >= size
        {
            return Err(FdtError::Layout);
        }
        // SAFETY: the bootloader maps the complete FDT blob and owns it for
        // the kernel lifetime; the total-size field was bounded above.
        let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, size) };
        Ok(Self { bytes })
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}
fn inside(total: usize, offset: usize, len: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total)
}
