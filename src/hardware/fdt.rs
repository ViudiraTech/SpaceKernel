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

pub struct Node<'a> {
    pub name: &'a str,
    properties: &'a [u8],
    strings: &'a [u8],
}

pub struct Property<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
}

pub struct Nodes<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    finished: bool,
}

pub struct Properties<'a> {
    bytes: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
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

    pub fn nodes(&self) -> Nodes<'_> {
        let start = be32(&self.bytes[8..12]) as usize;
        let length = be32(&self.bytes[36..40]) as usize;
        let strings_start = be32(&self.bytes[12..16]) as usize;
        let strings_length = be32(&self.bytes[32..36]) as usize;
        Nodes {
            structure: &self.bytes[start..start + length],
            strings: &self.bytes[strings_start..strings_start + strings_length],
            cursor: 0,
            finished: false,
        }
    }

    pub fn compatible(&self, needle: &str) -> Result<Option<Node<'_>>, FdtError> {
        for node in self.nodes() {
            let node = node?;
            if node.is_compatible(needle) {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }
}

impl<'a> Node<'a> {
    pub fn properties(&self) -> Properties<'a> {
        Properties {
            bytes: self.properties,
            strings: self.strings,
            cursor: 0,
        }
    }
    pub fn property(&self, name: &str) -> Option<&'a [u8]> {
        self.properties()
            .filter_map(Result::ok)
            .find(|property| property.name == name)
            .map(|property| property.value)
    }
    pub fn cell(&self, name: &str) -> Option<u32> {
        let value = self.property(name)?;
        (value.len() >= 4).then(|| be32(&value[..4]))
    }
    pub fn is_compatible(&self, needle: &str) -> bool {
        self.property("compatible").is_some_and(|value| {
            value
                .split(|byte| *byte == 0)
                .any(|item| item == needle.as_bytes())
        })
    }
}

impl<'a> Iterator for Nodes<'a> {
    type Item = Result<Node<'a>, FdtError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        while self.cursor + 4 <= self.structure.len() {
            let token = be32(&self.structure[self.cursor..self.cursor + 4]);
            self.cursor += 4;
            match token {
                1 => {
                    let rest = &self.structure[self.cursor..];
                    let Some(name_end) = rest.iter().position(|byte| *byte == 0) else {
                        self.finished = true;
                        return Some(Err(FdtError::Layout));
                    };
                    let Ok(name) = core::str::from_utf8(&rest[..name_end]) else {
                        self.finished = true;
                        return Some(Err(FdtError::Layout));
                    };
                    let Some(next) = self.cursor.checked_add(align4(name_end + 1)) else {
                        self.finished = true;
                        return Some(Err(FdtError::Layout));
                    };
                    if next > self.structure.len() {
                        self.finished = true;
                        return Some(Err(FdtError::Layout));
                    }
                    self.cursor = next;
                    let start = next;
                    let mut end = next;
                    while end + 4 <= self.structure.len() {
                        let kind = be32(&self.structure[end..end + 4]);
                        if kind == 4 {
                            end += 4;
                            continue;
                        }
                        if kind != 3 {
                            break;
                        }
                        if end + 12 > self.structure.len() {
                            break;
                        }
                        let length = be32(&self.structure[end + 4..end + 8]) as usize;
                        let Some(next) = end
                            .checked_add(12)
                            .and_then(|n| n.checked_add(align4(length)))
                        else {
                            break;
                        };
                        if next > self.structure.len() {
                            break;
                        }
                        end = next;
                    }
                    return Some(Ok(Node {
                        name,
                        properties: &self.structure[start..end],
                        strings: self.strings,
                    }));
                }
                2 | 4 => {}
                3 => {
                    if self.cursor + 8 > self.structure.len() {
                        break;
                    }
                    let length = be32(&self.structure[self.cursor..self.cursor + 4]) as usize;
                    let Some(next) = self
                        .cursor
                        .checked_add(8)
                        .and_then(|n| n.checked_add(align4(length)))
                    else {
                        break;
                    };
                    if next > self.structure.len() {
                        break;
                    }
                    self.cursor = next;
                }
                9 => {
                    self.finished = true;
                    return None;
                }
                _ => break,
            }
        }
        self.finished = true;
        Some(Err(FdtError::Layout))
    }
}

impl<'a> Iterator for Properties<'a> {
    type Item = Result<Property<'a>, FdtError>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor + 4 <= self.bytes.len() {
            let token = be32(&self.bytes[self.cursor..self.cursor + 4]);
            if token == 4 {
                self.cursor += 4;
                continue;
            }
            if token != 3 || self.cursor + 12 > self.bytes.len() {
                self.cursor = self.bytes.len();
                return Some(Err(FdtError::Layout));
            }
            let length = be32(&self.bytes[self.cursor + 4..self.cursor + 8]) as usize;
            let name_offset = be32(&self.bytes[self.cursor + 8..self.cursor + 12]) as usize;
            let start = self.cursor + 12;
            let Some(next) = start.checked_add(align4(length)) else {
                self.cursor = self.bytes.len();
                return Some(Err(FdtError::Layout));
            };
            if next > self.bytes.len() || name_offset >= self.strings.len() {
                self.cursor = self.bytes.len();
                return Some(Err(FdtError::Layout));
            }
            let Some(end) = self.strings[name_offset..]
                .iter()
                .position(|byte| *byte == 0)
            else {
                self.cursor = self.bytes.len();
                return Some(Err(FdtError::Layout));
            };
            let Ok(name) = core::str::from_utf8(&self.strings[name_offset..name_offset + end])
            else {
                self.cursor = self.bytes.len();
                return Some(Err(FdtError::Layout));
            };
            self.cursor = next;
            return Some(Ok(Property {
                name,
                value: &self.bytes[start..start + length],
            }));
        }
        None
    }
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}
fn inside(total: usize, offset: usize, len: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total)
}
fn align4(value: usize) -> usize {
    value.saturating_add(3) & !3
}
