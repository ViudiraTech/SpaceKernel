/*
 *
 *       src/vfs/ext4/format.rs
 *       ext4 on-disk constants, field accessors and metadata checksums
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! On-disk layout follows the Linux "ext4 Data Structures and Algorithms"
//! documentation. All ext4 fields are little-endian; jbd2 fields live in
//! `journal.rs` and are big-endian.

pub const SUPER_OFFSET: u64 = 1024;
pub const SUPER_MAGIC: u16 = 0xEF53;
pub const ROOT_INO: u32 = 2;

pub const INCOMPAT_FILETYPE: u32 = 0x2;
pub const INCOMPAT_RECOVER: u32 = 0x4;
pub const INCOMPAT_EXTENTS: u32 = 0x40;
pub const INCOMPAT_64BIT: u32 = 0x80;
pub const INCOMPAT_FLEX_BG: u32 = 0x200;
pub const INCOMPAT_CSUM_SEED: u32 = 0x2000;
pub const INCOMPAT_SUPPORTED: u32 = INCOMPAT_FILETYPE
    | INCOMPAT_RECOVER
    | INCOMPAT_EXTENTS
    | INCOMPAT_64BIT
    | INCOMPAT_FLEX_BG
    | INCOMPAT_CSUM_SEED;

pub const RO_SPARSE_SUPER: u32 = 0x1;
pub const RO_LARGE_FILE: u32 = 0x2;
pub const RO_HUGE_FILE: u32 = 0x8;
pub const RO_GDT_CSUM: u32 = 0x10;
pub const RO_DIR_NLINK: u32 = 0x20;
pub const RO_EXTRA_ISIZE: u32 = 0x40;
pub const RO_METADATA_CSUM: u32 = 0x400;
pub const RO_SUPPORTED: u32 = RO_SPARSE_SUPER
    | RO_LARGE_FILE
    | RO_HUGE_FILE
    | RO_GDT_CSUM
    | RO_DIR_NLINK
    | RO_EXTRA_ISIZE
    | RO_METADATA_CSUM;

pub const COMPAT_HAS_JOURNAL: u32 = 0x4;

pub const BG_INODE_UNINIT: u16 = 0x1;
pub const BG_BLOCK_UNINIT: u16 = 0x2;

pub const S_IFIFO: u16 = 0x1000;
pub const S_IFCHR: u16 = 0x2000;
pub const S_IFDIR: u16 = 0x4000;
pub const S_IFBLK: u16 = 0x6000;
pub const S_IFREG: u16 = 0x8000;
pub const S_IFLNK: u16 = 0xA000;
pub const S_IFSOCK: u16 = 0xC000;
pub const S_IFMT: u16 = 0xF000;

pub const FL_INDEX: u32 = 0x1000;
pub const FL_HUGE_FILE: u32 = 0x40000;
pub const FL_EXTENTS: u32 = 0x80000;
pub const FL_INLINE_DATA: u32 = 0x10000000;

pub const FT_REG: u8 = 1;
pub const FT_DIR: u8 = 2;
pub const FT_CHR: u8 = 3;
pub const FT_BLK: u8 = 4;
pub const FT_FIFO: u8 = 5;
pub const FT_SOCK: u8 = 6;
pub const FT_SYMLINK: u8 = 7;
pub const FT_DIR_CSUM: u8 = 0xDE;

pub const EXTENT_MAGIC: u16 = 0xF30A;
pub const INODE_BLOCK_OFFSET: usize = 0x28;
pub const INODE_BLOCK_LEN: usize = 60;
pub const FAST_SYMLINK_MAX: usize = 59;

pub fn le16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

pub fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

const fn crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82F6_3B78
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

static CRC32C: [u32; 256] = crc32c_table();

/// Raw CRC32C state update, matching the kernel's `crc32c(seed, data, len)`:
/// no pre- or post-inversion is applied, so calls chain.
pub fn crc32c(seed: u32, data: &[u8]) -> u32 {
    let mut crc = seed;
    for &byte in data {
        crc = CRC32C[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

/// Read-only view of the fields of the primary superblock used at runtime.
#[derive(Clone, Debug)]
pub struct Geometry {
    pub block_size: usize,
    pub first_data_block: u64,
    pub blocks_count: u64,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: usize,
    pub inodes_count: u32,
    pub desc_size: usize,
    pub group_count: u32,
    pub reserved_gdt: u32,
    pub incompat: u32,
    pub ro_compat: u32,
    pub csum_seed: u32,
    pub journal_inum: u32,
    pub desc_per_block: u32,
    pub sparse_super: bool,
    pub uuid: [u8; 16],
}

impl Geometry {
    pub fn metadata_csum(&self) -> bool {
        self.ro_compat & RO_METADATA_CSUM != 0
    }

    /// Whether `group` carries a superblock and group descriptor backup.
    pub fn group_has_super(&self, group: u32) -> bool {
        if !self.sparse_super || group <= 1 {
            return true;
        }
        for base in [3u64, 5, 7] {
            let mut value = base;
            while value < u64::from(group) {
                value *= base;
            }
            if value == u64::from(group) {
                return true;
            }
        }
        false
    }

    pub fn group_first_block(&self, group: u32) -> u64 {
        self.first_data_block + u64::from(group) * u64::from(self.blocks_per_group)
    }

    /// Number of valid blocks in `group`; the last group may be short.
    pub fn group_block_count(&self, group: u32) -> u32 {
        let first = self.group_first_block(group);
        (self.blocks_count - first).min(u64::from(self.blocks_per_group)) as u32
    }

    pub fn gdt_blocks(&self) -> u64 {
        u64::from(self.group_count).div_ceil(u64::from(self.desc_per_block))
    }
}

/// Metadata checksum seed specific to one inode.
pub fn inode_seed(geometry: &Geometry, ino: u32, generation: u32) -> u32 {
    let crc = crc32c(geometry.csum_seed, &ino.to_le_bytes());
    crc32c(crc, &generation.to_le_bytes())
}

/// Compute and store the 32-bit checksum of a raw inode record.
pub fn inode_checksum_set(geometry: &Geometry, ino: u32, raw: &mut [u8]) {
    if !geometry.metadata_csum() {
        return;
    }
    let generation = le32(raw, 0x64);
    let seed = inode_seed(geometry, ino, generation);
    const LO: usize = 0x7C;
    const HI: usize = 0x82;
    let mut crc = crc32c(seed, &raw[..LO]);
    crc = crc32c(crc, &[0, 0]);
    crc = crc32c(crc, &raw[LO + 2..128]);
    let has_hi = raw.len() > 128 && usize::from(le16(raw, 0x80)) + 128 >= HI + 2;
    if raw.len() > 128 {
        crc = crc32c(crc, &raw[128..HI]);
        if has_hi {
            crc = crc32c(crc, &[0, 0]);
            crc = crc32c(crc, &raw[HI + 2..]);
        } else {
            crc = crc32c(crc, &raw[HI..]);
        }
    }
    put16(raw, LO, crc as u16);
    if has_hi {
        put16(raw, HI, (crc >> 16) as u16);
    }
}

/// Compute and store a group descriptor checksum for the record `raw`.
pub fn desc_checksum_set(geometry: &Geometry, group: u32, raw: &mut [u8]) {
    const FIELD: usize = 0x1E;
    if geometry.metadata_csum() {
        let mut crc = crc32c(geometry.csum_seed, &group.to_le_bytes());
        crc = crc32c(crc, &raw[..FIELD]);
        crc = crc32c(crc, &[0, 0]);
        crc = crc32c(crc, &raw[FIELD + 2..]);
        put16(raw, FIELD, crc as u16);
    } else if geometry.ro_compat & RO_GDT_CSUM != 0 {
        put16(raw, FIELD, gdt_crc16(geometry, group, raw));
    }
}

/// Legacy `uninit_bg` CRC16 over UUID, group and descriptor.
fn gdt_crc16(geometry: &Geometry, group: u32, raw: &[u8]) -> u16 {
    fn update(mut crc: u16, data: &[u8]) -> u16 {
        for &byte in data {
            crc ^= u16::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xA001
                } else {
                    crc >> 1
                };
            }
        }
        crc
    }
    let mut crc = update(0xFFFF, &geometry.uuid);
    crc = update(crc, &group.to_le_bytes());
    crc = update(crc, &raw[..0x1E]);
    if raw.len() > 0x20 {
        crc = update(crc, &raw[0x20..]);
    }
    crc
}

/// Store the superblock's self checksum.
pub fn super_checksum_set(geometry: &Geometry, raw: &mut [u8]) {
    if geometry.metadata_csum() {
        let crc = crc32c(!0, &raw[..0x3FC]);
        put32(raw, 0x3FC, crc);
    }
}

/// Extent node tail checksum; `raw` is one full tree block.
pub fn extent_checksum_set(geometry: &Geometry, seed: u32, raw: &mut [u8]) {
    if !geometry.metadata_csum() {
        return;
    }
    let tail = 12 + 12 * usize::from(le16(raw, 4));
    let crc = crc32c(seed, &raw[..tail]);
    put32(raw, tail, crc);
}

/// Linear directory leaf tail checksum.
pub fn dirblock_checksum_set(geometry: &Geometry, seed: u32, raw: &mut [u8]) {
    if !geometry.metadata_csum() {
        return;
    }
    let tail = raw.len() - 12;
    let crc = crc32c(seed, &raw[..tail]);
    put32(raw, tail + 8, crc);
}

/// Write the 12-byte fake dirent that holds a leaf checksum.
pub fn dirblock_tail_init(raw: &mut [u8]) {
    let tail = raw.len() - 12;
    put32(raw, tail, 0);
    put16(raw, tail + 4, 12);
    raw[tail + 6] = 0;
    raw[tail + 7] = FT_DIR_CSUM;
    put32(raw, tail + 8, 0);
}
