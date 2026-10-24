/*
 *
 *       src/vfs/fat32.rs
 *       FAT32 (VFAT) on-disk filesystem driver and formatter
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Complete FAT32 filesystem driver supporting BPB parsing, FAT cluster chain
//! allocation/deallocation, 8.3 and VFAT Long File Names (LFN), file read/write,
//! directory creation/deletion, and volume formatting.

use super::{
    abi::MSDOS_SUPER_MAGIC,
    inode::{DirEntry, FileSystem, Inode, InodeKind, Stat, StatFs, VfsError},
};
use crate::{
    block::{self, BlockDevice, SECTOR_SIZE},
    sync::SpinLock,
};
use alloc::{format, string::String, sync::Arc, vec, vec::Vec};

const FAT_EOC: u32 = 0x0FFF_FFF8;
const FAT_MARK_EOC: u32 = 0x0FFF_FFFF;
const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F;

/// Format a block device (or partition) with a clean FAT32 filesystem.
pub fn format_fat32(dev: &Arc<dyn BlockDevice>, label: &str) -> Result<(), VfsError> {
    let info = dev.info();
    if info.block_size != SECTOR_SIZE || info.total_blocks < 128 {
        return Err(VfsError::InvalidArgument);
    }

    let total_sectors = info.total_blocks as u32;
    let reserved_sectors = 32u16;
    let num_fats = 2u8;
    let sectors_per_cluster = 1u8;

    let data_sectors_approx = total_sectors.saturating_sub(u32::from(reserved_sectors));
    let fat_sectors = (data_sectors_approx * 4).div_ceil(512).max(1);

    // 1. Build Boot Sector (BPB + FAT32 EBPB) at LBA 0
    let mut bpb = [0u8; SECTOR_SIZE];
    bpb[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    bpb[3..11].copy_from_slice(b"SPACEKRN");
    bpb[11..13].copy_from_slice(&(512u16).to_le_bytes()); // BytesPerSec
    bpb[13] = sectors_per_cluster; // SecPerClus
    bpb[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
    bpb[16] = num_fats;
    bpb[17..19].copy_from_slice(&0u16.to_le_bytes()); // RootEntCnt = 0 for FAT32
    bpb[19..21].copy_from_slice(&0u16.to_le_bytes()); // TotSec16 = 0
    bpb[21] = 0xF8; // Media = fixed disk
    bpb[22..24].copy_from_slice(&0u16.to_le_bytes()); // FATSz16 = 0
    bpb[24..26].copy_from_slice(&32u16.to_le_bytes()); // SecPerTrk
    bpb[26..28].copy_from_slice(&64u16.to_le_bytes()); // NumHeads
    bpb[28..32].copy_from_slice(&0u32.to_le_bytes()); // HiddSec
    bpb[32..36].copy_from_slice(&total_sectors.to_le_bytes()); // TotSec32

    // FAT32 Extended BPB
    bpb[36..40].copy_from_slice(&fat_sectors.to_le_bytes()); // FATSz32
    bpb[40..42].copy_from_slice(&0u16.to_le_bytes()); // ExtFlags
    bpb[42..44].copy_from_slice(&0u16.to_le_bytes()); // FSVer
    bpb[44..48].copy_from_slice(&2u32.to_le_bytes()); // RootClus = 2
    bpb[48..50].copy_from_slice(&1u16.to_le_bytes()); // FSInfo = LBA 1
    bpb[50..52].copy_from_slice(&6u16.to_le_bytes()); // BkBootSec = LBA 6
    bpb[64] = 0x80; // DrvNum
    bpb[66] = 0x29; // BootSig
    bpb[67..71].copy_from_slice(&0x2026_1023u32.to_le_bytes()); // VolID

    let mut vol_label = [b' '; 11];
    for (i, b) in label.bytes().take(11).enumerate() {
        vol_label[i] = b.to_ascii_uppercase();
    }
    bpb[71..82].copy_from_slice(&vol_label);
    bpb[82..90].copy_from_slice(b"FAT32   ");
    bpb[510] = 0x55;
    bpb[511] = 0xAA;

    dev.write_blocks(0, 1, &bpb)
        .map_err(|_| VfsError::IoError)?;
    dev.write_blocks(6, 1, &bpb)
        .map_err(|_| VfsError::IoError)?;

    // 2. Write FSInfo Sector at LBA 1
    let mut fsinfo = [0u8; SECTOR_SIZE];
    fsinfo[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    fsinfo[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    let total_clusters = total_sectors
        .saturating_sub(u32::from(reserved_sectors) + u32::from(num_fats) * fat_sectors);
    fsinfo[488..492].copy_from_slice(&total_clusters.saturating_sub(1).to_le_bytes());
    fsinfo[492..496].copy_from_slice(&3u32.to_le_bytes());
    fsinfo[508..512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
    dev.write_blocks(1, 1, &fsinfo)
        .map_err(|_| VfsError::IoError)?;

    // 3. Initialize FAT1 and FAT2
    let zero_sector = [0u8; SECTOR_SIZE];
    let mut fat0 = [0u8; SECTOR_SIZE];
    fat0[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes()); // Cluster 0: Media descriptor
    fat0[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes()); // Cluster 1: EOC
    fat0[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes()); // Cluster 2: Root directory EOC

    for fat_idx in 0..u32::from(num_fats) {
        let fat_base = u32::from(reserved_sectors) + fat_idx * fat_sectors;
        dev.write_blocks(u64::from(fat_base), 1, &fat0)
            .map_err(|_| VfsError::IoError)?;
        for s in 1..fat_sectors {
            dev.write_blocks(u64::from(fat_base + s), 1, &zero_sector)
                .map_err(|_| VfsError::IoError)?;
        }
    }

    // 4. Zero Root Directory Cluster (Cluster 2)
    let first_data_sector = u32::from(reserved_sectors) + u32::from(num_fats) * fat_sectors;
    for s in 0..u32::from(sectors_per_cluster) {
        dev.write_blocks(u64::from(first_data_sector + s), 1, &zero_sector)
            .map_err(|_| VfsError::IoError)?;
    }

    dev.flush().map_err(|_| VfsError::IoError)
}

struct Fat32Volume {
    dev_id: usize,
    dev: Arc<dyn BlockDevice>,
    sectors_per_cluster: u32,
    bytes_per_cluster: usize,
    reserved_sectors: u32,
    num_fats: u32,
    fat_sectors: u32,
    root_cluster: u32,
    first_data_sector: u32,
    total_clusters: u32,
}

#[derive(Clone, Debug)]
struct ParsedDirRecord {
    name: String,
    attr: u8,
    first_cluster: u32,
    size: u32,
    dir_cluster: u32,
    dir_offset: usize,
    lfn_start_cluster: u32,
    lfn_start_offset: usize,
}

impl Fat32Volume {
    fn cluster_to_lba(&self, cluster: u32) -> u64 {
        u64::from(self.first_data_sector)
            + u64::from(cluster.saturating_sub(2)) * u64::from(self.sectors_per_cluster)
    }

    fn read_sector(&self, lba: u64, buf: &mut [u8; SECTOR_SIZE]) -> Result<(), VfsError> {
        block::cache::read_sector(self.dev_id, &self.dev, lba, buf).map_err(|_| VfsError::IoError)
    }

    fn write_sector(&self, lba: u64, buf: &[u8; SECTOR_SIZE]) -> Result<(), VfsError> {
        block::cache::write_sector(self.dev_id, &self.dev, lba, buf).map_err(|_| VfsError::IoError)
    }

    fn read_cluster(&self, cluster: u32, out: &mut [u8]) -> Result<(), VfsError> {
        let base_lba = self.cluster_to_lba(cluster);
        let mut sec = [0u8; SECTOR_SIZE];
        for s in 0..self.sectors_per_cluster as usize {
            self.read_sector(base_lba + s as u64, &mut sec)?;
            let off = s * SECTOR_SIZE;
            out[off..off + SECTOR_SIZE].copy_from_slice(&sec);
        }
        Ok(())
    }

    fn write_cluster(&self, cluster: u32, data: &[u8]) -> Result<(), VfsError> {
        let base_lba = self.cluster_to_lba(cluster);
        let mut sec = [0u8; SECTOR_SIZE];
        for s in 0..self.sectors_per_cluster as usize {
            let off = s * SECTOR_SIZE;
            sec.copy_from_slice(&data[off..off + SECTOR_SIZE]);
            self.write_sector(base_lba + s as u64, &sec)?;
        }
        Ok(())
    }

    fn read_fat_entry(&self, cluster: u32) -> Result<u32, VfsError> {
        let fat_offset = (cluster as usize) * 4;
        let fat_lba = u64::from(self.reserved_sectors) + (fat_offset / SECTOR_SIZE) as u64;
        let ent_offset = fat_offset % SECTOR_SIZE;
        let mut sec = [0u8; SECTOR_SIZE];
        self.read_sector(fat_lba, &mut sec)?;
        let raw = u32::from_le_bytes(sec[ent_offset..ent_offset + 4].try_into().unwrap());
        Ok(raw & 0x0FFF_FFFF)
    }

    fn write_fat_entry(&self, cluster: u32, value: u32) -> Result<(), VfsError> {
        let fat_offset = (cluster as usize) * 4;
        let sec_index = (fat_offset / SECTOR_SIZE) as u64;
        let ent_offset = fat_offset % SECTOR_SIZE;

        for f in 0..self.num_fats {
            let lba =
                u64::from(self.reserved_sectors) + u64::from(f * self.fat_sectors) + sec_index;
            let mut sec = [0u8; SECTOR_SIZE];
            self.read_sector(lba, &mut sec)?;
            let old = u32::from_le_bytes(sec[ent_offset..ent_offset + 4].try_into().unwrap());
            let merged = (old & 0xF000_0000) | (value & 0x0FFF_FFFF);
            sec[ent_offset..ent_offset + 4].copy_from_slice(&merged.to_le_bytes());
            self.write_sector(lba, &sec)?;
        }
        Ok(())
    }

    fn cluster_chain(&self, start_cluster: u32) -> Result<Vec<u32>, VfsError> {
        if start_cluster < 2 {
            return Ok(Vec::new());
        }
        let mut chain = Vec::new();
        let mut cur = start_cluster;
        let max_len = (self.total_clusters as usize).min(65536);

        while (2..FAT_EOC).contains(&cur) && chain.len() < max_len {
            chain.push(cur);
            cur = self.read_fat_entry(cur)?;
        }
        Ok(chain)
    }

    fn alloc_cluster(&self, prev_cluster: Option<u32>) -> Result<u32, VfsError> {
        let max_cluster = self.total_clusters + 2;
        for c in 2..max_cluster {
            if self.read_fat_entry(c)? == 0 {
                self.write_fat_entry(c, FAT_MARK_EOC)?;
                if let Some(prev) = prev_cluster
                    && prev >= 2
                {
                    self.write_fat_entry(prev, c)?;
                }
                let zero = vec![0u8; self.bytes_per_cluster];
                self.write_cluster(c, &zero)?;
                return Ok(c);
            }
        }
        Err(VfsError::NoSpace)
    }

    fn free_chain(&self, start_cluster: u32) -> Result<(), VfsError> {
        let chain = self.cluster_chain(start_cluster)?;
        for c in chain {
            self.write_fat_entry(c, 0)?;
        }
        Ok(())
    }

    fn count_free_clusters(&self) -> Result<u64, VfsError> {
        let mut free = 0u64;
        let max_cluster = self.total_clusters + 2;
        for c in 2..max_cluster {
            if self.read_fat_entry(c)? == 0 {
                free += 1;
            }
        }
        Ok(free)
    }

    fn parse_directory(&self, dir_cluster: u32) -> Result<Vec<ParsedDirRecord>, VfsError> {
        let chain = self.cluster_chain(dir_cluster)?;
        let mut records = Vec::new();
        let mut cluster_buf = vec![0u8; self.bytes_per_cluster];

        let mut lfn_parts: Vec<(u8, String)> = Vec::new();
        let mut lfn_start: Option<(u32, usize)> = None;

        for &cluster in &chain {
            self.read_cluster(cluster, &mut cluster_buf)?;
            for off in (0..self.bytes_per_cluster).step_by(32) {
                let ent = &cluster_buf[off..off + 32];
                let first_byte = ent[0];
                if first_byte == 0x00 {
                    return Ok(records);
                }
                if first_byte == 0xE5 {
                    lfn_parts.clear();
                    lfn_start = None;
                    continue;
                }
                let attr = ent[11];
                if attr == ATTR_LFN {
                    let seq = first_byte & 0x1F;
                    if (first_byte & 0x40) != 0 {
                        lfn_parts.clear();
                        lfn_start = Some((cluster, off));
                    }
                    let part = decode_lfn_entry(ent);
                    lfn_parts.push((seq, part));
                    continue;
                }
                if (attr & ATTR_VOLUME_ID) != 0 {
                    lfn_parts.clear();
                    lfn_start = None;
                    continue;
                }

                let name = if !lfn_parts.is_empty() {
                    lfn_parts.sort_by_key(|(seq, _)| *seq);
                    let mut full = String::new();
                    for (_, p) in &lfn_parts {
                        full.push_str(p);
                    }
                    lfn_parts.clear();
                    full
                } else {
                    decode_short_name(&ent[0..11], ent[12])
                };

                let (l_clus, l_off) = lfn_start.take().unwrap_or((cluster, off));
                let hi = u16::from_le_bytes([ent[20], ent[21]]) as u32;
                let lo = u16::from_le_bytes([ent[26], ent[27]]) as u32;
                let first_cluster = (hi << 16) | lo;
                let size = u32::from_le_bytes([ent[28], ent[29], ent[30], ent[31]]);

                records.push(ParsedDirRecord {
                    name,
                    attr,
                    first_cluster,
                    size,
                    dir_cluster: cluster,
                    dir_offset: off,
                    lfn_start_cluster: l_clus,
                    lfn_start_offset: l_off,
                });
            }
        }
        Ok(records)
    }

    fn update_dir_entry_meta(
        &self,
        dir_cluster: u32,
        dir_offset: usize,
        first_cluster: u32,
        new_size: u32,
    ) -> Result<(), VfsError> {
        let mut cluster_buf = vec![0u8; self.bytes_per_cluster];
        self.read_cluster(dir_cluster, &mut cluster_buf)?;
        let ent = &mut cluster_buf[dir_offset..dir_offset + 32];
        let hi = (first_cluster >> 16) as u16;
        let lo = (first_cluster & 0xFFFF) as u16;
        ent[20..22].copy_from_slice(&hi.to_le_bytes());
        ent[26..28].copy_from_slice(&lo.to_le_bytes());
        ent[28..32].copy_from_slice(&new_size.to_le_bytes());
        self.write_cluster(dir_cluster, &cluster_buf)
    }

    fn allocate_dir_entries(
        &self,
        dir_cluster: u32,
        raw_entries: &[[u8; 32]],
    ) -> Result<(u32, usize), VfsError> {
        let needed = raw_entries.len();
        let mut chain = self.cluster_chain(dir_cluster)?;
        let mut cluster_buf = vec![0u8; self.bytes_per_cluster];

        for &cluster in &chain {
            self.read_cluster(cluster, &mut cluster_buf)?;
            let total_slots = self.bytes_per_cluster / 32;
            let mut run_start = 0usize;
            let mut run_len = 0usize;

            for slot in 0..total_slots {
                let off = slot * 32;
                let b0 = cluster_buf[off];
                if b0 == 0x00 || b0 == 0xE5 {
                    if run_len == 0 {
                        run_start = slot;
                    }
                    run_len += 1;
                    if run_len == needed {
                        for (i, raw) in raw_entries.iter().enumerate() {
                            let dest_off = (run_start + i) * 32;
                            cluster_buf[dest_off..dest_off + 32].copy_from_slice(raw);
                        }
                        self.write_cluster(cluster, &cluster_buf)?;
                        let short_off = (run_start + needed - 1) * 32;
                        return Ok((cluster, short_off));
                    }
                } else {
                    run_len = 0;
                }
            }
        }

        // Extend directory cluster chain by 1 cluster
        let last = chain.last().copied();
        let new_cluster = self.alloc_cluster(last)?;
        chain.push(new_cluster);
        cluster_buf.fill(0);
        for (i, raw) in raw_entries.iter().enumerate() {
            let dest_off = i * 32;
            cluster_buf[dest_off..dest_off + 32].copy_from_slice(raw);
        }
        self.write_cluster(new_cluster, &cluster_buf)?;
        let short_off = (needed - 1) * 32;
        Ok((new_cluster, short_off))
    }

    fn delete_dir_record(&self, rec: &ParsedDirRecord) -> Result<(), VfsError> {
        let mut cluster_buf = vec![0u8; self.bytes_per_cluster];
        if rec.lfn_start_cluster == rec.dir_cluster {
            self.read_cluster(rec.dir_cluster, &mut cluster_buf)?;
            for off in (rec.lfn_start_offset..=rec.dir_offset).step_by(32) {
                cluster_buf[off] = 0xE5;
            }
            self.write_cluster(rec.dir_cluster, &cluster_buf)?;
        } else {
            self.read_cluster(rec.dir_cluster, &mut cluster_buf)?;
            cluster_buf[rec.dir_offset] = 0xE5;
            self.write_cluster(rec.dir_cluster, &cluster_buf)?;
        }
        Ok(())
    }
}

fn decode_short_name(raw_11: &[u8], nt_res: u8) -> String {
    let base_raw = &raw_11[0..8];
    let ext_raw = &raw_11[8..11];
    let lower_base = (nt_res & 0x08) != 0;
    let lower_ext = (nt_res & 0x10) != 0;

    let mut base = String::from(core::str::from_utf8(base_raw).unwrap_or("").trim_end());
    if lower_base {
        base = base.to_ascii_lowercase();
    }
    let mut ext = String::from(core::str::from_utf8(ext_raw).unwrap_or("").trim_end());
    if lower_ext {
        ext = ext.to_ascii_lowercase();
    }
    if ext.is_empty() {
        base
    } else {
        format!("{}.{}", base, ext)
    }
}

fn decode_lfn_entry(ent: &[u8]) -> String {
    let offsets = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    let mut out = String::new();
    for &off in &offsets {
        let u = u16::from_le_bytes([ent[off], ent[off + 1]]);
        if u == 0x0000 || u == 0xFFFF {
            break;
        }
        if let Some(ch) = char::from_u32(u32::from(u)) {
            out.push(ch);
        }
    }
    out
}

fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for &b in short_name {
        sum = (if (sum & 1) != 0 { 0x80u8 } else { 0u8 })
            .wrapping_add(sum >> 1)
            .wrapping_add(b);
    }
    sum
}

fn build_dir_entries(name: &str, attr: u8, first_cluster: u32, size: u32) -> Vec<[u8; 32]> {
    let mut short_name = [b' '; 11];
    let upper = name.to_ascii_uppercase();
    let (base_part, ext_part) = match upper.rfind('.') {
        Some(idx) if idx > 0 => (&upper[..idx], &upper[idx + 1..]),
        _ => (upper.as_str(), ""),
    };

    for (i, b) in base_part
        .bytes()
        .filter(|&c| c != b' ' && c != b'.')
        .take(8)
        .enumerate()
    {
        short_name[i] = b;
    }
    for (i, b) in ext_part
        .bytes()
        .filter(|&c| c != b' ' && c != b'.')
        .take(3)
        .enumerate()
    {
        short_name[8 + i] = b;
    }

    let is_exact_upper_8_3 = {
        let reconstructed = decode_short_name(&short_name, 0);
        reconstructed == name
    };

    let mut entries = Vec::new();

    if !is_exact_upper_8_3 {
        let checksum = lfn_checksum(&short_name);
        let mut utf16: Vec<u16> = name.encode_utf16().collect();
        utf16.push(0x0000);
        let num_lfn = utf16.len().div_ceil(13);
        let offsets = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];

        for seq_idx in (0..num_lfn).rev() {
            let mut lfn = [0xFFu8; 32];
            let mut seq = (seq_idx + 1) as u8;
            if seq_idx + 1 == num_lfn {
                seq |= 0x40;
            }
            lfn[0] = seq;
            lfn[11] = ATTR_LFN;
            lfn[12] = 0;
            lfn[13] = checksum;
            lfn[26] = 0;
            lfn[27] = 0;

            for (char_i, &byte_off) in offsets.iter().enumerate() {
                let u_idx = seq_idx * 13 + char_i;
                let word = if u_idx < utf16.len() {
                    utf16[u_idx]
                } else {
                    0xFFFF
                };
                lfn[byte_off..byte_off + 2].copy_from_slice(&word.to_le_bytes());
            }
            entries.push(lfn);
        }
    }

    let mut short_ent = [0u8; 32];
    short_ent[0..11].copy_from_slice(&short_name);
    short_ent[11] = attr;
    let hi = (first_cluster >> 16) as u16;
    let lo = (first_cluster & 0xFFFF) as u16;
    short_ent[20..22].copy_from_slice(&hi.to_le_bytes());
    short_ent[26..28].copy_from_slice(&lo.to_le_bytes());
    short_ent[28..32].copy_from_slice(&size.to_le_bytes());
    entries.push(short_ent);

    entries
}

struct Fat32NodeMeta {
    first_cluster: u32,
    size: u32,
    dir_cluster: u32,
    dir_offset: usize,
}

pub struct Fat32Inode {
    vol: Arc<Fat32Volume>,
    ino: u64,
    kind: InodeKind,
    attr: u8,
    meta: SpinLock<Fat32NodeMeta>,
}

impl Fat32Inode {
    fn from_record(vol: Arc<Fat32Volume>, rec: &ParsedDirRecord) -> Arc<Self> {
        let kind = if (rec.attr & ATTR_DIRECTORY) != 0 {
            InodeKind::Directory
        } else {
            InodeKind::Regular
        };
        let ino = ((u64::from(rec.dir_cluster)) << 16) | (rec.dir_offset as u64);
        Arc::new(Self {
            vol,
            ino: ino.max(3),
            kind,
            attr: rec.attr,
            meta: SpinLock::new(Fat32NodeMeta {
                first_cluster: rec.first_cluster,
                size: rec.size,
                dir_cluster: rec.dir_cluster,
                dir_offset: rec.dir_offset,
            }),
        })
    }
}

impl Inode for Fat32Inode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let m = self.meta.lock();
        let mode = if self.kind == InodeKind::Directory {
            0o755
        } else if (self.attr & ATTR_READ_ONLY) != 0 {
            0o444
        } else {
            0o644
        };
        let size = if self.kind == InodeKind::Directory {
            self.vol.bytes_per_cluster as u64
        } else {
            u64::from(m.size)
        };
        Ok(Stat::simple(self.ino, self.kind, mode, size))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        if self.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        let m = self.meta.lock();
        let file_size = u64::from(m.size);
        if offset >= file_size || buf.is_empty() || m.first_cluster < 2 {
            return Ok(0);
        }

        let to_read = (file_size - offset).min(buf.len() as u64) as usize;
        let chain = self.vol.cluster_chain(m.first_cluster)?;
        let bpc = self.vol.bytes_per_cluster;
        let mut cluster_buf = vec![0u8; bpc];
        let mut done = 0usize;

        while done < to_read {
            let cur_off = (offset as usize) + done;
            let clus_idx = cur_off / bpc;
            let clus_off = cur_off % bpc;
            if clus_idx >= chain.len() {
                break;
            }
            self.vol.read_cluster(chain[clus_idx], &mut cluster_buf)?;
            let chunk = (bpc - clus_off).min(to_read - done);
            buf[done..done + chunk].copy_from_slice(&cluster_buf[clus_off..clus_off + chunk]);
            done += chunk;
        }
        Ok(done)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, VfsError> {
        if self.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        if buf.is_empty() {
            return Ok(0);
        }

        let mut m = self.meta.lock();
        let end_pos = (offset as usize)
            .checked_add(buf.len())
            .ok_or(VfsError::NoSpace)?;
        if end_pos > u32::MAX as usize {
            return Err(VfsError::NoSpace);
        }

        if m.first_cluster < 2 {
            m.first_cluster = self.vol.alloc_cluster(None)?;
        }

        let bpc = self.vol.bytes_per_cluster;
        let needed_clusters = end_pos.div_ceil(bpc).max(1);
        let mut chain = self.vol.cluster_chain(m.first_cluster)?;
        while chain.len() < needed_clusters {
            let last = chain.last().copied();
            let next = self.vol.alloc_cluster(last)?;
            chain.push(next);
        }

        let mut cluster_buf = vec![0u8; bpc];
        let mut done = 0usize;
        while done < buf.len() {
            let cur_off = (offset as usize) + done;
            let clus_idx = cur_off / bpc;
            let clus_off = cur_off % bpc;
            let chunk = (bpc - clus_off).min(buf.len() - done);

            if clus_off != 0 || chunk != bpc {
                self.vol.read_cluster(chain[clus_idx], &mut cluster_buf)?;
            }
            cluster_buf[clus_off..clus_off + chunk].copy_from_slice(&buf[done..done + chunk]);
            self.vol.write_cluster(chain[clus_idx], &cluster_buf)?;
            done += chunk;
        }

        if end_pos as u32 > m.size {
            m.size = end_pos as u32;
        }
        if m.dir_cluster >= 2 {
            self.vol
                .update_dir_entry_meta(m.dir_cluster, m.dir_offset, m.first_cluster, m.size)?;
        }
        Ok(done)
    }

    fn truncate(&self, size: u64) -> Result<(), VfsError> {
        if self.kind == InodeKind::Directory {
            return Err(VfsError::IsADirectory);
        }
        let mut m = self.meta.lock();
        if size == 0 && m.first_cluster >= 2 {
            self.vol.free_chain(m.first_cluster)?;
            m.first_cluster = 0;
        }
        m.size = size as u32;
        if m.dir_cluster >= 2 {
            self.vol
                .update_dir_entry_meta(m.dir_cluster, m.dir_offset, m.first_cluster, m.size)?;
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        if self.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        let dir_cluster = self.meta.lock().first_cluster;
        let records = self.vol.parse_directory(dir_cluster)?;
        for rec in &records {
            if rec.name.eq_ignore_ascii_case(name) {
                return Ok(Self::from_record(self.vol.clone(), rec));
            }
        }
        Err(VfsError::NotFound)
    }

    fn create(&self, name: &str, kind: InodeKind, _mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        if self.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            return Err(VfsError::InvalidArgument);
        }
        let dir_cluster = self.meta.lock().first_cluster;
        let records = self.vol.parse_directory(dir_cluster)?;
        if records.iter().any(|r| r.name.eq_ignore_ascii_case(name)) {
            return Err(VfsError::AlreadyExists);
        }

        let (attr, first_cluster) = match kind {
            InodeKind::Regular => (ATTR_ARCHIVE, 0u32),
            InodeKind::Directory => {
                let new_clus = self.vol.alloc_cluster(None)?;
                let mut dot_buf = vec![0u8; self.vol.bytes_per_cluster];
                // "." entry
                dot_buf[0..11].copy_from_slice(b".          ");
                dot_buf[11] = ATTR_DIRECTORY;
                dot_buf[20..22].copy_from_slice(&((new_clus >> 16) as u16).to_le_bytes());
                dot_buf[26..28].copy_from_slice(&((new_clus & 0xFFFF) as u16).to_le_bytes());
                // ".." entry
                dot_buf[32..43].copy_from_slice(b"..         ");
                dot_buf[43] = ATTR_DIRECTORY;
                let parent_clus = if dir_cluster == self.vol.root_cluster {
                    0
                } else {
                    dir_cluster
                };
                dot_buf[52..54].copy_from_slice(&((parent_clus >> 16) as u16).to_le_bytes());
                dot_buf[58..60].copy_from_slice(&((parent_clus & 0xFFFF) as u16).to_le_bytes());
                self.vol.write_cluster(new_clus, &dot_buf)?;
                (ATTR_DIRECTORY, new_clus)
            }
            _ => return Err(VfsError::NotSupported),
        };

        let raw_entries = build_dir_entries(name, attr, first_cluster, 0);
        let (ent_cluster, ent_offset) = self.vol.allocate_dir_entries(dir_cluster, &raw_entries)?;

        let rec = ParsedDirRecord {
            name: String::from(name),
            attr,
            first_cluster,
            size: 0,
            dir_cluster: ent_cluster,
            dir_offset: ent_offset,
            lfn_start_cluster: ent_cluster,
            lfn_start_offset: ent_offset,
        };
        Ok(Self::from_record(self.vol.clone(), &rec))
    }

    fn mkdir(&self, name: &str, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
        self.create(name, InodeKind::Directory, mode)
    }

    fn unlink(&self, name: &str) -> Result<(), VfsError> {
        if self.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        let dir_cluster = self.meta.lock().first_cluster;
        let records = self.vol.parse_directory(dir_cluster)?;
        let rec = records
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .ok_or(VfsError::NotFound)?;
        if (rec.attr & ATTR_DIRECTORY) != 0 {
            return Err(VfsError::IsADirectory);
        }
        if rec.first_cluster >= 2 {
            self.vol.free_chain(rec.first_cluster)?;
        }
        self.vol.delete_dir_record(rec)
    }

    fn rmdir(&self, name: &str) -> Result<(), VfsError> {
        if self.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        let dir_cluster = self.meta.lock().first_cluster;
        let records = self.vol.parse_directory(dir_cluster)?;
        let rec = records
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .ok_or(VfsError::NotFound)?;
        if (rec.attr & ATTR_DIRECTORY) == 0 {
            return Err(VfsError::NotADirectory);
        }
        let sub_records = self.vol.parse_directory(rec.first_cluster)?;
        if sub_records.iter().any(|r| r.name != "." && r.name != "..") {
            return Err(VfsError::NotEmpty);
        }
        if rec.first_cluster >= 2 {
            self.vol.free_chain(rec.first_cluster)?;
        }
        self.vol.delete_dir_record(rec)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        if self.kind != InodeKind::Directory {
            return Err(VfsError::NotADirectory);
        }
        let dir_cluster = self.meta.lock().first_cluster;
        let records = self.vol.parse_directory(dir_cluster)?;
        let mut entries = Vec::new();

        if !records.iter().any(|r| r.name == ".") {
            entries.push(DirEntry {
                ino: self.ino,
                name: String::from("."),
                kind: InodeKind::Directory,
            });
        }
        if !records.iter().any(|r| r.name == "..") {
            entries.push(DirEntry {
                ino: 2,
                name: String::from(".."),
                kind: InodeKind::Directory,
            });
        }

        for rec in &records {
            let kind = if (rec.attr & ATTR_DIRECTORY) != 0 {
                InodeKind::Directory
            } else {
                InodeKind::Regular
            };
            let ino = (((u64::from(rec.dir_cluster)) << 16) | (rec.dir_offset as u64)).max(3);
            entries.push(DirEntry {
                ino,
                name: rec.name.clone(),
                kind,
            });
        }
        Ok(entries)
    }

    fn sync(&self) -> Result<(), VfsError> {
        block::cache::record_flush();
        self.vol.dev.flush().map_err(|_| VfsError::IoError)
    }
}

pub struct Fat32Fs {
    vol: Arc<Fat32Volume>,
    root: Arc<Fat32Inode>,
}

impl Fat32Fs {
    pub fn mount(dev_id: usize, dev: Arc<dyn BlockDevice>) -> Result<Arc<Self>, VfsError> {
        if dev.info().block_size != SECTOR_SIZE {
            return Err(VfsError::InvalidArgument);
        }
        let mut bpb = [0u8; SECTOR_SIZE];
        block::cache::read_sector(dev_id, &dev, 0, &mut bpb).map_err(|_| VfsError::IoError)?;

        if bpb[510] != 0x55 || bpb[511] != 0xAA {
            return Err(VfsError::InvalidArgument);
        }
        let bytes_per_sector = u16::from_le_bytes([bpb[11], bpb[12]]) as usize;
        let sectors_per_cluster = u32::from(bpb[13]);
        let reserved_sectors = u32::from(u16::from_le_bytes([bpb[14], bpb[15]]));
        let num_fats = u32::from(bpb[16]);
        let total_sec_16 = u32::from(u16::from_le_bytes([bpb[19], bpb[20]]));
        let total_sec_32 = u32::from_le_bytes([bpb[32], bpb[33], bpb[34], bpb[35]]);
        let total_sectors = if total_sec_16 != 0 {
            total_sec_16
        } else {
            total_sec_32
        };
        let fat_sz_16 = u32::from(u16::from_le_bytes([bpb[22], bpb[23]]));
        let fat_sz_32 = u32::from_le_bytes([bpb[36], bpb[37], bpb[38], bpb[39]]);
        let fat_sectors = if fat_sz_16 != 0 { fat_sz_16 } else { fat_sz_32 };
        let root_cluster = u32::from_le_bytes([bpb[44], bpb[45], bpb[46], bpb[47]]);

        if bytes_per_sector != SECTOR_SIZE
            || sectors_per_cluster == 0
            || reserved_sectors == 0
            || num_fats == 0
            || fat_sectors == 0
            || root_cluster < 2
        {
            return Err(VfsError::InvalidArgument);
        }

        let first_data_sector = reserved_sectors + num_fats * fat_sectors;
        if total_sectors <= first_data_sector {
            return Err(VfsError::InvalidArgument);
        }
        let data_sectors = total_sectors - first_data_sector;
        let total_clusters = data_sectors / sectors_per_cluster;

        let vol = Arc::new(Fat32Volume {
            dev_id,
            dev,
            sectors_per_cluster,
            bytes_per_cluster: (sectors_per_cluster as usize) * SECTOR_SIZE,
            reserved_sectors,
            num_fats,
            fat_sectors,
            root_cluster,
            first_data_sector,
            total_clusters,
        });

        let root = Arc::new(Fat32Inode {
            vol: vol.clone(),
            ino: 2,
            kind: InodeKind::Directory,
            attr: ATTR_DIRECTORY,
            meta: SpinLock::new(Fat32NodeMeta {
                first_cluster: root_cluster,
                size: 0,
                dir_cluster: 0,
                dir_offset: 0,
            }),
        });

        Ok(Arc::new(Self { vol, root }))
    }
}

impl FileSystem for Fat32Fs {
    fn fs_type(&self) -> &'static str {
        "vfat"
    }

    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }

    fn statfs(&self) -> Result<StatFs, VfsError> {
        let free_clusters = self.vol.count_free_clusters().unwrap_or(0);
        Ok(StatFs {
            magic: MSDOS_SUPER_MAGIC,
            block_size: self.vol.bytes_per_cluster as u64,
            total_blocks: u64::from(self.vol.total_clusters),
            free_blocks: free_clusters,
            total_files: 0,
            free_files: 0,
            max_name_len: 255,
        })
    }

    fn sync(&self) -> Result<(), VfsError> {
        block::cache::record_flush();
        self.vol.dev.flush().map_err(|_| VfsError::IoError)
    }
}
