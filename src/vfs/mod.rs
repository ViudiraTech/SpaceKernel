/*
 *
 *       src/vfs/mod.rs
 *       Linux ABI Virtual File System (VFS) core, mount table and path resolution
 *
 *       2026/10/23 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Industrial-grade VFS subsystem providing mount management, symlink-aware path
//! resolution, per-task file descriptor tables, RamFS (`/`), DevFS (`/dev`),
//! ProcFS (`/proc`), and FAT32 (`/mnt`).

pub mod abi;
pub mod devfs;
pub mod devtmpfs;
pub mod fat32;
pub mod fd;
pub mod inode;
pub mod path;
pub mod procfs;
#[cfg(feature = "boot-self-test")]
pub mod pseudo_test;
pub mod ramfs;
pub mod sysfs;
pub mod tree;

pub use fd::{
    FdEntry, FileDescriptorTable, OpenFile, create_pipe_pair, create_pipe_pair_with_flags,
};
pub use inode::{DirEntry, FileSystem, Inode, InodeKind, Stat, StatFs, VfsError};
pub use path::{normalize_path, split_parent_name};

use crate::{block, kinfo, sched, sync::SpinLock};
use alloc::{format, string::String, sync::Arc, vec::Vec};

const MAX_SYMLINK_DEPTH: usize = 8;

#[derive(Clone)]
pub struct MountEntry {
    pub device: String,
    pub mount_point: String,
    pub fs_type: &'static str,
    pub fs: Arc<dyn FileSystem>,
}

static MOUNTS: SpinLock<Vec<MountEntry>> = SpinLock::new(Vec::new());
static KERNEL_FD_TABLE: SpinLock<Option<Arc<SpinLock<FileDescriptorTable>>>> = SpinLock::new(None);

/// Return the current task's file descriptor table, or the kernel fallback table.
pub fn current_fd_table() -> Arc<SpinLock<FileDescriptorTable>> {
    if let Some(tbl) = sched::current_fd_table() {
        return tbl;
    }
    let mut guard = KERNEL_FD_TABLE.lock();
    if let Some(tbl) = guard.as_ref() {
        return tbl.clone();
    }
    let tbl = Arc::new(SpinLock::new(FileDescriptorTable::new_with_stdio()));
    *guard = Some(tbl.clone());
    tbl
}

/// Return a snapshot of all active filesystem mounts.
pub fn list_mounts() -> Vec<MountEntry> {
    MOUNTS.lock().clone()
}

/// Mount a filesystem instance at `mount_point`.
pub fn mount_fs(device: &str, mount_point: &str, fs: Arc<dyn FileSystem>) -> Result<(), VfsError> {
    let mp = normalize_path("/", mount_point);
    let mut mounts = MOUNTS.lock();
    if let Some(existing) = mounts.iter_mut().find(|m| m.mount_point == mp) {
        existing.device = String::from(device);
        existing.fs_type = fs.fs_type();
        existing.fs = fs;
        return Ok(());
    }
    mounts.push(MountEntry {
        device: String::from(device),
        mount_point: mp,
        fs_type: fs.fs_type(),
        fs,
    });
    // Sort by descending mount_point length so longest prefix always matches first
    mounts.sort_by(|a, b| b.mount_point.len().cmp(&a.mount_point.len()));
    Ok(())
}

/// Mount by filesystem type name (`ramfs`, `devfs`, `proc`, `vfat` / `fat32`).
pub fn mount_by_type(source: &str, target: &str, fstype: &str) -> Result<(), VfsError> {
    let mp = normalize_path("/", target);
    if mp != "/" {
        let _ = mkdir_abs(&mp, 0o755);
    }
    let fs: Arc<dyn FileSystem> = match fstype {
        "ramfs" | "tmpfs" => Arc::new(ramfs::RamFs::new()),
        "devfs" | "devtmpfs" => Arc::new(devtmpfs::DevTmpFs),
        "sysfs" => Arc::new(sysfs::SysFs),
        "proc" | "procfs" => Arc::new(procfs::ProcFs::new()),
        "vfat" | "fat32" | "msdos" => {
            let dev_name = source.strip_prefix("/dev/").unwrap_or(source);
            let (dev_id, dev) = block::find_device(dev_name).ok_or(VfsError::NotFound)?;
            fat32::Fat32Fs::mount(dev_id, dev)?
        }
        _ => return Err(VfsError::NotSupported),
    };
    mount_fs(source, &mp, fs)
}

/// Unmount a filesystem mounted at `target`.
pub fn umount(target: &str) -> Result<(), VfsError> {
    let mp = normalize_path("/", target);
    if mp == "/" {
        return Err(VfsError::Busy);
    }
    let mut mounts = MOUNTS.lock();
    let idx = mounts
        .iter()
        .position(|m| m.mount_point == mp)
        .ok_or(VfsError::InvalidArgument)?;
    let removed = mounts.remove(idx);
    drop(mounts);
    removed.fs.sync()
}

/// Find the best-matching mount entry for `abs_path` and return `(MountEntry, relative_subpath)`.
pub fn find_mount(abs_path: &str) -> Result<(MountEntry, String), VfsError> {
    let mounts = MOUNTS.lock();
    for m in mounts.iter() {
        if m.mount_point == "/" {
            let rel = abs_path.strip_prefix('/').unwrap_or(abs_path);
            return Ok((m.clone(), String::from(rel)));
        }
        if abs_path == m.mount_point {
            return Ok((m.clone(), String::new()));
        }
        if let Some(rest) = abs_path.strip_prefix(m.mount_point.as_str())
            && let Some(rel) = rest.strip_prefix('/')
        {
            return Ok((m.clone(), String::from(rel)));
        }
    }
    Err(VfsError::NotFound)
}

/// Resolve `dirfd` and user `path` into a normalized absolute path.
pub fn resolve_at_path(
    table: &FileDescriptorTable,
    dirfd: i32,
    user_path: &str,
) -> Result<String, VfsError> {
    if user_path.is_empty() {
        return Err(VfsError::NotFound);
    }
    if user_path.starts_with('/') {
        return Ok(normalize_path("/", user_path));
    }
    if dirfd == abi::AT_FDCWD {
        return Ok(normalize_path(table.cwd(), user_path));
    }
    if dirfd < 0 {
        return Err(VfsError::BadFileDescriptor);
    }
    let entry = table
        .get(dirfd as usize)
        .ok_or(VfsError::BadFileDescriptor)?;
    let meta = entry.file.inode.metadata()?;
    if meta.kind != InodeKind::Directory {
        return Err(VfsError::NotADirectory);
    }
    Ok(normalize_path(&entry.file.abs_path, user_path))
}

/// Look up an inode by normalized absolute path, crossing mount points and resolving symlinks.
pub fn lookup_abs(
    abs_path: &str,
    follow_final_symlink: bool,
) -> Result<(Arc<dyn Inode>, String), VfsError> {
    lookup_abs_depth(abs_path, follow_final_symlink, 0)
}

fn lookup_abs_depth(
    abs_path: &str,
    follow_final_symlink: bool,
    depth: usize,
) -> Result<(Arc<dyn Inode>, String), VfsError> {
    if depth > MAX_SYMLINK_DEPTH {
        return Err(VfsError::TooManySymlinks);
    }
    let norm = normalize_path("/", abs_path);
    let (mount, rel) = find_mount(&norm)?;
    if rel.is_empty() {
        return Ok((mount.fs.root_inode(), norm));
    }

    let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let mut cur_inode = mount.fs.root_inode();
    let mut cur_path = if mount.mount_point == "/" {
        String::new()
    } else {
        mount.mount_point.clone()
    };

    for (idx, &seg) in parts.iter().enumerate() {
        let is_last = idx + 1 == parts.len();
        cur_path.push('/');
        cur_path.push_str(seg);

        // Check if cur_path is itself a mount point
        if let Ok((sub_mount, sub_rel)) = find_mount(&cur_path)
            && sub_rel.is_empty()
            && sub_mount.mount_point == cur_path
        {
            cur_inode = sub_mount.fs.root_inode();
            continue;
        }

        let next_inode = cur_inode.lookup(seg)?;
        let meta = next_inode.metadata()?;
        if meta.kind == InodeKind::Symlink && (!is_last || follow_final_symlink) {
            let target = next_inode.readlink()?;
            let parent_dir = split_parent_name(&cur_path)
                .map(|(p, _)| p)
                .unwrap_or_else(|| String::from("/"));
            let mut resolved = normalize_path(&parent_dir, &target);
            if !is_last {
                for &rem in &parts[idx + 1..] {
                    if !resolved.ends_with('/') {
                        resolved.push('/');
                    }
                    resolved.push_str(rem);
                }
            }
            return lookup_abs_depth(&resolved, follow_final_symlink, depth + 1);
        }
        cur_inode = next_inode;
    }

    Ok((cur_inode, norm))
}

/// Read directory entries and merge any child mount points directly under `abs_path`.
pub fn readdir_with_mounts(
    abs_path: &str,
    inode: &Arc<dyn Inode>,
) -> Result<Vec<DirEntry>, VfsError> {
    let mut entries = inode.readdir()?;
    let norm = normalize_path("/", abs_path);
    let prefix = if norm == "/" {
        String::from("/")
    } else {
        format!("{}/", norm)
    };

    for m in list_mounts() {
        if m.mount_point == "/" || m.mount_point == norm {
            continue;
        }
        if let Some(rest) = m.mount_point.strip_prefix(prefix.as_str())
            && !rest.is_empty()
            && !rest.contains('/')
            && !entries.iter().any(|e| e.name == rest)
        {
            entries.push(DirEntry {
                ino: 9999,
                name: String::from(rest),
                kind: InodeKind::Directory,
            });
        }
    }

    Ok(entries)
}

/// Open a file by absolute path and `O_*` flags.
pub fn open_abs(abs_path: &str, flags: u32, mode: u32) -> Result<Arc<OpenFile>, VfsError> {
    let norm = normalize_path("/", abs_path);
    let create = (flags & abi::O_CREAT) != 0;
    let excl = (flags & abi::O_EXCL) != 0;
    let trunc = (flags & abi::O_TRUNC) != 0;
    let must_dir = (flags & (abi::O_DIRECTORY_GENERIC | abi::O_DIRECTORY_ARM64)) != 0;

    let (inode, final_path, newly_created) = match lookup_abs(&norm, !excl) {
        Ok((ino, p)) => {
            if create && excl {
                return Err(VfsError::AlreadyExists);
            }
            (ino, p, false)
        }
        Err(VfsError::NotFound) if create => {
            let (parent_path, filename) =
                split_parent_name(&norm).ok_or(VfsError::InvalidArgument)?;
            let (parent_inode, _) = lookup_abs(&parent_path, true)?;
            let created = parent_inode.create(&filename, InodeKind::Regular, mode & 0o7777)?;
            (created, norm, true)
        }
        Err(e) => return Err(e),
    };

    let meta = inode.metadata()?;
    if must_dir && meta.kind != InodeKind::Directory {
        return Err(VfsError::NotADirectory);
    }
    let acc = flags & abi::O_ACCMODE;
    if meta.kind == InodeKind::Directory && (acc == abi::O_WRONLY || acc == abi::O_RDWR || trunc) {
        return Err(VfsError::IsADirectory);
    }
    if !newly_created && meta.kind != InodeKind::Directory && meta.kind != InodeKind::Symlink {
        if (acc == abi::O_RDONLY || acc == abi::O_RDWR) && (meta.mode & 0o444) == 0 {
            return Err(VfsError::PermissionDenied);
        }
        if (acc == abi::O_WRONLY || acc == abi::O_RDWR || trunc) && (meta.mode & 0o222) == 0 {
            return Err(VfsError::PermissionDenied);
        }
    }
    if trunc && meta.kind == InodeKind::Regular && (acc == abi::O_WRONLY || acc == abi::O_RDWR) {
        inode.truncate(0)?;
    }

    let inode = inode.open(flags)?.unwrap_or(inode);
    Ok(Arc::new(OpenFile::new(inode, final_path, flags)))
}

/// Create a directory at normalized absolute path `abs_path`.
pub fn mkdir_abs(abs_path: &str, mode: u32) -> Result<Arc<dyn Inode>, VfsError> {
    let norm = normalize_path("/", abs_path);
    if lookup_abs(&norm, false).is_ok() {
        return Err(VfsError::AlreadyExists);
    }
    let (parent_path, name) = split_parent_name(&norm).ok_or(VfsError::AlreadyExists)?;
    let (parent_inode, _) = lookup_abs(&parent_path, true)?;
    parent_inode.mkdir(&name, mode & 0o7777)
}

/// Remove a file or directory at normalized absolute path `abs_path`.
pub fn unlink_abs(abs_path: &str, remove_dir: bool) -> Result<(), VfsError> {
    let norm = normalize_path("/", abs_path);
    if norm == "/" {
        return Err(VfsError::Busy);
    }
    let (parent_path, name) = split_parent_name(&norm).ok_or(VfsError::InvalidArgument)?;
    let (parent_inode, _) = lookup_abs(&parent_path, true)?;
    if remove_dir {
        parent_inode.rmdir(&name)
    } else {
        parent_inode.unlink(&name)
    }
}

/// Create a symbolic link at `link_abs` pointing to `target`.
pub fn symlink_abs(target: &str, link_abs: &str) -> Result<Arc<dyn Inode>, VfsError> {
    let norm = normalize_path("/", link_abs);
    if lookup_abs(&norm, false).is_ok() {
        return Err(VfsError::AlreadyExists);
    }
    let (parent_path, name) = split_parent_name(&norm).ok_or(VfsError::InvalidArgument)?;
    let (parent_inode, _) = lookup_abs(&parent_path, true)?;
    parent_inode.symlink(&name, target)
}

/// Read symbolic link target at `abs_path`.
pub fn readlink_abs(abs_path: &str) -> Result<String, VfsError> {
    let (inode, _) = lookup_abs(abs_path, false)?;
    inode.readlink()
}

/// Sync all mounted filesystems to their backing block devices.
pub fn sync_all() {
    for m in list_mounts() {
        let _ = m.fs.sync();
    }
}

/// Convenience helper to write a string/byte slice to an absolute file path.
pub fn write_file(abs_path: &str, data: &[u8]) -> Result<(), VfsError> {
    let f = open_abs(abs_path, abi::O_CREAT | abi::O_WRONLY | abi::O_TRUNC, 0o644)?;
    f.write(data)?;
    f.inode.sync()
}

/// Convenience helper to read an entire file at `abs_path` into a `Vec<u8>`.
pub fn read_file(abs_path: &str) -> Result<Vec<u8>, VfsError> {
    let f = open_abs(abs_path, abi::O_RDONLY, 0)?;
    let mut out = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let n = f.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Ok(out)
}

/// Initialize the VFS subsystem, mount `/`, `/dev`, `/proc`, and `/mnt` (FAT32).
pub fn init() {
    // 1. Mount RamFS on `/`
    let root_fs = Arc::new(ramfs::RamFs::new());
    mount_fs("ramfs", "/", root_fs).expect("VFS: failed to mount rootfs");

    for dir in [
        "/dev", "/proc", "/sys", "/tmp", "/mnt", "/etc", "/var", "/home",
    ] {
        let _ = mkdir_abs(dir, 0o755);
    }

    // Populate standard /etc configuration files
    let _ = write_file("/etc/hostname", b"spacekernel\n");
    let _ = write_file(
        "/etc/os-release",
        b"NAME=\"SpaceKernel\"\nVERSION=\"0.2.0\"\nID=spacekernel\nPRETTY_NAME=\"SpaceKernel 0.2.0 (ViudiraTech)\"\n",
    );
    let _ = write_file(
        "/etc/motd",
        b"Welcome to SpaceKernel (Linux ABI VFS + IDE/AHCI/NVMe Storage Subsystem)\n",
    );
    let _ = write_file(
        "/etc/fstab",
        b"ramfs   /      ramfs  defaults 0 0\ndevfs   /dev   devfs  defaults 0 0\nproc    /proc  proc   defaults 0 0\n",
    );

    // 2. Mount DevFS on `/dev` and ProcFS on `/proc`
    crate::device::init();
    let _ = mount_fs("devtmpfs", "/dev", Arc::new(devtmpfs::DevTmpFs));
    let _ = mount_fs("sysfs", "/sys", Arc::new(sysfs::SysFs));
    let _ = mount_fs("proc", "/proc", Arc::new(procfs::ProcFs::new()));

    // 3. Mount the preferred FAT32 partition on `/mnt`
    // Prefer hardware NVMe/AHCI/IDE partitions first if available, falling back to `ram0p1`.
    let candidates = ["nvme0n1p1", "sda1", "hda1", "ram0p1"];
    let mut mounted_fat = false;
    for cand in candidates {
        if let Some((dev_id, dev)) = block::find_device(cand)
            && let Ok(fat_fs) = fat32::Fat32Fs::mount(dev_id, dev)
        {
            let dev_path = format!("/dev/{}", cand);
            if mount_fs(&dev_path, "/mnt", fat_fs).is_ok() {
                kinfo!("VFS: mounted FAT32 ({}) on /mnt", dev_path);
                mounted_fat = true;
                break;
            }
        }
    }

    if mounted_fat {
        let _ = write_file(
            "/mnt/README.TXT",
            b"SpaceKernel FAT32 Volume\r\nBacked by unified block cache and storage driver.\r\n",
        );
        let _ = write_file(
            "/mnt/KERNEL.INI",
            b"[spacekernel]\r\nvfs=linux-abi\r\nstorage=ide,ahci,nvme,ramdisk\r\n",
        );
    }

    kinfo!(
        "VFS: initialized ({} filesystems mounted)",
        list_mounts().len()
    );
}

#[cfg(feature = "boot-self-test")]
pub fn run_self_test() {
    // 1. Test RamFS create, write, read, symlink, readlink, unlink
    write_file("/tmp/vfs_test.txt", b"hello-vfs-ramfs").expect("VFS ramfs write failed");
    let data = read_file("/tmp/vfs_test.txt").expect("VFS ramfs read failed");
    assert_eq!(&data, b"hello-vfs-ramfs");

    symlink_abs("/tmp/vfs_test.txt", "/tmp/vfs_link.txt").expect("VFS symlink failed");
    assert_eq!(
        readlink_abs("/tmp/vfs_link.txt").expect("VFS readlink failed"),
        "/tmp/vfs_test.txt"
    );
    let link_data = read_file("/tmp/vfs_link.txt").expect("VFS symlink read failed");
    assert_eq!(&link_data, b"hello-vfs-ramfs");
    unlink_abs("/tmp/vfs_link.txt", false).expect("VFS unlink symlink failed");
    unlink_abs("/tmp/vfs_test.txt", false).expect("VFS unlink file failed");

    // 2. Test DevFS (/dev/zero, /dev/null, /dev/urandom, /dev/ram0)
    let f_zero = open_abs("/dev/zero", abi::O_RDONLY, 0).expect("open /dev/zero failed");
    let mut zbuf = [0xAAu8; 32];
    assert_eq!(f_zero.read(&mut zbuf).unwrap(), 32);
    assert!(zbuf.iter().all(|&b| b == 0));

    let f_null = open_abs("/dev/null", abi::O_RDWR, 0).expect("open /dev/null failed");
    assert_eq!(f_null.write(b"discarded").unwrap(), 9);
    assert_eq!(f_null.read(&mut zbuf).unwrap(), 0);

    let f_blk = open_abs("/dev/ram0", abi::O_RDONLY, 0).expect("open /dev/ram0 failed");
    let mut mbr_sig = [0u8; 2];
    assert_eq!(f_blk.pread(510, &mut mbr_sig).unwrap(), 2);
    assert_eq!(mbr_sig, [0x55, 0xAA]);

    // 3. Test ProcFS (/proc/version, /proc/mounts, /proc/partitions)
    let ver = read_file("/proc/version").expect("read /proc/version failed");
    assert!(ver.starts_with(b"SpaceOS-9 version"));
    let mounts = read_file("/proc/mounts").expect("read /proc/mounts failed");
    assert!(!mounts.is_empty());

    // 4. Test FAT32 (/mnt) create, LFN support, write, read, mkdir, unlink, rmdir
    mkdir_abs("/mnt/testdir", 0o755).expect("FAT32 mkdir failed");
    write_file("/mnt/testdir/long_file_name_test.txt", b"FAT32-LFN-OK")
        .expect("FAT32 LFN write failed");
    let fat_read =
        read_file("/mnt/testdir/long_file_name_test.txt").expect("FAT32 LFN read failed");
    assert_eq!(&fat_read, b"FAT32-LFN-OK");
    unlink_abs("/mnt/testdir/long_file_name_test.txt", false).expect("FAT32 unlink failed");
    unlink_abs("/mnt/testdir", true).expect("FAT32 rmdir failed");
    sync_all();

    crate::self_test::verbose_log(format_args!(
        "VFS (RamFS/DevFS/ProcFS/FAT32) self-test passed"
    ));
}
