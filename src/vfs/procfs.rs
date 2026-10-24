/*
 *
 *       src/vfs/procfs.rs
 *       Process and kernel telemetry filesystem (/proc)
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Linux-compatible `/proc` virtual filesystem (`cpuinfo`, `meminfo`, `uptime`,
//! `version`, `mounts`, `filesystems`, `partitions`, `diskstats`, `cmdline`, `stat`,
//! `self/status`, and `<tid>/status`).

use super::{
    abi::PROC_SUPER_MAGIC,
    inode::{DirEntry, FileSystem, Inode, InodeKind, Stat, StatFs, VfsError},
};
use crate::{block, boot, cpuid, mm, sched, time};
use alloc::{format, string::String, sync::Arc, vec::Vec};

enum ProcFileKind {
    CpuInfo,
    MemInfo,
    Uptime,
    Version,
    Mounts,
    Filesystems,
    Partitions,
    DiskStats,
    Cmdline,
    Stat,
    TaskStatus(u64),
}

struct ProcDynamicFile {
    ino: u64,
    kind: ProcFileKind,
}

impl ProcDynamicFile {
    fn generate(&self) -> String {
        match self.kind {
            ProcFileKind::CpuInfo => {
                let mut out = String::new();
                if let Some(info) = cpuid::info() {
                    for core_id in 0..boot::cpu_count() {
                        out.push_str(&format!(
                            "processor\t: {}\nvendor_id\t: {}\nmodel name\t: {}\narch\t\t: {:?}\naddress sizes\t: {:?} bits physical, {:?} bits virtual\n\n",
                            core_id,
                            info.vendor.as_str(),
                            info.model.as_str(),
                            info.architecture,
                            info.physical_bits,
                            info.virtual_bits
                        ));
                    }
                }
                out
            }
            ProcFileKind::MemInfo => {
                let pmm = mm::pmm::stats();
                let total_kb = (pmm.frames * 4096) / 1024;
                let free_kb = (pmm.free * 4096) / 1024;
                let used_kb = total_kb.saturating_sub(free_kb);
                format!(
                    "MemTotal:       {:8} kB\nMemFree:        {:8} kB\nMemAvailable:   {:8} kB\nMemUsed:        {:8} kB\n",
                    total_kb, free_kb, free_kb, used_kb
                )
            }
            ProcFileKind::Uptime => {
                let us = time::uptime_micros();
                let secs = us / 1_000_000;
                let centis = (us % 1_000_000) / 10_000;
                format!("{}.{:02} 0.00\n", secs, centis)
            }
            ProcFileKind::Version => {
                let arch_name = if cfg!(target_arch = "x86_64") {
                    "x86_64"
                } else if cfg!(target_arch = "aarch64") {
                    "aarch64"
                } else {
                    "riscv64"
                };
                format!(
                    "SpaceKernel version 0.2.0 ({}) (Rust 2024) #1 SMP PREEMPT_EEVDF\n",
                    arch_name
                )
            }
            ProcFileKind::Mounts => {
                let mut out = String::new();
                for m in super::list_mounts() {
                    out.push_str(&format!(
                        "{} {} {} rw,relatime 0 0\n",
                        m.device, m.mount_point, m.fs_type
                    ));
                }
                out
            }
            ProcFileKind::Filesystems => {
                String::from("nodev\tramfs\nnodev\tdevfs\nnodev\tproc\n\tvfat\n")
            }
            ProcFileKind::Partitions => {
                let mut out = String::from("major minor  #blocks  name\n\n");
                for (idx, dev) in block::devices().iter().enumerate() {
                    let info = dev.info();
                    let blocks_1k = info.capacity_bytes() / 1024;
                    out.push_str(&format!("   8 {:5} {:9} {}\n", idx, blocks_1k, info.name));
                }
                out
            }
            ProcFileKind::DiskStats => {
                let cache = block::cache::stats();
                format!(
                    "cache_hits: {}\ncache_misses: {}\ncache_writes: {}\ncache_flushes: {}\ncache_evictions: {}\n",
                    cache.hits, cache.misses, cache.writes, cache.flushes, cache.evictions
                )
            }
            ProcFileKind::Cmdline => {
                format!("{}\n", boot::cmdline())
            }
            ProcFileKind::Stat => {
                format!(
                    "cpu  0 0 0 0 0 0 0 0 0 0\nprocesses {}\n",
                    sched::task_list().len()
                )
            }
            ProcFileKind::TaskStatus(tid) => {
                for t in sched::task_list() {
                    if t.tid == tid {
                        return format!(
                            "Name:\tkthread/{}\nState:\t{:?}\nPid:\t{}\nTgid:\t{}\nCpu:\t{}\nRuntimeNs:\t{}\nVoluntary_ctxt_switches:\t{}\nNonvoluntary_ctxt_switches:\t{}\n",
                            t.tid,
                            t.state,
                            t.tid,
                            t.tid,
                            t.cpu,
                            t.runtime_ns,
                            t.voluntary_switches,
                            t.involuntary_switches
                        );
                    }
                }
                format!("Name:\ttask/{}\nPid:\t{}\n", tid, tid)
            }
        }
    }
}

impl Inode for ProcDynamicFile {
    fn metadata(&self) -> Result<Stat, VfsError> {
        let content = self.generate();
        Ok(Stat::simple(
            self.ino,
            InodeKind::Regular,
            0o444,
            content.len() as u64,
        ))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError> {
        let content = self.generate();
        let bytes = content.as_bytes();
        if offset >= bytes.len() as u64 || buf.is_empty() {
            return Ok(0);
        }
        let start = offset as usize;
        let count = (bytes.len() - start).min(buf.len());
        buf[..count].copy_from_slice(&bytes[start..start + count]);
        Ok(count)
    }
}

struct ProcTaskDir {
    tid: u64,
}

impl Inode for ProcTaskDir {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(
            5000 + self.tid,
            InodeKind::Directory,
            0o555,
            0,
        ))
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        match name {
            "status" => Ok(Arc::new(ProcDynamicFile {
                ino: 6000 + self.tid,
                kind: ProcFileKind::TaskStatus(self.tid),
            })),
            _ => Err(VfsError::NotFound),
        }
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        Ok(alloc::vec![
            DirEntry {
                ino: 5000 + self.tid,
                name: String::from("."),
                kind: InodeKind::Directory,
            },
            DirEntry {
                ino: 1,
                name: String::from(".."),
                kind: InodeKind::Directory,
            },
            DirEntry {
                ino: 6000 + self.tid,
                name: String::from("status"),
                kind: InodeKind::Regular,
            },
        ])
    }
}

struct ProcRootInode;

impl Inode for ProcRootInode {
    fn metadata(&self) -> Result<Stat, VfsError> {
        Ok(Stat::simple(1, InodeKind::Directory, 0o555, 0))
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, VfsError> {
        match name {
            "cpuinfo" => Ok(Arc::new(ProcDynamicFile {
                ino: 101,
                kind: ProcFileKind::CpuInfo,
            })),
            "meminfo" => Ok(Arc::new(ProcDynamicFile {
                ino: 102,
                kind: ProcFileKind::MemInfo,
            })),
            "uptime" => Ok(Arc::new(ProcDynamicFile {
                ino: 103,
                kind: ProcFileKind::Uptime,
            })),
            "version" => Ok(Arc::new(ProcDynamicFile {
                ino: 104,
                kind: ProcFileKind::Version,
            })),
            "mounts" => Ok(Arc::new(ProcDynamicFile {
                ino: 105,
                kind: ProcFileKind::Mounts,
            })),
            "filesystems" => Ok(Arc::new(ProcDynamicFile {
                ino: 106,
                kind: ProcFileKind::Filesystems,
            })),
            "partitions" => Ok(Arc::new(ProcDynamicFile {
                ino: 107,
                kind: ProcFileKind::Partitions,
            })),
            "diskstats" => Ok(Arc::new(ProcDynamicFile {
                ino: 108,
                kind: ProcFileKind::DiskStats,
            })),
            "cmdline" => Ok(Arc::new(ProcDynamicFile {
                ino: 109,
                kind: ProcFileKind::Cmdline,
            })),
            "stat" => Ok(Arc::new(ProcDynamicFile {
                ino: 110,
                kind: ProcFileKind::Stat,
            })),
            "self" => {
                let tid = sched::current().map(|t| t.stats().tid).unwrap_or(1);
                Ok(Arc::new(ProcTaskDir { tid }))
            }
            _ => {
                if let Ok(tid) = name.parse::<u64>() {
                    Ok(Arc::new(ProcTaskDir { tid }))
                } else {
                    Err(VfsError::NotFound)
                }
            }
        }
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, VfsError> {
        let mut entries = alloc::vec![
            DirEntry {
                ino: 1,
                name: String::from("."),
                kind: InodeKind::Directory,
            },
            DirEntry {
                ino: 1,
                name: String::from(".."),
                kind: InodeKind::Directory,
            },
            DirEntry {
                ino: 101,
                name: String::from("cpuinfo"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 102,
                name: String::from("meminfo"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 103,
                name: String::from("uptime"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 104,
                name: String::from("version"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 105,
                name: String::from("mounts"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 106,
                name: String::from("filesystems"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 107,
                name: String::from("partitions"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 108,
                name: String::from("diskstats"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 109,
                name: String::from("cmdline"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 110,
                name: String::from("stat"),
                kind: InodeKind::Regular,
            },
            DirEntry {
                ino: 4999,
                name: String::from("self"),
                kind: InodeKind::Directory,
            },
        ];
        for t in sched::task_list() {
            entries.push(DirEntry {
                ino: 5000 + t.tid,
                name: format!("{}", t.tid),
                kind: InodeKind::Directory,
            });
        }
        Ok(entries)
    }
}

pub struct ProcFs {
    root: Arc<ProcRootInode>,
}

impl Default for ProcFs {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcFs {
    pub fn new() -> Self {
        Self {
            root: Arc::new(ProcRootInode),
        }
    }
}

impl FileSystem for ProcFs {
    fn fs_type(&self) -> &'static str {
        "proc"
    }

    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }

    fn statfs(&self) -> Result<StatFs, VfsError> {
        Ok(StatFs {
            magic: PROC_SUPER_MAGIC,
            block_size: 4096,
            total_blocks: 0,
            free_blocks: 0,
            total_files: 0,
            free_files: 0,
            max_name_len: 255,
        })
    }
}
