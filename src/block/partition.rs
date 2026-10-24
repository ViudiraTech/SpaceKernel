/*
 *
 *       src/block/partition.rs
 *       MBR and GPT partition table discovery and slice block devices
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Automatic discovery of MBR and GPT partitions on block devices.

use super::{BlockDevice, BlockDeviceInfo, BlockDeviceKind, BlockError, SECTOR_SIZE};
use crate::kinfo;
use alloc::{format, string::String, sync::Arc, vec::Vec};

/// A partition slice view into a parent `BlockDevice`.
pub struct PartitionDevice {
    parent: Arc<dyn BlockDevice>,
    info: BlockDeviceInfo,
    start_lba: u64,
    sector_count: u64,
    part_type: u8,
}

impl PartitionDevice {
    pub fn new(
        parent: Arc<dyn BlockDevice>,
        name: String,
        start_lba: u64,
        sector_count: u64,
        part_type: u8,
        label: String,
    ) -> Self {
        let parent_info = parent.info();
        let model = if label.is_empty() {
            format!("{} part (type {:#04x})", parent_info.model, part_type)
        } else {
            format!("{} ({})", label, parent_info.model)
        };
        let info = BlockDeviceInfo {
            name,
            model,
            serial: parent_info.serial,
            kind: BlockDeviceKind::Partition,
            block_size: parent_info.block_size,
            total_blocks: sector_count,
            read_only: parent_info.read_only,
            removable: parent_info.removable,
        };
        Self {
            parent,
            info,
            start_lba,
            sector_count,
            part_type,
        }
    }

    pub fn part_type(&self) -> u8 {
        self.part_type
    }

    pub fn start_lba(&self) -> u64 {
        self.start_lba
    }
}

impl BlockDevice for PartitionDevice {
    fn info(&self) -> BlockDeviceInfo {
        self.info.clone()
    }

    fn read_blocks(&self, lba: u64, count: usize, buf: &mut [u8]) -> Result<(), BlockError> {
        let end = lba
            .checked_add(count as u64)
            .ok_or(BlockError::OutOfRange)?;
        if end > self.sector_count {
            return Err(BlockError::OutOfRange);
        }
        self.parent.read_blocks(self.start_lba + lba, count, buf)
    }

    fn write_blocks(&self, lba: u64, count: usize, buf: &[u8]) -> Result<(), BlockError> {
        let end = lba
            .checked_add(count as u64)
            .ok_or(BlockError::OutOfRange)?;
        if end > self.sector_count {
            return Err(BlockError::OutOfRange);
        }
        self.parent.write_blocks(self.start_lba + lba, count, buf)
    }

    fn flush(&self) -> Result<(), BlockError> {
        self.parent.flush()
    }
}

/// Format partition device name following Linux conventions (`sda1` vs `nvme0n1p1` / `ram0p1`).
fn format_partition_name(disk_name: &str, index: usize) -> String {
    if disk_name.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        format!("{}p{}", disk_name, index)
    } else {
        format!("{}{}", disk_name, index)
    }
}

/// Probe a newly registered whole-disk device for MBR or GPT partitions.
pub fn probe_partitions(disk: &Arc<dyn BlockDevice>) -> Vec<Arc<dyn BlockDevice>> {
    let mut discovered: Vec<Arc<dyn BlockDevice>> = Vec::new();
    let info = disk.info();
    if info.kind == BlockDeviceKind::Partition
        || info.block_size != SECTOR_SIZE
        || info.total_blocks < 2
    {
        return discovered;
    }

    let mut mbr = [0u8; SECTOR_SIZE];
    if disk.read_blocks(0, 1, &mut mbr).is_err() {
        return discovered;
    }

    // Check MBR signature 0x55, 0xAA at offset 510..512
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        return discovered;
    }

    // Check if any entry is a GPT Protective MBR (type 0xEE)
    let mut is_gpt = false;
    for i in 0..4 {
        let offset = 446 + i * 16;
        let part_type = mbr[offset + 4];
        if part_type == 0xEE {
            is_gpt = true;
            break;
        }
    }

    if is_gpt && info.total_blocks >= 34 {
        if let Some(gpt_parts) = parse_gpt(disk, &info.name, info.total_blocks) {
            return gpt_parts;
        }
    }

    // Parse standard 4 primary MBR partitions
    for i in 0..4 {
        let offset = 446 + i * 16;
        let part_type = mbr[offset + 4];
        if part_type == 0 || part_type == 0xEE {
            continue;
        }
        let start_lba = u32::from_le_bytes([
            mbr[offset + 8],
            mbr[offset + 9],
            mbr[offset + 10],
            mbr[offset + 11],
        ]) as u64;
        let sector_count = u32::from_le_bytes([
            mbr[offset + 12],
            mbr[offset + 13],
            mbr[offset + 14],
            mbr[offset + 15],
        ]) as u64;

        if start_lba == 0 || sector_count == 0 {
            continue;
        }
        if start_lba.saturating_add(sector_count) > info.total_blocks {
            continue;
        }

        let part_name = format_partition_name(&info.name, i + 1);
        kinfo!(
            "block: {}: MBR partition {} [LBA {}..{}, {} sectors, type {:#04x}]",
            info.name,
            part_name,
            start_lba,
            start_lba + sector_count - 1,
            sector_count,
            part_type
        );
        let part = Arc::new(PartitionDevice::new(
            disk.clone(),
            part_name,
            start_lba,
            sector_count,
            part_type,
            String::new(),
        ));
        discovered.push(part);
    }

    discovered
}

fn parse_gpt(
    disk: &Arc<dyn BlockDevice>,
    disk_name: &str,
    total_blocks: u64,
) -> Option<Vec<Arc<dyn BlockDevice>>> {
    let mut header = [0u8; SECTOR_SIZE];
    disk.read_blocks(1, 1, &mut header).ok()?;

    // Verify "EFI PART" signature
    if &header[0..8] != b"EFI PART" {
        return None;
    }

    let entries_lba = u64::from_le_bytes(header[72..80].try_into().ok()?);
    let num_entries = u32::from_le_bytes(header[80..84].try_into().ok()?) as usize;
    let entry_size = u32::from_le_bytes(header[84..88].try_into().ok()?) as usize;

    if entry_size < 128 || !SECTOR_SIZE.is_multiple_of(entry_size) || entries_lba >= total_blocks {
        return None;
    }

    let mut discovered: Vec<Arc<dyn BlockDevice>> = Vec::new();
    let entries_per_sector = SECTOR_SIZE / entry_size;
    let max_entries = num_entries.min(128);
    let sectors_to_read = max_entries.div_ceil(entries_per_sector);

    let mut sector_buf = [0u8; SECTOR_SIZE];
    let mut part_idx = 0usize;

    for s in 0..sectors_to_read {
        let lba = entries_lba + s as u64;
        if lba >= total_blocks || disk.read_blocks(lba, 1, &mut sector_buf).is_err() {
            break;
        }
        for e in 0..entries_per_sector {
            if part_idx >= max_entries {
                break;
            }
            part_idx += 1;
            let off = e * entry_size;
            let type_guid = &sector_buf[off..off + 16];
            if type_guid.iter().all(|&b| b == 0) {
                continue;
            }
            let first_lba = u64::from_le_bytes(sector_buf[off + 32..off + 40].try_into().unwrap());
            let last_lba = u64::from_le_bytes(sector_buf[off + 40..off + 48].try_into().unwrap());
            if first_lba == 0 || last_lba < first_lba || last_lba >= total_blocks {
                continue;
            }
            let count = last_lba - first_lba + 1;

            // Decode UTF-16LE partition name (36 UTF-16 code units at offset 56..128)
            let mut label = String::new();
            for c in 0..36 {
                let ch_off = off + 56 + c * 2;
                let unit = u16::from_le_bytes([sector_buf[ch_off], sector_buf[ch_off + 1]]);
                if unit == 0 {
                    break;
                }
                if let Some(ch) = char::from_u32(unit as u32) {
                    label.push(ch);
                }
            }

            let part_name = format_partition_name(disk_name, part_idx);
            kinfo!(
                "block: {}: GPT partition {} '{}' [LBA {}..{}, {} sectors]",
                disk_name,
                part_name,
                label,
                first_lba,
                last_lba,
                count
            );
            let part = Arc::new(PartitionDevice::new(
                disk.clone(),
                part_name,
                first_lba,
                count,
                0xEE,
                label,
            ));
            discovered.push(part);
        }
    }

    Some(discovered)
}

/// Write an MBR with a single primary partition entry to LBA 0 of `dev`.
pub fn write_mbr_single_partition(
    dev: &Arc<dyn BlockDevice>,
    part_type: u8,
    start_lba: u32,
    sector_count: u32,
) -> Result<(), BlockError> {
    let mut mbr = [0u8; SECTOR_SIZE];
    let offset = 446;
    mbr[offset] = 0x80; // Bootable/active flag
    mbr[offset + 1] = 0x00;
    mbr[offset + 2] = 0x02;
    mbr[offset + 3] = 0x00;
    mbr[offset + 4] = part_type;
    mbr[offset + 5] = 0xFE;
    mbr[offset + 6] = 0xFF;
    mbr[offset + 7] = 0xFF;
    mbr[offset + 8..offset + 12].copy_from_slice(&start_lba.to_le_bytes());
    mbr[offset + 12..offset + 16].copy_from_slice(&sector_count.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    dev.write_blocks(0, 1, &mbr)?;
    dev.flush()
}
