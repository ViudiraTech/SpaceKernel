/*
 *       src/syscall/metadata.rs
 *       Device-node creation and inode ownership syscalls
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
use crate::{
    user::uaccess,
    vfs::{self, InodeKind, VfsError},
};
fn path_at(dirfd: i32, pointer: u64) -> Result<alloc::string::String, VfsError> {
    let path = uaccess::string(pointer, 4096)?;
    vfs::resolve_at_path(&vfs::current_fd_table().lock(), dirfd, &path)
}
fn result(value: Result<(), VfsError>) -> i64 {
    match value {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}
pub fn mknodat(dirfd: i32, pointer: u64, mode: u32, rdev: u64) -> i64 {
    result((|| {
        let path = path_at(dirfd, pointer)?;
        let (parent, name) = vfs::split_parent_name(&path).ok_or(VfsError::InvalidArgument)?;
        let (parent, _) = vfs::lookup_abs(&parent, true)?;
        match mode & vfs::abi::S_IFMT {
            vfs::abi::S_IFCHR | vfs::abi::S_IFBLK => {
                let kind = if mode & vfs::abi::S_IFMT == vfs::abi::S_IFCHR {
                    InodeKind::CharDevice
                } else {
                    InodeKind::BlockDevice
                };
                let node = crate::device::alias(rdev, kind, mode)?;
                parent.insert_node(&name, node)?;
            }
            0 | vfs::abi::S_IFREG => {
                parent.create(&name, InodeKind::Regular, mode & 0o7777)?;
            }
            vfs::abi::S_IFIFO => {
                parent.create(&name, InodeKind::Fifo, mode & 0o7777)?;
            }
            _ => return Err(VfsError::NotSupported),
        }
        Ok(())
    })())
}
pub fn fchmod(fd: i32, mode: u32) -> i64 {
    result(super::get_open_file(fd).and_then(|f| f.file.inode.chmod(mode)))
}
pub fn fchmodat(dirfd: i32, pointer: u64, mode: u32, flags: u32) -> i64 {
    result((|| {
        if flags & !vfs::abi::AT_SYMLINK_NOFOLLOW != 0 {
            return Err(VfsError::InvalidArgument);
        }
        let path = path_at(dirfd, pointer)?;
        vfs::lookup_abs(&path, flags & vfs::abi::AT_SYMLINK_NOFOLLOW == 0)?
            .0
            .chmod(mode)
    })())
}
pub fn fchown(fd: i32, uid: u32, gid: u32) -> i64 {
    result(super::get_open_file(fd).and_then(|f| {
        let stat = f.file.inode.metadata()?;
        f.file.inode.chown(
            if uid == u32::MAX { stat.uid } else { uid },
            if gid == u32::MAX { stat.gid } else { gid },
        )
    }))
}
pub fn fchownat(dirfd: i32, pointer: u64, uid: u32, gid: u32, flags: u32) -> i64 {
    result((|| {
        if flags & !vfs::abi::AT_SYMLINK_NOFOLLOW != 0 {
            return Err(VfsError::InvalidArgument);
        }
        let path = path_at(dirfd, pointer)?;
        let inode = vfs::lookup_abs(&path, flags & vfs::abi::AT_SYMLINK_NOFOLLOW == 0)?.0;
        let stat = inode.metadata()?;
        inode.chown(
            if uid == u32::MAX { stat.uid } else { uid },
            if gid == u32::MAX { stat.gid } else { gid },
        )
    })())
}
