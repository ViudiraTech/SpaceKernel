/*
 *       src/gpu/abi.rs
 *       ioctl sizes generated from the frozen Linux UAPI snapshot
 *       2026/10/24 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */
pub fn descriptor(nr: u8) -> Option<(usize, u32)> {
    match nr {
        0x00 => Some((64, 3)),  // DRM_IOCTL_VERSION
        0x01 => Some((16, 3)),  // DRM_IOCTL_GET_UNIQUE
        0x02 => Some((4, 2)),   // DRM_IOCTL_GET_MAGIC
        0x07 => Some((16, 3)),  // DRM_IOCTL_SET_VERSION
        0x09 => Some((8, 1)),   // DRM_IOCTL_GEM_CLOSE
        0x0c => Some((16, 3)),  // DRM_IOCTL_GET_CAP
        0x0d => Some((16, 1)),  // DRM_IOCTL_SET_CLIENT_CAP
        0x1e => Some((0, 0)),   // DRM_IOCTL_SET_MASTER
        0x1f => Some((0, 0)),   // DRM_IOCTL_DROP_MASTER
        0x2d => Some((12, 3)),  // DRM_IOCTL_PRIME_HANDLE_TO_FD
        0x2e => Some((12, 3)),  // DRM_IOCTL_PRIME_FD_TO_HANDLE
        0xa0 => Some((64, 3)),  // DRM_IOCTL_MODE_GETRESOURCES
        0xa1 => Some((104, 3)), // DRM_IOCTL_MODE_GETCRTC
        0xa2 => Some((104, 3)), // DRM_IOCTL_MODE_SETCRTC
        0xa6 => Some((20, 3)),  // DRM_IOCTL_MODE_GETENCODER
        0xa7 => Some((80, 3)),  // DRM_IOCTL_MODE_GETCONNECTOR
        0xaa => Some((64, 3)),  // DRM_IOCTL_MODE_GETPROPERTY
        0xac => Some((16, 3)),  // DRM_IOCTL_MODE_GETPROPBLOB
        0xad => Some((28, 3)),  // DRM_IOCTL_MODE_GETFB
        0xae => Some((28, 3)),  // DRM_IOCTL_MODE_ADDFB
        0xaf => Some((4, 3)),   // DRM_IOCTL_MODE_RMFB
        0xb0 => Some((24, 3)),  // DRM_IOCTL_MODE_PAGE_FLIP
        0xb1 => Some((24, 3)),  // DRM_IOCTL_MODE_DIRTYFB
        0xb2 => Some((32, 3)),  // DRM_IOCTL_MODE_CREATE_DUMB
        0xb3 => Some((16, 3)),  // DRM_IOCTL_MODE_MAP_DUMB
        0xb4 => Some((4, 3)),   // DRM_IOCTL_MODE_DESTROY_DUMB
        0xb5 => Some((16, 3)),  // DRM_IOCTL_MODE_GETPLANERESOURCES
        0xb6 => Some((32, 3)),  // DRM_IOCTL_MODE_GETPLANE
        0xb7 => Some((48, 3)),  // DRM_IOCTL_MODE_SETPLANE
        0xb8 => Some((104, 3)), // DRM_IOCTL_MODE_ADDFB2
        0xb9 => Some((32, 3)),  // DRM_IOCTL_MODE_OBJ_GETPROPERTIES
        0xbc => Some((56, 3)),  // DRM_IOCTL_MODE_ATOMIC
        0xbd => Some((16, 3)),  // DRM_IOCTL_MODE_CREATEPROPBLOB
        0xbe => Some((4, 3)),   // DRM_IOCTL_MODE_DESTROYPROPBLOB
        0xce => Some((104, 3)), // DRM_IOCTL_MODE_GETFB2
        0xa3 => Some((28, 3)),  // DRM_IOCTL_MODE_CURSOR
        0xbb => Some((36, 3)),  // DRM_IOCTL_MODE_CURSOR2
        0xbf => Some((8, 3)),   // DRM_IOCTL_SYNCOBJ_CREATE
        0xc0 => Some((8, 3)),   // DRM_IOCTL_SYNCOBJ_DESTROY
        0xc1 => Some((24, 3)),  // DRM_IOCTL_SYNCOBJ_HANDLE_TO_FD
        0xc2 => Some((24, 3)),  // DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE
        0xc3 => Some((40, 3)),  // DRM_IOCTL_SYNCOBJ_WAIT
        0xc4 => Some((16, 3)),  // DRM_IOCTL_SYNCOBJ_RESET
        0xc5 => Some((16, 3)),  // DRM_IOCTL_SYNCOBJ_SIGNAL
        0xca => Some((48, 3)),  // DRM_IOCTL_SYNCOBJ_TIMELINE_WAIT
        0xcb => Some((24, 3)),  // DRM_IOCTL_SYNCOBJ_QUERY
        0xcc => Some((32, 3)),  // DRM_IOCTL_SYNCOBJ_TRANSFER
        0xcd => Some((24, 3)),  // DRM_IOCTL_SYNCOBJ_TIMELINE_SIGNAL
        0x41 => Some((16, 3)),  // DRM_IOCTL_VIRTGPU_MAP
        0x42 => Some((64, 3)),  // DRM_IOCTL_VIRTGPU_EXECBUFFER
        0x43 => Some((16, 3)),  // DRM_IOCTL_VIRTGPU_GETPARAM
        0x44 => Some((56, 3)),  // DRM_IOCTL_VIRTGPU_RESOURCE_CREATE
        0x45 => Some((16, 3)),  // DRM_IOCTL_VIRTGPU_RESOURCE_INFO
        0x46 => Some((44, 3)),  // DRM_IOCTL_VIRTGPU_TRANSFER_FROM_HOST
        0x47 => Some((44, 3)),  // DRM_IOCTL_VIRTGPU_TRANSFER_TO_HOST
        0x48 => Some((8, 3)),   // DRM_IOCTL_VIRTGPU_WAIT
        0x49 => Some((24, 3)),  // DRM_IOCTL_VIRTGPU_GET_CAPS
        0x4a => Some((56, 3)),  // DRM_IOCTL_VIRTGPU_RESOURCE_CREATE_BLOB
        0x4b => Some((16, 3)),  // DRM_IOCTL_VIRTGPU_CONTEXT_INIT
        _ => None,
    }
}
