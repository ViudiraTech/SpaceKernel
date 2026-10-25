/*
 *
 *       src/syscall/numbers.rs
 *       Architecture-specific Linux system call numbers
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Linux 64-bit ABI system call numbers for `x86_64` and generic (`aarch64` / `riscv64`).

#[cfg(target_arch = "x86_64")]
pub const SYS_READ: u64 = 0;
#[cfg(target_arch = "x86_64")]
pub const SYS_WRITE: u64 = 1;
#[cfg(target_arch = "x86_64")]
pub const SYS_OPEN: u64 = 2;
#[cfg(target_arch = "x86_64")]
pub const SYS_CLOSE: u64 = 3;
#[cfg(target_arch = "x86_64")]
pub const SYS_STAT: u64 = 4;
#[cfg(target_arch = "x86_64")]
pub const SYS_FSTAT: u64 = 5;
#[cfg(target_arch = "x86_64")]
pub const SYS_LSTAT: u64 = 6;
#[cfg(target_arch = "x86_64")]
pub const SYS_LSEEK: u64 = 8;
#[cfg(target_arch = "x86_64")]
pub const SYS_MMAP: u64 = 9;
#[cfg(target_arch = "x86_64")]
pub const SYS_MUNMAP: u64 = 11;
#[cfg(target_arch = "x86_64")]
pub const SYS_BRK: u64 = 12;
#[cfg(target_arch = "x86_64")]
pub const SYS_IOCTL: u64 = 16;
#[cfg(target_arch = "x86_64")]
pub const SYS_PREAD64: u64 = 17;
#[cfg(target_arch = "x86_64")]
pub const SYS_PWRITE64: u64 = 18;
#[cfg(target_arch = "x86_64")]
pub const SYS_READV: u64 = 19;
#[cfg(target_arch = "x86_64")]
pub const SYS_WRITEV: u64 = 20;
#[cfg(target_arch = "x86_64")]
pub const SYS_ACCESS: u64 = 21;
#[cfg(target_arch = "x86_64")]
pub const SYS_PIPE: u64 = 22;
#[cfg(target_arch = "x86_64")]
pub const SYS_SCHED_YIELD: u64 = 24;
#[cfg(target_arch = "x86_64")]
pub const SYS_DUP: u64 = 32;
#[cfg(target_arch = "x86_64")]
pub const SYS_DUP2: u64 = 33;
#[cfg(target_arch = "x86_64")]
pub const SYS_NANOSLEEP: u64 = 35;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETPID: u64 = 39;
#[cfg(target_arch = "x86_64")]
pub const SYS_CLONE: u64 = 56;
#[cfg(target_arch = "x86_64")]
pub const SYS_FORK: u64 = 57;
#[cfg(target_arch = "x86_64")]
pub const SYS_WAIT4: u64 = 61;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETPPID: u64 = 110;
#[cfg(target_arch = "x86_64")]
pub const SYS_EXIT: u64 = 60;
#[cfg(target_arch = "x86_64")]
pub const SYS_FCNTL: u64 = 72;
#[cfg(target_arch = "x86_64")]
pub const SYS_FSYNC: u64 = 74;
#[cfg(target_arch = "x86_64")]
pub const SYS_FTRUNCATE: u64 = 77;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETCWD: u64 = 79;
#[cfg(target_arch = "x86_64")]
pub const SYS_CHDIR: u64 = 80;
#[cfg(target_arch = "x86_64")]
pub const SYS_MKDIR: u64 = 83;
#[cfg(target_arch = "x86_64")]
pub const SYS_RMDIR: u64 = 84;
#[cfg(target_arch = "x86_64")]
pub const SYS_CREAT: u64 = 85;
#[cfg(target_arch = "x86_64")]
pub const SYS_UNLINK: u64 = 87;
#[cfg(target_arch = "x86_64")]
pub const SYS_SYMLINK: u64 = 88;
#[cfg(target_arch = "x86_64")]
pub const SYS_READLINK: u64 = 89;
#[cfg(target_arch = "x86_64")]
pub const SYS_STATFS: u64 = 137;
#[cfg(target_arch = "x86_64")]
pub const SYS_FSTATFS: u64 = 138;
#[cfg(target_arch = "x86_64")]
pub const SYS_ARCH_PRCTL: u64 = 158;
#[cfg(target_arch = "x86_64")]
pub const SYS_SYNC: u64 = 162;
#[cfg(target_arch = "x86_64")]
pub const SYS_MOUNT: u64 = 165;
#[cfg(target_arch = "x86_64")]
pub const SYS_UMOUNT2: u64 = 166;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETTID: u64 = 186;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETDENTS64: u64 = 217;
#[cfg(target_arch = "x86_64")]
pub const SYS_SET_TID_ADDRESS: u64 = 218;
#[cfg(target_arch = "x86_64")]
pub const SYS_EXIT_GROUP: u64 = 231;
#[cfg(target_arch = "x86_64")]
pub const SYS_OPENAT: u64 = 257;
#[cfg(target_arch = "x86_64")]
pub const SYS_MKDIRAT: u64 = 258;
#[cfg(target_arch = "x86_64")]
pub const SYS_NEWFSTATAT: u64 = 262;
#[cfg(target_arch = "x86_64")]
pub const SYS_UNLINKAT: u64 = 263;
#[cfg(target_arch = "x86_64")]
pub const SYS_SYMLINKAT: u64 = 266;
#[cfg(target_arch = "x86_64")]
pub const SYS_READLINKAT: u64 = 267;
#[cfg(target_arch = "x86_64")]
pub const SYS_FACCESSAT: u64 = 269;
#[cfg(target_arch = "x86_64")]
pub const SYS_DUP3: u64 = 292;
#[cfg(target_arch = "x86_64")]
pub const SYS_PIPE2: u64 = 293;
#[cfg(target_arch = "x86_64")]
pub const SYS_EVENTFD2: u64 = 290;
#[cfg(target_arch = "x86_64")]
pub const SYS_SOCKETPAIR: u64 = 53;
#[cfg(target_arch = "x86_64")]
pub const SYS_SENDMSG: u64 = 46;
#[cfg(target_arch = "x86_64")]
pub const SYS_RECVMSG: u64 = 47;
#[cfg(target_arch = "x86_64")]
pub const SYS_SHUTDOWN: u64 = 48;
#[cfg(target_arch = "x86_64")]
pub const SYS_SOCKET: u64 = 41;
#[cfg(target_arch = "x86_64")]
pub const SYS_CONNECT: u64 = 42;
#[cfg(target_arch = "x86_64")]
pub const SYS_ACCEPT: u64 = 43;
#[cfg(target_arch = "x86_64")]
pub const SYS_SENDTO: u64 = 44;
#[cfg(target_arch = "x86_64")]
pub const SYS_RECVFROM: u64 = 45;
#[cfg(target_arch = "x86_64")]
pub const SYS_BIND: u64 = 49;
#[cfg(target_arch = "x86_64")]
pub const SYS_LISTEN: u64 = 50;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETSOCKNAME: u64 = 51;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETPEERNAME: u64 = 52;
#[cfg(target_arch = "x86_64")]
pub const SYS_SETSOCKOPT: u64 = 54;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETSOCKOPT: u64 = 55;
#[cfg(target_arch = "x86_64")]
pub const SYS_ACCEPT4: u64 = 288;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGACTION: u64 = 13;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGPROCMASK: u64 = 14;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGRETURN: u64 = 15;
#[cfg(target_arch = "x86_64")]
pub const SYS_PAUSE: u64 = 34;
#[cfg(target_arch = "x86_64")]
pub const SYS_KILL: u64 = 62;
#[cfg(target_arch = "x86_64")]
pub const SYS_SETPGID: u64 = 109;
#[cfg(target_arch = "x86_64")]
pub const SYS_SETSID: u64 = 112;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETPGID: u64 = 121;
#[cfg(target_arch = "x86_64")]
pub const SYS_GETSID: u64 = 124;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGPENDING: u64 = 127;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGTIMEDWAIT: u64 = 128;
#[cfg(target_arch = "x86_64")]
pub const SYS_RT_SIGSUSPEND: u64 = 130;
#[cfg(target_arch = "x86_64")]
pub const SYS_SIGALTSTACK: u64 = 131;
#[cfg(target_arch = "x86_64")]
pub const SYS_TKILL: u64 = 200;
#[cfg(target_arch = "x86_64")]
pub const SYS_TGKILL: u64 = 234;
#[cfg(target_arch = "x86_64")]
pub const SYS_WAITID: u64 = 247;
#[cfg(target_arch = "x86_64")]
pub const SYS_FUTEX: u64 = 202;
#[cfg(target_arch = "x86_64")]
pub const SYS_EPOLL_CREATE: u64 = 213;
#[cfg(target_arch = "x86_64")]
pub const SYS_EPOLL_WAIT: u64 = 232;
#[cfg(target_arch = "x86_64")]
pub const SYS_EPOLL_CTL: u64 = 233;
#[cfg(target_arch = "x86_64")]
pub const SYS_EPOLL_PWAIT: u64 = 281;
#[cfg(target_arch = "x86_64")]
pub const SYS_EPOLL_CREATE1: u64 = 291;
#[cfg(target_arch = "x86_64")]
pub const SYS_SIGNALFD: u64 = 282;
#[cfg(target_arch = "x86_64")]
pub const SYS_SIGNALFD4: u64 = 289;
#[cfg(target_arch = "x86_64")]
pub const SYS_TIMERFD_CREATE: u64 = 283;
#[cfg(target_arch = "x86_64")]
pub const SYS_TIMERFD_SETTIME: u64 = 286;
#[cfg(target_arch = "x86_64")]
pub const SYS_TIMERFD_GETTIME: u64 = 287;
#[cfg(target_arch = "x86_64")]
pub const SYS_MEMFD_CREATE: u64 = 319;

// asm-generic/unistd.h numbers for AArch64 and RISC-V 64
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETCWD: u64 = 17;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_DUP: u64 = 23;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_DUP3: u64 = 24;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FCNTL: u64 = 25;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_IOCTL: u64 = 29;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MKDIRAT: u64 = 34;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_UNLINKAT: u64 = 35;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SYMLINKAT: u64 = 36;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_UMOUNT2: u64 = 39;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MOUNT: u64 = 40;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_STATFS: u64 = 43;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FSTATFS: u64 = 44;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FTRUNCATE: u64 = 46;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FACCESSAT: u64 = 48;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CHDIR: u64 = 49;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_OPENAT: u64 = 56;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CLOSE: u64 = 57;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PIPE2: u64 = 59;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EVENTFD2: u64 = 19;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SOCKETPAIR: u64 = 199;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SENDMSG: u64 = 211;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RECVMSG: u64 = 212;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SHUTDOWN: u64 = 210;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SOCKET: u64 = 198;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_BIND: u64 = 200;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_LISTEN: u64 = 201;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_ACCEPT: u64 = 202;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CONNECT: u64 = 203;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETSOCKNAME: u64 = 204;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETPEERNAME: u64 = 205;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SENDTO: u64 = 206;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RECVFROM: u64 = 207;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETSOCKOPT: u64 = 208;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETSOCKOPT: u64 = 209;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_ACCEPT4: u64 = 242;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_KILL: u64 = 129;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TKILL: u64 = 130;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TGKILL: u64 = 131;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SIGALTSTACK: u64 = 132;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGSUSPEND: u64 = 133;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGACTION: u64 = 134;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGPROCMASK: u64 = 135;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGPENDING: u64 = 136;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGTIMEDWAIT: u64 = 137;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_RT_SIGRETURN: u64 = 139;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETPGID: u64 = 154;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETPGID: u64 = 155;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETSID: u64 = 156;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETSID: u64 = 157;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_WAITID: u64 = 95;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FUTEX: u64 = 98;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EPOLL_CREATE1: u64 = 20;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EPOLL_CTL: u64 = 21;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EPOLL_PWAIT: u64 = 22;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SIGNALFD4: u64 = 74;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TIMERFD_CREATE: u64 = 85;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TIMERFD_SETTIME: u64 = 86;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TIMERFD_GETTIME: u64 = 87;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MEMFD_CREATE: u64 = 279;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETDENTS64: u64 = 61;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_LSEEK: u64 = 62;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_READ: u64 = 63;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_WRITE: u64 = 64;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_READV: u64 = 65;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_WRITEV: u64 = 66;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PREAD64: u64 = 67;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PWRITE64: u64 = 68;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_READLINKAT: u64 = 78;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_NEWFSTATAT: u64 = 79;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FSTAT: u64 = 80;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SYNC: u64 = 81;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FSYNC: u64 = 82;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EXIT: u64 = 93;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EXIT_GROUP: u64 = 94;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SET_TID_ADDRESS: u64 = 96;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_NANOSLEEP: u64 = 101;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SCHED_YIELD: u64 = 124;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETPID: u64 = 172;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CLONE: u64 = 220;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_WAIT4: u64 = 260;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETPPID: u64 = 173;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETTID: u64 = 178;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_BRK: u64 = 214;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MUNMAP: u64 = 215;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MMAP: u64 = 222;

#[cfg(target_arch = "x86_64")]
pub const SYS_MPROTECT: u64 = 10;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MPROTECT: u64 = 226;

#[cfg(target_arch = "x86_64")]
pub const SYS_EXECVE: u64 = 59;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_EXECVE: u64 = 221;

#[cfg(target_arch = "x86_64")]
pub const SYS_PPOLL: u64 = 271;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PPOLL: u64 = 73;

#[cfg(target_arch = "x86_64")]
pub const SYS_CLOCK_GETTIME: u64 = 228;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CLOCK_GETTIME: u64 = 113;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETRANDOM: u64 = 318;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETRANDOM: u64 = 278;

#[cfg(target_arch = "x86_64")]
pub const SYS_POLL: u64 = 7;

#[cfg(target_arch = "x86_64")]
pub const SYS_MKNODAT: u64 = 259;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MKNODAT: u64 = 33;

#[cfg(target_arch = "x86_64")]
pub const SYS_FCHMOD: u64 = 91;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FCHMOD: u64 = 52;

#[cfg(target_arch = "x86_64")]
pub const SYS_FCHMODAT: u64 = 268;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FCHMODAT: u64 = 53;

#[cfg(target_arch = "x86_64")]
pub const SYS_SENDFILE: u64 = 40;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SENDFILE: u64 = 71;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETUID: u64 = 102;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETUID: u64 = 174;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETGID: u64 = 104;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETGID: u64 = 176;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETEUID: u64 = 107;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETEUID: u64 = 175;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETEGID: u64 = 108;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETEGID: u64 = 177;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETUID: u64 = 105;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETUID: u64 = 146;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETGID: u64 = 106;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETGID: u64 = 144;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETREUID: u64 = 113;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETREUID: u64 = 145;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETREGID: u64 = 114;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETREGID: u64 = 143;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETGROUPS: u64 = 115;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETGROUPS: u64 = 158;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETGROUPS: u64 = 116;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETGROUPS: u64 = 159;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETRESUID: u64 = 117;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETRESUID: u64 = 147;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETRESUID: u64 = 118;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETRESUID: u64 = 148;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETRESGID: u64 = 119;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETRESGID: u64 = 149;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETRESGID: u64 = 120;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETRESGID: u64 = 150;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETFSUID: u64 = 122;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETFSUID: u64 = 151;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETFSGID: u64 = 123;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETFSGID: u64 = 152;

#[cfg(target_arch = "x86_64")]
pub const SYS_UNAME: u64 = 63;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_UNAME: u64 = 160;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETHOSTNAME: u64 = 170;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETHOSTNAME: u64 = 161;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETDOMAINNAME: u64 = 171;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETDOMAINNAME: u64 = 162;

#[cfg(target_arch = "x86_64")]
pub const SYS_UMASK: u64 = 95;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_UMASK: u64 = 166;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETRLIMIT: u64 = 97;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETRLIMIT: u64 = 163;

#[cfg(target_arch = "x86_64")]
pub const SYS_SETRLIMIT: u64 = 160;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SETRLIMIT: u64 = 164;

#[cfg(target_arch = "x86_64")]
pub const SYS_PRLIMIT64: u64 = 302;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PRLIMIT64: u64 = 261;

#[cfg(target_arch = "x86_64")]
pub const SYS_SCHED_GETAFFINITY: u64 = 204;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SCHED_GETAFFINITY: u64 = 123;

#[cfg(target_arch = "x86_64")]
pub const SYS_SCHED_SETAFFINITY: u64 = 203;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SCHED_SETAFFINITY: u64 = 122;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETTIMEOFDAY: u64 = 96;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETTIMEOFDAY: u64 = 169;

#[cfg(target_arch = "x86_64")]
pub const SYS_CLOCK_GETRES: u64 = 229;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CLOCK_GETRES: u64 = 114;

#[cfg(target_arch = "x86_64")]
pub const SYS_CLOCK_NANOSLEEP: u64 = 230;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_CLOCK_NANOSLEEP: u64 = 115;

#[cfg(target_arch = "x86_64")]
pub const SYS_GETRUSAGE: u64 = 98;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GETRUSAGE: u64 = 165;

#[cfg(target_arch = "x86_64")]
pub const SYS_SYSINFO: u64 = 99;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SYSINFO: u64 = 179;

#[cfg(target_arch = "x86_64")]
pub const SYS_TIMES: u64 = 100;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_TIMES: u64 = 153;

#[cfg(target_arch = "x86_64")]
pub const SYS_MADVISE: u64 = 28;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_MADVISE: u64 = 233;

#[cfg(target_arch = "x86_64")]
pub const SYS_STATX: u64 = 332;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_STATX: u64 = 291;

#[cfg(target_arch = "x86_64")]
pub const SYS_PRCTL: u64 = 157;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_PRCTL: u64 = 167;

#[cfg(target_arch = "x86_64")]
pub const SYS_SET_ROBUST_LIST: u64 = 273;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_SET_ROBUST_LIST: u64 = 99;

#[cfg(target_arch = "x86_64")]
pub const SYS_GET_ROBUST_LIST: u64 = 274;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_GET_ROBUST_LIST: u64 = 100;

#[cfg(target_arch = "x86_64")]
pub const SYS_FCHOWN: u64 = 93;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FCHOWN: u64 = 55;

#[cfg(target_arch = "x86_64")]
pub const SYS_FCHOWNAT: u64 = 260;
#[cfg(not(target_arch = "x86_64"))]
pub const SYS_FCHOWNAT: u64 = 54;
