/*
 *
 *       src/vfs/dcache.rs
 *       Dentry and inode cache with Linux-style lookup and invalidation
 *
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Per-mount dentry and inode caches.
//!
//! The cache follows the Linux VFS model at the level this kernel needs:
//! positive and negative dentries keyed by `(mount, parent inode, name)`,
//! inode identity keyed by `(mount, inode number)`, least-recently-used
//! shrinking, and explicit invalidation when namespaces change.  Filesystem
//! I/O never runs while the cache lock is held; evicted inodes are dropped
//! after the lock is released.

use alloc::{
    collections::BTreeMap,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};

use super::Inode;
use crate::sync::SpinLock;

/// Bound the caches independently of the physical page allocator.  Linux
/// scales these with available memory and a shrinker; a fixed ceiling keeps
/// this kernel's footprint predictable while remaining reclaimable.
const MAX_DENTRIES: usize = 8192;
const MAX_INODES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DentryKey {
    pub mount: u64,
    pub parent: u64,
    pub name: String,
}

impl DentryKey {
    pub fn new(mount: u64, parent: u64, name: &str) -> Self {
        Self {
            mount,
            parent,
            name: String::from(name),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct InodeKey {
    mount: u64,
    ino: u64,
}

pub enum Lookup {
    Positive(Arc<dyn Inode>),
    Negative,
    Miss,
}

struct Dentry {
    inode: Option<Arc<dyn Inode>>,
    path: String,
    tick: u64,
}

struct InodeEntry {
    inode: Weak<dyn Inode>,
    tick: u64,
}

struct Cache {
    dentries: BTreeMap<DentryKey, Dentry>,
    dentry_lru: BTreeMap<u64, DentryKey>,
    inodes: BTreeMap<InodeKey, InodeEntry>,
    inode_lru: BTreeMap<u64, InodeKey>,
    tick: u64,
    hits: u64,
    misses: u64,
    negative_hits: u64,
}

impl Cache {
    fn new() -> Self {
        Self {
            dentries: BTreeMap::new(),
            dentry_lru: BTreeMap::new(),
            inodes: BTreeMap::new(),
            inode_lru: BTreeMap::new(),
            tick: 0,
            hits: 0,
            misses: 0,
            negative_hits: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn touch_dentry(&mut self, key: &DentryKey) {
        let tick = self.next_tick();
        if let Some(entry) = self.dentries.get_mut(key) {
            self.dentry_lru.remove(&entry.tick);
            entry.tick = tick;
            self.dentry_lru.insert(tick, key.clone());
        }
    }

    fn insert_dentry(
        &mut self,
        key: DentryKey,
        inode: Option<Arc<dyn Inode>>,
        path: String,
    ) -> Option<Dentry> {
        let old = self.dentries.remove(&key);
        if let Some(old) = &old {
            self.dentry_lru.remove(&old.tick);
        }
        let tick = self.next_tick();
        self.dentry_lru.insert(tick, key.clone());
        self.dentries.insert(key, Dentry { inode, path, tick });
        old
    }

    fn evict_dentries(&mut self) -> Vec<Dentry> {
        let mut evicted = Vec::new();
        while self.dentries.len() > MAX_DENTRIES {
            let Some(oldest) = self.dentry_lru.keys().next().copied() else {
                break;
            };
            let Some(key) = self.dentry_lru.remove(&oldest) else {
                break;
            };
            if let Some(entry) = self.dentries.remove(&key) {
                evicted.push(entry);
            }
        }
        evicted
    }

    fn insert_inode(&mut self, mount: u64, ino: u64, inode: &Arc<dyn Inode>) {
        let key = InodeKey { mount, ino };
        let tick = self.next_tick();
        if let Some(old) = self.inodes.remove(&key) {
            self.inode_lru.remove(&old.tick);
        }
        self.inode_lru.insert(tick, key);
        self.inodes.insert(
            key,
            InodeEntry {
                inode: Arc::downgrade(inode),
                tick,
            },
        );
        if self.inodes.len() > MAX_INODES {
            let Some(oldest) = self.inode_lru.keys().next().copied() else {
                return;
            };
            if let Some(key) = self.inode_lru.remove(&oldest) {
                self.inodes.remove(&key);
            }
        }
    }

    fn touch_inode(&mut self, key: InodeKey) -> Option<Arc<dyn Inode>> {
        let upgraded = {
            let entry = self.inodes.get(&key)?;
            entry.inode.upgrade()
        };
        if upgraded.is_none() {
            if let Some(entry) = self.inodes.remove(&key) {
                self.inode_lru.remove(&entry.tick);
            }
            return None;
        }
        let tick = self.next_tick();
        if let Some(entry) = self.inodes.get_mut(&key) {
            self.inode_lru.remove(&entry.tick);
            entry.tick = tick;
            self.inode_lru.insert(tick, key);
        }
        upgraded
    }

    fn remove_path_prefix(&mut self, mount: u64, path: &str) -> Vec<Dentry> {
        let prefix = if path == "/" {
            String::from("/")
        } else {
            let mut prefix = String::from(path);
            prefix.push('/');
            prefix
        };
        let keys: Vec<DentryKey> = self
            .dentries
            .iter()
            .filter(|(key, entry)| {
                key.mount == mount && (entry.path == path || entry.path.starts_with(&prefix))
            })
            .map(|(key, _)| key.clone())
            .collect();
        let mut removed = Vec::new();
        for key in keys {
            if let Some(entry) = self.dentries.remove(&key) {
                self.dentry_lru.remove(&entry.tick);
                removed.push(entry);
            }
        }
        removed
    }

    fn remove_inode(&mut self, mount: u64, ino: u64) {
        let key = InodeKey { mount, ino };
        if let Some(entry) = self.inodes.remove(&key) {
            self.inode_lru.remove(&entry.tick);
        }
    }
}

static CACHE: SpinLock<Option<Cache>> = SpinLock::new(None);

fn with_cache<R>(function: impl FnOnce(&mut Cache) -> R) -> R {
    let mut guard = CACHE.lock();
    if guard.is_none() {
        *guard = Some(Cache::new());
    }
    function(guard.as_mut().expect("dentry cache initialized"))
}

pub fn lookup(key: &DentryKey) -> Lookup {
    with_cache(|cache| {
        if let Some(inode) = cache
            .dentries
            .get(key)
            .and_then(|entry| entry.inode.clone())
        {
            cache.hits = cache.hits.saturating_add(1);
            cache.touch_dentry(key);
            return Lookup::Positive(inode);
        }
        if cache.dentries.contains_key(key) {
            cache.negative_hits = cache.negative_hits.saturating_add(1);
            cache.hits = cache.hits.saturating_add(1);
            cache.touch_dentry(key);
            return Lookup::Negative;
        }
        cache.misses = cache.misses.saturating_add(1);
        Lookup::Miss
    })
}

pub fn insert_positive(key: DentryKey, inode: Arc<dyn Inode>, path: &str) {
    let mut evicted = Vec::new();
    with_cache(|cache| {
        if let Some(old) = cache.insert_dentry(key, Some(inode), String::from(path)) {
            evicted.push(old);
        }
        evicted.extend(cache.evict_dentries());
    });
    drop(evicted);
}

pub fn insert_negative(key: DentryKey, path: &str) {
    let mut evicted = Vec::new();
    with_cache(|cache| {
        if let Some(old) = cache.insert_dentry(key, None, String::from(path)) {
            evicted.push(old);
        }
        evicted.extend(cache.evict_dentries());
    });
    drop(evicted);
}

pub fn invalidate(key: &DentryKey) {
    let removed = with_cache(|cache| {
        let entry = cache.dentries.remove(key);
        if let Some(entry) = &entry {
            cache.dentry_lru.remove(&entry.tick);
        }
        entry
    });
    drop(removed);
}

pub fn invalidate_path_prefix(mount: u64, path: &str) {
    let removed = with_cache(|cache| cache.remove_path_prefix(mount, path));
    drop(removed);
}

pub fn invalidate_mount(mount: u64) {
    let mut removed = with_cache(|cache| {
        let keys: Vec<DentryKey> = cache
            .dentries
            .keys()
            .filter(|key| key.mount == mount)
            .cloned()
            .collect();
        let mut removed = Vec::new();
        for key in keys {
            if let Some(entry) = cache.dentries.remove(&key) {
                cache.dentry_lru.remove(&entry.tick);
                removed.push(entry);
            }
        }
        let inode_keys: Vec<InodeKey> = cache
            .inodes
            .keys()
            .filter(|key| key.mount == mount)
            .copied()
            .collect();
        for key in inode_keys {
            cache.remove_inode(key.mount, key.ino);
        }
        removed
    });
    removed.clear();
}

pub fn icache_lookup(mount: u64, ino: u64) -> Option<Arc<dyn Inode>> {
    with_cache(|cache| cache.touch_inode(InodeKey { mount, ino }))
}

pub fn icache_insert(mount: u64, ino: u64, inode: &Arc<dyn Inode>) {
    with_cache(|cache| cache.insert_inode(mount, ino, inode));
}

pub fn icache_remove(mount: u64, ino: u64) {
    with_cache(|cache| cache.remove_inode(mount, ino));
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CacheStats {
    pub dentries: usize,
    pub negative: usize,
    pub unused: usize,
    pub inodes: usize,
    pub live_inodes: usize,
    pub hits: u64,
    pub negative_hits: u64,
    pub misses: u64,
}

pub fn stats() -> CacheStats {
    with_cache(|cache| {
        let mut stats = CacheStats {
            dentries: cache.dentries.len(),
            inodes: cache.inodes.len(),
            hits: cache.hits,
            negative_hits: cache.negative_hits,
            misses: cache.misses,
            ..CacheStats::default()
        };
        for entry in cache.dentries.values() {
            match &entry.inode {
                None => stats.negative += 1,
                Some(inode) if Arc::strong_count(inode) == 1 => stats.unused += 1,
                Some(_) => {}
            }
        }
        stats.live_inodes = cache
            .inodes
            .values()
            .filter(|entry| entry.inode.strong_count() != 0)
            .count();
        stats
    })
}

#[cfg(feature = "boot-self-test")]
pub fn self_test() {
    use crate::{kinfo, vfs};
    use alloc::string::String;

    let _ = vfs::mkdir_abs("/tmp", 0o755);

    // A repeated lookup on a stable filesystem must return the same inode
    // object.  Dynamic filesystems (procfs/sysfs/devtmpfs) intentionally
    // revalidate instead.
    let stable = String::from("/tmp/spk-dcache-stable");
    let _ = vfs::unlink_abs(&stable, false);
    vfs::write_file(&stable, b"stable").expect("create stable cache file");
    let (first, _) = vfs::lookup_abs(&stable, true).expect("stable lookup");
    let (second, _) = vfs::lookup_abs(&stable, true).expect("stable relookup");
    assert!(
        Arc::ptr_eq(&first, &second),
        "dentry cache did not reuse the inode object"
    );
    vfs::unlink_abs(&stable, false).expect("unlink stable cache file");

    // A miss installs a negative dentry; creation replaces it.
    let path = String::from("/tmp/spk-dcache-negative");
    let _ = vfs::unlink_abs(&path, false);
    assert_eq!(
        vfs::lookup_abs(&path, true).err(),
        Some(vfs::VfsError::NotFound)
    );
    assert!(stats().negative != 0, "negative dentry not installed");
    vfs::write_file(&path, b"dcache").expect("create cached file");
    let (created, _) = vfs::lookup_abs(&path, true).expect("lookup created file");
    let (again, _) = vfs::lookup_abs(&path, true).expect("relookup created file");
    assert!(Arc::ptr_eq(&created, &again), "created dentry not reused");
    vfs::unlink_abs(&path, false).expect("unlink cached file");
    assert_eq!(
        vfs::lookup_abs(&path, true).err(),
        Some(vfs::VfsError::NotFound)
    );

    // Recreating a name must not resurrect the previous inode object even if
    // the filesystem reuses the same inode number.
    vfs::write_file(&path, b"dcache2").expect("recreate cached file");
    let (recreated, _) = vfs::lookup_abs(&path, true).expect("lookup recreated file");
    assert!(
        !Arc::ptr_eq(&created, &recreated),
        "recreated inode reused a stale cached object"
    );
    vfs::unlink_abs(&path, false).expect("unlink recreated file");

    // Removing a directory invalidates the whole cached subtree.
    let directory = String::from("/tmp/spk-dcache-dir");
    let _ = vfs::unlink_abs(&directory, true);
    vfs::mkdir_abs(&directory, 0o755).expect("create cache directory");
    let child = String::from("/tmp/spk-dcache-dir/child");
    vfs::write_file(&child, b"x").expect("create cache child");
    assert!(vfs::lookup_abs(&child, true).is_ok());
    assert_eq!(
        vfs::unlink_abs(&directory, true).err(),
        Some(vfs::VfsError::NotEmpty)
    );
    vfs::unlink_abs(&child, false).expect("unlink cache child");
    vfs::unlink_abs(&directory, true).expect("remove cache directory");
    assert_eq!(
        vfs::lookup_abs(&directory, true).err(),
        Some(vfs::VfsError::NotFound)
    );
    assert_eq!(
        vfs::lookup_abs(&child, true).err(),
        Some(vfs::VfsError::NotFound)
    );

    // The Linux-compatible statistics file must be readable and non-empty.
    let stats_text = vfs::read_file("/proc/sys/fs/dentry-state").expect("dentry-state");
    let first_field = core::str::from_utf8(&stats_text)
        .expect("dentry-state UTF-8")
        .split_ascii_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .expect("dentry-state number");
    assert!(first_field > 0, "dentry cache reported empty");
    kinfo!("VFS dentry/inode cache self-test passed");
}
