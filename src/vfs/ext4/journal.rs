/*
 *
 *       src/vfs/ext4/journal.rs
 *       jbd2 journal: crash recovery and write-ahead transaction commit
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Implements the Linux jbd2 on-disk format, so a journal left behind by this
//! driver can be replayed by Linux or e2fsck and vice versa.
//!
//! Ordering contract for a commit: file data reaches its final location first
//! (the caller flushes it), then descriptors, metadata copies and revokes reach
//! the log, then a device flush, then the commit block, then another flush.
//! Metadata may only be written to its final location after that second flush.

use super::format::crc32c;
use crate::block::{BlockDevice, SECTOR_SIZE};
use crate::vfs::VfsError;
use alloc::{collections::BTreeMap, sync::Arc, vec, vec::Vec};

const MAGIC: u32 = 0xC03B_3998;
const BT_DESCRIPTOR: u32 = 1;
const BT_COMMIT: u32 = 2;
const BT_SB_V1: u32 = 3;
const BT_SB_V2: u32 = 4;
const BT_REVOKE: u32 = 5;

const INCOMPAT_REVOKE: u32 = 0x1;
const INCOMPAT_64BIT: u32 = 0x2;
const INCOMPAT_ASYNC_COMMIT: u32 = 0x4;
const INCOMPAT_CSUM_V2: u32 = 0x8;
const INCOMPAT_CSUM_V3: u32 = 0x10;
const INCOMPAT_KNOWN: u32 =
    INCOMPAT_REVOKE | INCOMPAT_64BIT | INCOMPAT_ASYNC_COMMIT | INCOMPAT_CSUM_V2 | INCOMPAT_CSUM_V3;

const TAG_ESCAPED: u32 = 1;
const TAG_SAME_UUID: u32 = 2;
const TAG_LAST: u32 = 8;

const CHECKSUM_CRC32C: u8 = 4;

fn be32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn put_be32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Maps a logical journal block to an absolute filesystem block.
pub type JournalMap = Vec<(u32, u64, u32)>;

pub struct Journal {
    device: Arc<dyn BlockDevice>,
    block_size: usize,
    map: JournalMap,
    max_len: u32,
    first: u32,
    uuid: [u8; 16],
    incompat: u32,
    csum_seed: u32,
    /// Next free log position while the log is non-empty.
    head: u32,
    /// Log position of the oldest committed, not yet checkpointed transaction.
    /// Zero means the log is empty.
    start: u32,
    /// Sequence number of `start` when the log is non-empty.
    tail_sequence: u32,
    next_sequence: u32,
    wanted_csum: bool,
}

/// Blocks replayed from a recovered journal.
pub struct RecoveryReport {
    pub transactions: u32,
    pub blocks: u32,
    pub revoked: u32,
}

impl Journal {
    fn fs_block(&self, logical: u32) -> Result<u64, VfsError> {
        for &(start, physical, length) in &self.map {
            if logical >= start && logical - start < length {
                return Ok(physical + u64::from(logical - start));
            }
        }
        crate::kwarn!("ext4: log block {} not mapped", logical);
        Err(VfsError::IoError)
    }

    fn read_log(&self, logical: u32, buffer: &mut [u8]) -> Result<(), VfsError> {
        let block = self.fs_block(logical)?;
        self.read_fs(block, buffer)
    }

    fn read_fs(&self, block: u64, buffer: &mut [u8]) -> Result<(), VfsError> {
        let sectors = self.block_size / SECTOR_SIZE;
        self.device
            .read_blocks(block * sectors as u64, sectors, buffer)
            .map_err(|error| {
                crate::kwarn!("ext4: journal read of block {} failed: {:?}", block, error);
                VfsError::IoError
            })
    }

    fn write_fs(&self, block: u64, buffer: &[u8]) -> Result<(), VfsError> {
        let sectors = self.block_size / SECTOR_SIZE;
        self.device
            .write_blocks(block * sectors as u64, sectors, buffer)
            .map_err(|error| {
                crate::kwarn!("ext4: journal write of block {} failed: {:?}", block, error);
                VfsError::IoError
            })
    }

    fn write_log(&self, logical: u32, buffer: &[u8]) -> Result<(), VfsError> {
        let block = self.fs_block(logical)?;
        self.write_fs(block, buffer)
    }

    fn flush(&self) -> Result<(), VfsError> {
        self.device.flush().map_err(|error| {
            crate::kwarn!("ext4: journal flush failed: {:?}", error);
            VfsError::IoError
        })
    }

    /// Load the journal superblock and validate its feature set.
    ///
    /// Fails with `NotSupported` for logs this driver may not interpret, such
    /// as external journals or fast commit areas.
    pub fn open(
        device: Arc<dyn BlockDevice>,
        block_size: usize,
        map: JournalMap,
        wanted_csum: bool,
    ) -> Result<Self, VfsError> {
        let mut journal = Self {
            device,
            block_size,
            map,
            max_len: 0,
            first: 1,
            uuid: [0; 16],
            incompat: 0,
            csum_seed: 0,
            head: 1,
            start: 0,
            tail_sequence: 1,
            next_sequence: 1,
            wanted_csum,
        };
        let mut raw = vec![0u8; block_size];
        journal.read_log(0, &mut raw)?;
        if be32(&raw, 0) != MAGIC {
            return Err(VfsError::InvalidArgument);
        }
        let kind = be32(&raw, 4);
        if kind != BT_SB_V1 && kind != BT_SB_V2 {
            return Err(VfsError::InvalidArgument);
        }
        if be32(&raw, 0x0C) as usize != block_size {
            return Err(VfsError::InvalidArgument);
        }
        journal.max_len = be32(&raw, 0x10);
        journal.first = be32(&raw, 0x14);
        journal.tail_sequence = be32(&raw, 0x18);
        journal.next_sequence = journal.tail_sequence;
        journal.start = be32(&raw, 0x1C);
        if kind == BT_SB_V2 {
            journal.incompat = be32(&raw, 0x28);
            if journal.incompat & !INCOMPAT_KNOWN != 0 {
                return Err(VfsError::NotSupported);
            }
            if be32(&raw, 0x40) > 1 {
                return Err(VfsError::NotSupported);
            }
            journal.uuid.copy_from_slice(&raw[0x30..0x40]);
        }
        if journal.max_len < 32 || journal.first == 0 || journal.first >= journal.max_len {
            return Err(VfsError::InvalidArgument);
        }
        journal.csum_seed = crc32c(!0, &journal.uuid);
        journal.head = journal.first;
        Ok(journal)
    }

    fn has_csum(&self) -> bool {
        self.incompat & (INCOMPAT_CSUM_V2 | INCOMPAT_CSUM_V3) != 0
    }

    fn tag_bytes(&self) -> usize {
        if self.incompat & INCOMPAT_CSUM_V3 != 0 {
            return 16;
        }
        let mut size = 12;
        if self.incompat & INCOMPAT_CSUM_V2 != 0 {
            size += 2;
        }
        if self.incompat & INCOMPAT_64BIT == 0 {
            size -= 4;
        }
        size
    }

    fn next(&self, position: u32) -> u32 {
        if position + 1 >= self.max_len {
            self.first
        } else {
            position + 1
        }
    }

    fn block_checksum_ok(&self, block: &[u8], field: usize) -> bool {
        if !self.has_csum() {
            return true;
        }
        let mut copy = block.to_vec();
        let stored = be32(&copy, field);
        put_be32(&mut copy, field, 0);
        crc32c(self.csum_seed, &copy) == stored
    }

    fn data_checksum(&self, sequence: u32, data: &[u8]) -> u32 {
        let crc = crc32c(self.csum_seed, &sequence.to_be_bytes());
        crc32c(crc, data)
    }

    fn parse_tag(&self, raw: &[u8], offset: usize) -> (u64, u32, u32) {
        let low = be32(raw, offset);
        if self.incompat & INCOMPAT_CSUM_V3 != 0 {
            let flags = be32(raw, offset + 4);
            let high = if self.incompat & INCOMPAT_64BIT != 0 {
                be32(raw, offset + 8)
            } else {
                0
            };
            let csum = be32(raw, offset + 12);
            ((u64::from(high) << 32) | u64::from(low), flags, csum)
        } else {
            let csum = u32::from(u16::from_be_bytes([raw[offset + 4], raw[offset + 5]]));
            let flags = u32::from(u16::from_be_bytes([raw[offset + 6], raw[offset + 7]]));
            let high = if self.incompat & INCOMPAT_64BIT != 0 {
                be32(raw, offset + 8)
            } else {
                0
            };
            ((u64::from(high) << 32) | u64::from(low), flags, csum)
        }
    }

    /// Replay all fully committed transactions, then mark the log empty.
    ///
    /// A transaction without a valid commit block, or with a block checksum
    /// mismatch, ends the scan: everything after it never committed.
    pub fn recover(&mut self) -> Result<RecoveryReport, VfsError> {
        struct Tagged {
            sequence: u32,
            target: u64,
            position: u32,
            escaped: bool,
        }

        let mut report = RecoveryReport {
            transactions: 0,
            blocks: 0,
            revoked: 0,
        };
        if self.start == 0 {
            return Ok(report);
        }
        let mut raw = vec![0u8; self.block_size];
        let mut data = vec![0u8; self.block_size];
        let mut position = self.start;
        let mut sequence = self.tail_sequence;
        let mut committed: Vec<Tagged> = Vec::new();
        let mut revoked: BTreeMap<u64, u32> = BTreeMap::new();
        let mut pending: Vec<Tagged> = Vec::new();
        let mut pending_revokes: Vec<u64> = Vec::new();
        let tag_bytes = self.tag_bytes();
        let scan_limit = self.max_len;
        let mut scanned = 0u32;

        'scan: while scanned < scan_limit {
            self.read_log(position, &mut raw)?;
            scanned += 1;
            if be32(&raw, 0) != MAGIC || be32(&raw, 8) != sequence {
                break;
            }
            match be32(&raw, 4) {
                BT_DESCRIPTOR => {
                    if !self.block_checksum_ok(&raw, self.block_size - 4) {
                        break;
                    }
                    let limit = if self.has_csum() {
                        self.block_size - 4
                    } else {
                        self.block_size
                    };
                    let mut offset = 12;
                    let mut cursor = self.next(position);
                    loop {
                        if offset + tag_bytes > limit {
                            break;
                        }
                        let (target, flags, csum) = self.parse_tag(&raw, offset);
                        offset += tag_bytes;
                        if flags & TAG_SAME_UUID == 0 {
                            offset += 16;
                        }
                        if self.has_csum() {
                            self.read_log(cursor, &mut data)?;
                            let mut real = data.clone();
                            if flags & TAG_ESCAPED != 0 {
                                real[..4].copy_from_slice(&MAGIC.to_be_bytes());
                            }
                            let actual = self.data_checksum(sequence, &real);
                            let matches = if self.incompat & INCOMPAT_CSUM_V3 != 0 {
                                actual == csum
                            } else {
                                (actual & 0xFFFF) == csum
                            };
                            if !matches {
                                break 'scan;
                            }
                        }
                        pending.push(Tagged {
                            sequence,
                            target,
                            position: cursor,
                            escaped: flags & TAG_ESCAPED != 0,
                        });
                        cursor = self.next(cursor);
                        scanned += 1;
                        if flags & TAG_LAST != 0 {
                            break;
                        }
                    }
                    position = cursor;
                }
                BT_REVOKE => {
                    if !self.block_checksum_ok(&raw, self.block_size - 4) {
                        break;
                    }
                    let used = be32(&raw, 12) as usize;
                    let entry = if self.incompat & INCOMPAT_64BIT != 0 {
                        8
                    } else {
                        4
                    };
                    if used > self.block_size || used < 16 {
                        break;
                    }
                    let mut offset = 16;
                    while offset + entry <= used {
                        let block = if entry == 8 {
                            (u64::from(be32(&raw, offset)) << 32)
                                | u64::from(be32(&raw, offset + 4))
                        } else {
                            u64::from(be32(&raw, offset))
                        };
                        pending_revokes.push(block);
                        offset += entry;
                    }
                    position = self.next(position);
                }
                BT_COMMIT => {
                    if !self.block_checksum_ok(&raw, 0x10) {
                        break;
                    }
                    committed.append(&mut pending);
                    for block in pending_revokes.drain(..) {
                        let slot = revoked.entry(block).or_insert(sequence);
                        if sequence.wrapping_sub(*slot) < 0x8000_0000 {
                            *slot = sequence;
                        }
                    }
                    report.transactions += 1;
                    sequence = sequence.wrapping_add(1);
                    position = self.next(position);
                }
                _ => break,
            }
        }

        report.revoked = revoked.len() as u32;
        for tag in &committed {
            if let Some(&last) = revoked.get(&tag.target)
                && last.wrapping_sub(tag.sequence) < 0x8000_0000
            {
                continue;
            }
            self.read_log(tag.position, &mut data)?;
            if tag.escaped {
                data[..4].copy_from_slice(&MAGIC.to_be_bytes());
            }
            self.write_fs(tag.target, &data)?;
            report.blocks += 1;
        }
        self.flush()?;
        // Never reuse a sequence number that may survive in stale log blocks.
        self.next_sequence = sequence.wrapping_add(1);
        self.tail_sequence = self.next_sequence;
        self.start = 0;
        self.write_superblock()?;
        self.flush()?;
        Ok(report)
    }

    /// Upgrade the log format to what this driver writes. Only legal while empty.
    pub fn adopt_features(&mut self, fs_uuid: &[u8; 16]) -> Result<(), VfsError> {
        debug_assert_eq!(self.start, 0);
        let mut features = INCOMPAT_REVOKE | INCOMPAT_64BIT;
        if self.wanted_csum {
            features |= INCOMPAT_CSUM_V3;
        }
        if self.uuid == [0; 16] {
            self.uuid = *fs_uuid;
        }
        self.incompat = features;
        self.csum_seed = crc32c(!0, &self.uuid);
        self.write_superblock()?;
        self.flush()
    }

    fn write_superblock(&self) -> Result<(), VfsError> {
        let mut raw = vec![0u8; self.block_size];
        self.read_log(0, &mut raw)?;
        put_be32(&mut raw, 4, BT_SB_V2);
        put_be32(&mut raw, 0x18, self.tail_sequence);
        put_be32(&mut raw, 0x1C, self.start);
        put_be32(&mut raw, 0x20, 0);
        put_be32(&mut raw, 0x28, self.incompat);
        raw[0x30..0x40].copy_from_slice(&self.uuid);
        if be32(&raw, 0x40) == 0 {
            put_be32(&mut raw, 0x40, 1);
        }
        if self.has_csum() {
            raw[0x50] = CHECKSUM_CRC32C;
            put_be32(&mut raw, 0xFC, 0);
            let crc = crc32c(!0, &raw[..1024]);
            put_be32(&mut raw, 0xFC, crc);
        } else {
            raw[0x50] = 0;
            put_be32(&mut raw, 0xFC, 0);
        }
        self.write_log(0, &raw)
    }

    /// Log blocks usable by transactions.
    pub fn capacity(&self) -> u32 {
        self.max_len - self.first
    }

    /// Log blocks occupied by committed, not yet checkpointed transactions.
    pub fn used(&self) -> u32 {
        if self.start == 0 {
            0
        } else if self.head >= self.start {
            self.head - self.start
        } else {
            self.capacity() - (self.start - self.head)
        }
    }

    /// Upper bound of log blocks needed to commit this many blocks and revokes.
    pub fn commit_cost(&self, blocks: usize, revokes: usize) -> u32 {
        let per_descriptor = (self.block_size - 16) / self.tag_bytes();
        let per_revoke = (self.block_size - 20) / 8;
        (blocks + blocks.div_ceil(per_descriptor) + revokes.div_ceil(per_revoke) + 1 + 1) as u32
    }

    pub fn is_empty(&self) -> bool {
        self.start == 0
    }

    /// Whether the next commit starts a log that was empty; the caller must
    /// persist the needs-recovery flag before such a commit.
    pub fn starting_fresh(&self) -> bool {
        self.start == 0
    }

    /// Write one transaction to the log and make it durable.
    ///
    /// `blocks` holds `(final block, current contents)` pairs. On success the
    /// transaction will survive a crash, and the caller may write those
    /// contents to their final locations.
    pub fn commit(
        &mut self,
        blocks: &[(u64, &[u8])],
        revokes: &[u64],
        now_seconds: u64,
    ) -> Result<(), VfsError> {
        let cost = self.commit_cost(blocks.len(), revokes.len());
        if self.used() + cost >= self.capacity() {
            return Err(VfsError::NoSpace);
        }
        let sequence = self.next_sequence;
        let first_commit = self.start == 0;
        let start_position = if first_commit { self.first } else { self.head };
        let mut position = start_position;
        let tag_bytes = self.tag_bytes();
        let has_csum = self.has_csum();
        let tail = if has_csum { 4 } else { 0 };
        let high_bits = self.incompat & INCOMPAT_64BIT != 0;

        for chunk in revokes.chunks((self.block_size - 16 - tail) / 8) {
            let mut raw = vec![0u8; self.block_size];
            put_be32(&mut raw, 0, MAGIC);
            put_be32(&mut raw, 4, BT_REVOKE);
            put_be32(&mut raw, 8, sequence);
            let mut offset = 16;
            for &block in chunk {
                if high_bits {
                    put_be32(&mut raw, offset, (block >> 32) as u32);
                    put_be32(&mut raw, offset + 4, block as u32);
                    offset += 8;
                } else {
                    put_be32(&mut raw, offset, block as u32);
                    offset += 4;
                }
            }
            put_be32(&mut raw, 12, offset as u32);
            if has_csum {
                let field = self.block_size - 4;
                put_be32(&mut raw, field, 0);
                let crc = crc32c(self.csum_seed, &raw);
                put_be32(&mut raw, field, crc);
            }
            self.write_log(position, &raw)?;
            position = self.next(position);
        }

        let per_descriptor = (self.block_size - 12 - tail) / tag_bytes;
        for chunk in blocks.chunks(per_descriptor) {
            let mut descriptor = vec![0u8; self.block_size];
            put_be32(&mut descriptor, 0, MAGIC);
            put_be32(&mut descriptor, 4, BT_DESCRIPTOR);
            put_be32(&mut descriptor, 8, sequence);
            let descriptor_position = position;
            position = self.next(position);
            let mut offset = 12;
            for (index, (target, contents)) in chunk.iter().enumerate() {
                let escaped = be32(contents, 0) == MAGIC;
                let mut flags = TAG_SAME_UUID;
                if escaped {
                    flags |= TAG_ESCAPED;
                }
                if index + 1 == chunk.len() {
                    flags |= TAG_LAST;
                }
                let checksum = if has_csum {
                    self.data_checksum(sequence, contents)
                } else {
                    0
                };
                put_be32(&mut descriptor, offset, *target as u32);
                if self.incompat & INCOMPAT_CSUM_V3 != 0 {
                    put_be32(&mut descriptor, offset + 4, flags);
                    put_be32(
                        &mut descriptor,
                        offset + 8,
                        if high_bits { (*target >> 32) as u32 } else { 0 },
                    );
                    put_be32(&mut descriptor, offset + 12, checksum);
                } else {
                    descriptor[offset + 4..offset + 6]
                        .copy_from_slice(&(checksum as u16).to_be_bytes());
                    descriptor[offset + 6..offset + 8]
                        .copy_from_slice(&(flags as u16).to_be_bytes());
                    if high_bits {
                        put_be32(&mut descriptor, offset + 8, (*target >> 32) as u32);
                    }
                }
                offset += tag_bytes;

                if escaped {
                    let mut copy = contents.to_vec();
                    copy[..4].fill(0);
                    self.write_log(position, &copy)?;
                } else {
                    self.write_log(position, contents)?;
                }
                position = self.next(position);
            }
            if has_csum {
                let field = self.block_size - 4;
                put_be32(&mut descriptor, field, 0);
                let crc = crc32c(self.csum_seed, &descriptor);
                put_be32(&mut descriptor, field, crc);
            }
            self.write_log(descriptor_position, &descriptor)?;
        }

        if first_commit {
            // Recovery must find this transaction before its commit block lands.
            self.start = start_position;
            self.tail_sequence = sequence;
            self.write_superblock()?;
        }
        self.flush()?;

        let mut commit = vec![0u8; self.block_size];
        put_be32(&mut commit, 0, MAGIC);
        put_be32(&mut commit, 4, BT_COMMIT);
        put_be32(&mut commit, 8, sequence);
        commit[0x30..0x38].copy_from_slice(&now_seconds.to_be_bytes());
        if has_csum {
            commit[0x0C] = CHECKSUM_CRC32C;
            commit[0x0D] = 4;
            let crc = crc32c(self.csum_seed, &commit);
            put_be32(&mut commit, 0x10, crc);
        }
        self.write_log(position, &commit)?;
        self.flush()?;

        self.head = self.next(position);
        self.next_sequence = sequence.wrapping_add(1);
        Ok(())
    }

    /// Declare every committed transaction checkpointed.
    ///
    /// The caller must already have flushed all logged blocks to their final
    /// locations.
    pub fn mark_empty(&mut self) -> Result<(), VfsError> {
        if self.start == 0 {
            return Ok(());
        }
        self.start = 0;
        self.tail_sequence = self.next_sequence;
        self.head = self.first;
        self.write_superblock()?;
        self.flush()
    }
}
