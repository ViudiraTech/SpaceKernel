//! Bounds checked access to the firmware-supplied flattened device tree (FDT / DTB).
//! Conforms to Devicetree Specification v0.4.

const MAGIC: u32 = 0xd00d_feed;
const HEADER_SIZE: usize = 40;
const MAX_SIZE: usize = 16 * 1024 * 1024; // 16 MiB max DTB size

const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_NOP: u32 = 4;
const FDT_END: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdtError {
    Null,
    Magic,
    Length,
    Layout,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FdtHeader {
    pub totalsize: usize,
    pub off_dt_struct: usize,
    pub off_dt_strings: usize,
    pub off_mem_rsvmap: usize,
    pub version: u32,
    pub last_comp_version: u32,
    pub boot_cpuid_phys: u32,
    pub size_dt_strings: usize,
    pub size_dt_struct: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryReservation {
    pub address: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegRange {
    pub address: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusRange {
    pub child_bus_address: u64,
    pub parent_bus_address: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fdt<'a> {
    bytes: &'a [u8],
}

impl Fdt<'static> {
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
        let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, size) };
        Self::from_slice(bytes)
    }
}

impl<'a> Fdt<'a> {
    pub fn from_slice(bytes: &'a [u8]) -> Result<Self, FdtError> {
        if bytes.len() < HEADER_SIZE {
            return Err(FdtError::Length);
        }
        if be32(&bytes[0..4]) != MAGIC {
            return Err(FdtError::Magic);
        }
        let size = be32(&bytes[4..8]) as usize;
        if !(HEADER_SIZE..=MAX_SIZE).contains(&size) || size > bytes.len() {
            return Err(FdtError::Length);
        }
        let structure = be32(&bytes[8..12]) as usize;
        let strings = be32(&bytes[12..16]) as usize;
        let reserve = be32(&bytes[16..20]) as usize;
        let strings_size = be32(&bytes[32..36]) as usize;
        let structure_size = be32(&bytes[36..40]) as usize;
        if structure % 4 != 0
            || reserve % 8 != 0
            || !inside(size, structure, structure_size)
            || !inside(size, strings, strings_size)
            || reserve >= size
        {
            return Err(FdtError::Layout);
        }
        Ok(Self {
            bytes: &bytes[..size],
        })
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn header(&self) -> FdtHeader {
        FdtHeader {
            totalsize: be32(&self.bytes[4..8]) as usize,
            off_dt_struct: be32(&self.bytes[8..12]) as usize,
            off_dt_strings: be32(&self.bytes[12..16]) as usize,
            off_mem_rsvmap: be32(&self.bytes[16..20]) as usize,
            version: be32(&self.bytes[20..24]),
            last_comp_version: be32(&self.bytes[24..28]),
            boot_cpuid_phys: be32(&self.bytes[28..32]),
            size_dt_strings: be32(&self.bytes[32..36]) as usize,
            size_dt_struct: be32(&self.bytes[36..40]) as usize,
        }
    }

    fn structure(&self) -> &'a [u8] {
        let start = be32(&self.bytes[8..12]) as usize;
        let length = be32(&self.bytes[36..40]) as usize;
        &self.bytes[start..start + length]
    }

    fn strings(&self) -> &'a [u8] {
        let strings_start = be32(&self.bytes[12..16]) as usize;
        let strings_length = be32(&self.bytes[32..36]) as usize;
        &self.bytes[strings_start..strings_start + strings_length]
    }

    pub fn memory_reservations(&self) -> MemoryReservations<'a> {
        let reserve_start = be32(&self.bytes[16..20]) as usize;
        MemoryReservations {
            bytes: &self.bytes[reserve_start..],
            cursor: 0,
        }
    }

    pub fn root(&self) -> Result<Node<'a>, FdtError> {
        let structure = self.structure();
        let strings = self.strings();
        let mut offset = 0;
        while offset + 4 <= structure.len() && be32(&structure[offset..offset + 4]) == FDT_NOP {
            offset += 4;
        }
        if offset + 4 > structure.len() || be32(&structure[offset..offset + 4]) != FDT_BEGIN_NODE {
            return Err(FdtError::Layout);
        }
        scan_node(structure, strings, offset, 0, 2, 1)
    }

    pub fn all_nodes(&self) -> AllNodes<'a> {
        let root = self.root().ok();
        AllNodes {
            stack: [const { None }; 16],
            depth: 0,
            root_yielded: false,
            root,
        }
    }

    pub fn find_node(&self, path: &str) -> Option<Node<'a>> {
        let mut current = self.root().ok()?;
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Some(current);
        }
        for segment in path.split('/') {
            if segment.is_empty() {
                continue;
            }
            let child = current
                .children()
                .find(|c| c.name == segment || c.name_without_unit() == segment)?;
            current = child;
        }
        Some(current)
    }

    pub fn find_compatible(&self, needle: &str) -> Option<Node<'a>> {
        self.all_nodes().find(|node| node.is_compatible(needle))
    }

    pub fn find_by_phandle(&self, phandle: u32) -> Option<Node<'a>> {
        self.all_nodes()
            .find(|node| node.phandle() == Some(phandle))
    }

    pub fn chosen(&self) -> Option<Chosen<'a>> {
        self.find_node("/chosen").map(|node| Chosen { node })
    }

    pub fn memory_regions(&self) -> impl Iterator<Item = RegRange> + 'a {
        self.all_nodes()
            .filter(|node| {
                node.property("device_type")
                    .and_then(|p| p.as_str())
                    .is_some_and(|dt| dt == "memory")
                    || node.name.starts_with("memory")
            })
            .filter_map(|node| node.reg())
            .flatten()
    }

    pub fn cpus(&self) -> impl Iterator<Item = Node<'a>> {
        self.find_node("/cpus")
            .into_iter()
            .flat_map(|cpus_node| cpus_node.children())
            .filter(|node| {
                node.property("device_type")
                    .and_then(|p| p.as_str())
                    .is_some_and(|dt| dt == "cpu")
                    || node.name.starts_with("cpu")
            })
    }

    /// Legacy compat helper for node search
    pub fn compatible(&self, needle: &str) -> Result<Option<Node<'a>>, FdtError> {
        Ok(self.find_compatible(needle))
    }

    /// Legacy compat helper
    pub fn nodes(&self) -> Nodes<'a> {
        let structure = self.structure();
        let strings = self.strings();
        Nodes {
            structure,
            strings,
            cursor: 0,
            finished: false,
        }
    }
}

pub struct MemoryReservations<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Iterator for MemoryReservations<'a> {
    type Item = MemoryReservation;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor + 16 > self.bytes.len() {
            return None;
        }
        let address = be64(&self.bytes[self.cursor..self.cursor + 8]);
        let size = be64(&self.bytes[self.cursor + 8..self.cursor + 16]);
        self.cursor += 16;
        if address == 0 && size == 0 {
            return None;
        }
        Some(MemoryReservation { address, size })
    }
}

#[derive(Clone, Copy)]
pub struct Node<'a> {
    pub name: &'a str,
    structure: &'a [u8],
    strings: &'a [u8],
    prop_start: usize,
    prop_end: usize,
    body_end: usize,
    pub depth: usize,
    pub address_cells: usize,
    pub size_cells: usize,
    pub parent_address_cells: usize,
    pub parent_size_cells: usize,
}

pub type FdtNode<'a> = Node<'a>;

impl<'a> Node<'a> {
    pub fn name_without_unit(&self) -> &'a str {
        match self.name.split_once('@') {
            Some((prefix, _)) => prefix,
            None => self.name,
        }
    }

    pub fn unit_address(&self) -> Option<&'a str> {
        self.name.split_once('@').map(|(_, unit)| unit)
    }

    pub fn properties(&self) -> Properties<'a> {
        Properties {
            structure: self.structure,
            strings: self.strings,
            cursor: self.prop_start,
            limit: self.prop_end,
        }
    }

    pub fn property(&self, name: &str) -> Option<Property<'a>> {
        self.properties().find(|property| property.name == name)
    }

    pub fn cell(&self, name: &str) -> Option<u32> {
        self.property(name).and_then(|p| p.as_u32())
    }

    pub fn is_compatible(&self, needle: &str) -> bool {
        self.property("compatible")
            .is_some_and(|p| p.as_string_list().any(|s| s == needle))
    }

    pub fn phandle(&self) -> Option<u32> {
        self.property("phandle")
            .and_then(|p| p.as_u32())
            .or_else(|| self.property("linux,phandle").and_then(|p| p.as_u32()))
    }

    pub fn interrupt_parent(&self) -> Option<u32> {
        self.property("interrupt-parent").and_then(|p| p.as_u32())
    }

    pub fn interrupts(&self) -> Option<&'a [u8]> {
        self.property("interrupts").map(|p| p.value)
    }

    pub fn reg(&self) -> Option<RegRanges<'a>> {
        let prop = self.property("reg")?;
        Some(RegRanges {
            bytes: prop.value,
            cursor: 0,
            address_cells: self.parent_address_cells,
            size_cells: self.parent_size_cells,
        })
    }

    pub fn ranges(&self) -> Option<RangeRanges<'a>> {
        let prop = self.property("ranges")?;
        Some(RangeRanges {
            bytes: prop.value,
            cursor: 0,
            child_address_cells: self.address_cells,
            parent_address_cells: self.parent_address_cells,
            size_cells: self.size_cells,
        })
    }

    pub fn translate_address(&self, child_bus_address: u64) -> u64 {
        let Some(ranges) = self.ranges() else {
            return child_bus_address;
        };
        if ranges.bytes.is_empty() {
            return child_bus_address;
        }
        for entry in ranges {
            if child_bus_address >= entry.child_bus_address
                && child_bus_address < entry.child_bus_address.saturating_add(entry.length)
            {
                return entry
                    .parent_bus_address
                    .saturating_add(child_bus_address - entry.child_bus_address);
            }
        }
        child_bus_address
    }

    pub fn children(&self) -> Children<'a> {
        Children {
            structure: self.structure,
            strings: self.strings,
            cursor: self.prop_end,
            limit: self.body_end,
            depth: self.depth + 1,
            parent_address_cells: self.address_cells,
            parent_size_cells: self.size_cells,
        }
    }
}

fn scan_node<'a>(
    structure: &'a [u8],
    strings: &'a [u8],
    start_offset: usize,
    depth: usize,
    parent_address_cells: usize,
    parent_size_cells: usize,
) -> Result<Node<'a>, FdtError> {
    if start_offset + 4 > structure.len()
        || be32(&structure[start_offset..start_offset + 4]) != FDT_BEGIN_NODE
    {
        return Err(FdtError::Layout);
    }
    let mut cursor = start_offset + 4;
    let name_end = structure[cursor..]
        .iter()
        .position(|b| *b == 0)
        .ok_or(FdtError::Layout)?;
    let name = core::str::from_utf8(&structure[cursor..cursor + name_end])
        .map_err(|_| FdtError::Layout)?;
    cursor = cursor
        .checked_add(align4(name_end + 1))
        .ok_or(FdtError::Layout)?;
    if cursor > structure.len() {
        return Err(FdtError::Layout);
    }

    let prop_start = cursor;
    while cursor + 4 <= structure.len() {
        let token = be32(&structure[cursor..cursor + 4]);
        if token == FDT_NOP {
            cursor += 4;
            continue;
        }
        if token != FDT_PROP {
            break;
        }
        if cursor + 12 > structure.len() {
            return Err(FdtError::Layout);
        }
        let length = be32(&structure[cursor + 4..cursor + 8]) as usize;
        cursor = cursor
            .checked_add(12)
            .and_then(|c| c.checked_add(align4(length)))
            .ok_or(FdtError::Layout)?;
        if cursor > structure.len() {
            return Err(FdtError::Layout);
        }
    }
    let prop_end = cursor;

    // Read #address-cells and #size-cells defined on this node for its children
    let own_address_cells =
        find_property_raw(structure, strings, prop_start, prop_end, "#address-cells")
            .and_then(|p| p.as_u32())
            .map(|v| v as usize)
            .unwrap_or(2);
    let own_size_cells = find_property_raw(structure, strings, prop_start, prop_end, "#size-cells")
        .and_then(|p| p.as_u32())
        .map(|v| v as usize)
        .unwrap_or(1);

    // Scan forward to find the matching FDT_END_NODE for this node
    let mut subdepth = 1;
    let mut scan_cur = prop_end;
    while scan_cur + 4 <= structure.len() && subdepth > 0 {
        let token = be32(&structure[scan_cur..scan_cur + 4]);
        match token {
            FDT_NOP => scan_cur += 4,
            FDT_BEGIN_NODE => {
                subdepth += 1;
                scan_cur += 4;
                let end = structure[scan_cur..]
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or(FdtError::Layout)?;
                scan_cur = scan_cur
                    .checked_add(align4(end + 1))
                    .ok_or(FdtError::Layout)?;
                if scan_cur > structure.len() {
                    return Err(FdtError::Layout);
                }
            }
            FDT_END_NODE => {
                subdepth -= 1;
                if subdepth == 0 {
                    break;
                }
                scan_cur += 4;
            }
            FDT_PROP => {
                if scan_cur + 12 > structure.len() {
                    return Err(FdtError::Layout);
                }
                let length = be32(&structure[scan_cur + 4..scan_cur + 8]) as usize;
                scan_cur = scan_cur
                    .checked_add(12)
                    .and_then(|c| c.checked_add(align4(length)))
                    .ok_or(FdtError::Layout)?;
                if scan_cur > structure.len() {
                    return Err(FdtError::Layout);
                }
            }
            FDT_END => return Err(FdtError::Layout),
            _ => return Err(FdtError::Layout),
        }
    }

    if subdepth != 0 {
        return Err(FdtError::Layout);
    }

    Ok(Node {
        name,
        structure,
        strings,
        prop_start,
        prop_end,
        body_end: scan_cur,
        depth,
        address_cells: own_address_cells,
        size_cells: own_size_cells,
        parent_address_cells,
        parent_size_cells,
    })
}

fn find_property_raw<'a>(
    structure: &'a [u8],
    strings: &'a [u8],
    start: usize,
    limit: usize,
    name: &str,
) -> Option<Property<'a>> {
    let mut props = Properties {
        structure,
        strings,
        cursor: start,
        limit,
    };
    props.find(|p| p.name == name)
}

#[derive(Clone, Copy)]
pub struct Children<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    limit: usize,
    depth: usize,
    parent_address_cells: usize,
    parent_size_cells: usize,
}

impl<'a> Iterator for Children<'a> {
    type Item = Node<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor + 4 <= self.limit {
            let token = be32(&self.structure[self.cursor..self.cursor + 4]);
            match token {
                FDT_NOP => self.cursor += 4,
                FDT_BEGIN_NODE => {
                    let child = scan_node(
                        self.structure,
                        self.strings,
                        self.cursor,
                        self.depth,
                        self.parent_address_cells,
                        self.parent_size_cells,
                    )
                    .ok()?;
                    self.cursor = child.body_end + 4;
                    return Some(child);
                }
                FDT_END_NODE => return None,
                _ => return None,
            }
        }
        None
    }
}

pub struct AllNodes<'a> {
    stack: [Option<Children<'a>>; 16],
    depth: usize,
    root_yielded: bool,
    root: Option<Node<'a>>,
}

impl<'a> Iterator for AllNodes<'a> {
    type Item = Node<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if !self.root_yielded {
            self.root_yielded = true;
            if let Some(root) = self.root {
                self.stack[0] = Some(root.children());
                self.depth = 0;
                return Some(root);
            }
            return None;
        }

        loop {
            if let Some(ref mut iter) = self.stack[self.depth] {
                if let Some(child) = iter.next() {
                    if self.depth + 1 < self.stack.len() {
                        self.stack[self.depth + 1] = Some(child.children());
                        self.depth += 1;
                    }
                    return Some(child);
                } else {
                    self.stack[self.depth] = None;
                    if self.depth == 0 {
                        return None;
                    }
                    self.depth -= 1;
                }
            } else {
                if self.depth == 0 {
                    return None;
                }
                self.depth -= 1;
            }
        }
    }
}

pub struct Properties<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    limit: usize,
}

impl<'a> Iterator for Properties<'a> {
    type Item = Property<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.cursor + 4 <= self.limit {
            let token = be32(&self.structure[self.cursor..self.cursor + 4]);
            if token == FDT_NOP {
                self.cursor += 4;
                continue;
            }
            if token != FDT_PROP || self.cursor + 12 > self.limit {
                return None;
            }
            let length = be32(&self.structure[self.cursor + 4..self.cursor + 8]) as usize;
            let name_offset = be32(&self.structure[self.cursor + 8..self.cursor + 12]) as usize;
            let start = self.cursor + 12;
            let next = start.checked_add(align4(length))?;
            if next > self.limit || name_offset >= self.strings.len() {
                return None;
            }
            let end = self.strings[name_offset..].iter().position(|b| *b == 0)?;
            let name = core::str::from_utf8(&self.strings[name_offset..name_offset + end]).ok()?;
            self.cursor = next;
            return Some(Property {
                name,
                value: &self.structure[start..start + length],
            });
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Property<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
}

impl<'a> Property<'a> {
    pub fn as_str(&self) -> Option<&'a str> {
        let bytes = if self.value.ends_with(b"\0") {
            &self.value[..self.value.len() - 1]
        } else {
            self.value
        };
        core::str::from_utf8(bytes).ok()
    }

    pub fn as_string_list(&self) -> StringList<'a> {
        StringList { bytes: self.value }
    }

    pub fn as_u32(&self) -> Option<u32> {
        if self.value.len() >= 4 {
            Some(be32(&self.value[..4]))
        } else {
            None
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        if self.value.len() >= 8 {
            Some(be64(&self.value[..8]))
        } else {
            None
        }
    }
}

pub struct StringList<'a> {
    bytes: &'a [u8],
}

impl<'a> Iterator for StringList<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        while !self.bytes.is_empty() {
            let end = self
                .bytes
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(self.bytes.len());
            let slice = &self.bytes[..end];
            self.bytes = if end < self.bytes.len() {
                &self.bytes[end + 1..]
            } else {
                &[]
            };
            if let Ok(s) = core::str::from_utf8(slice) {
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegRanges<'a> {
    bytes: &'a [u8],
    cursor: usize,
    address_cells: usize,
    size_cells: usize,
}

impl<'a> Iterator for RegRanges<'a> {
    type Item = RegRange;
    fn next(&mut self) -> Option<Self::Item> {
        let entry_len = (self.address_cells + self.size_cells) * 4;
        if entry_len == 0 || self.cursor + entry_len > self.bytes.len() {
            return None;
        }
        let addr_bytes = &self.bytes[self.cursor..self.cursor + self.address_cells * 4];
        let size_bytes = &self.bytes[self.cursor + self.address_cells * 4..self.cursor + entry_len];
        self.cursor += entry_len;
        let address = read_cells(addr_bytes)?;
        let size = read_cells(size_bytes)?;
        Some(RegRange { address, size })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RangeRanges<'a> {
    bytes: &'a [u8],
    cursor: usize,
    child_address_cells: usize,
    parent_address_cells: usize,
    size_cells: usize,
}

impl<'a> Iterator for RangeRanges<'a> {
    type Item = BusRange;
    fn next(&mut self) -> Option<Self::Item> {
        let entry_len =
            (self.child_address_cells + self.parent_address_cells + self.size_cells) * 4;
        if entry_len == 0 || self.cursor + entry_len > self.bytes.len() {
            return None;
        }
        let child_addr_bytes = &self.bytes[self.cursor..self.cursor + self.child_address_cells * 4];
        let parent_addr_offset = self.cursor + self.child_address_cells * 4;
        let parent_addr_bytes =
            &self.bytes[parent_addr_offset..parent_addr_offset + self.parent_address_cells * 4];
        let size_offset = parent_addr_offset + self.parent_address_cells * 4;
        let size_bytes = &self.bytes[size_offset..size_offset + self.size_cells * 4];
        self.cursor += entry_len;
        let child_bus_address = read_cells(child_addr_bytes)?;
        let parent_bus_address = read_cells(parent_addr_bytes)?;
        let length = read_cells(size_bytes)?;
        Some(BusRange {
            child_bus_address,
            parent_bus_address,
            length,
        })
    }
}

pub struct Chosen<'a> {
    pub node: Node<'a>,
}

impl<'a> Chosen<'a> {
    pub fn bootargs(&self) -> Option<&'a str> {
        self.node.property("bootargs").and_then(|p| p.as_str())
    }

    pub fn stdout_path(&self) -> Option<&'a str> {
        self.node.property("stdout-path").and_then(|p| p.as_str())
    }
}

/// Legacy flat iterator for backward compatibility
pub struct Nodes<'a> {
    structure: &'a [u8],
    strings: &'a [u8],
    cursor: usize,
    finished: bool,
}

impl<'a> Iterator for Nodes<'a> {
    type Item = Result<Node<'a>, FdtError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        while self.cursor + 4 <= self.structure.len() {
            let token = be32(&self.structure[self.cursor..self.cursor + 4]);
            if token == FDT_NOP {
                self.cursor += 4;
                continue;
            }
            if token == FDT_BEGIN_NODE {
                match scan_node(self.structure, self.strings, self.cursor, 0, 2, 1) {
                    Ok(node) => {
                        self.cursor = node.prop_start;
                        return Some(Ok(node));
                    }
                    Err(e) => {
                        self.finished = true;
                        return Some(Err(e));
                    }
                }
            } else if token == FDT_END_NODE {
                self.cursor += 4;
            } else if token == FDT_PROP {
                if self.cursor + 8 > self.structure.len() {
                    break;
                }
                let length = be32(&self.structure[self.cursor + 4..self.cursor + 8]) as usize;
                let Some(next) = self
                    .cursor
                    .checked_add(12)
                    .and_then(|c| c.checked_add(align4(length)))
                else {
                    break;
                };
                self.cursor = next;
            } else if token == FDT_END {
                self.finished = true;
                return None;
            } else {
                break;
            }
        }
        self.finished = true;
        Some(Err(FdtError::Layout))
    }
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes[..4].try_into().unwrap())
}

fn be64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes[..8].try_into().unwrap())
}

fn inside(total: usize, offset: usize, len: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total)
}

fn align4(value: usize) -> usize {
    value.saturating_add(3) & !3
}

fn read_cells(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        0 => Some(0),
        1..=4 => {
            let mut buf = [0u8; 4];
            buf[4 - bytes.len()..].copy_from_slice(bytes);
            Some(u32::from_be_bytes(buf) as u64)
        }
        5..=8 => {
            let mut buf = [0u8; 8];
            buf[8 - bytes.len()..].copy_from_slice(bytes);
            Some(u64::from_be_bytes(buf))
        }
        12 => {
            // PCI 3-cell address: phys.hi (flags), phys.mid, phys.lo (64-bit base address)
            Some(u64::from_be_bytes(bytes[4..12].try_into().ok()?))
        }
        _ => None,
    }
}
