/*
 *       user/test/include/test_syscall.h
 *       Minimal freestanding Linux x86_64 syscall surface for kernel tests
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#ifndef SPACEOS_TEST_SYSCALL_H
#define SPACEOS_TEST_SYSCALL_H

typedef unsigned long u64;
typedef long i64;
typedef unsigned int u32;
typedef int i32;
typedef unsigned short u16;

#define SYS_READ 0
#define SYS_WRITE 1
#define SYS_CLOSE 3
#define SYS_OPENAT 257
#define AT_FDCWD (-100)
#define O_RDWR 2
#define SYS_READ 0
#define SYS_RT_SIGACTION 13
#define SYS_RT_SIGPROCMASK 14
#define SYS_RT_SIGRETURN 15
#define SYS_IOCTL 16
#define SYS_SCHED_YIELD 24
#define SYS_GETPID 39
#define SYS_CLONE 56
#define SYS_FORK 57
#define SYS_EXECVE 59
#define SYS_EXIT 60
#define SYS_WAIT4 61
#define SYS_KILL 62
#define SYS_SETPGID 109
#define SYS_GETPGID 121
#define SYS_GETSID 124
#define SYS_GETTID 186
#define SYS_RT_SIGPENDING 127
#define SYS_EXIT_GROUP 231
#define SYS_FUTEX 202

#define SIGUSR1 10
#define SIGUSR2 12
#define SIGTERM 15
#define SIGSTOP 19
#define SIGCONT 18
#define SIGTTIN 21
#define SIGQUIT 3

#define SIG_BLOCK 0
#define SIG_UNBLOCK 1
#define SIG_SETMASK 2

#define FUTEX_WAIT 0
#define FUTEX_WAKE 1
#define FUTEX_WAIT_BITSET 9
#define FUTEX_WAKE_BITSET 10
#define FUTEX_PRIVATE_FLAG 128

#define ETIMEDOUT 110

#define TCGETS 0x5401
#define TCSETS 0x5402
#define ICANON 0x0002
#define ECHO 0x0008
#define ICRNL 0x0100

#define TIOCGPGRP 0x540F
#define TIOCSPGRP 0x5410
#define TIOCGSID 0x5429

#define WUNTRACED 2
#define WCONTINUED 8

#define SA_SIGINFO 4
#define SA_RESTORER 0x04000000

#define CLONE_VM 0x00000100
#define CLONE_FS 0x00000200
#define CLONE_FILES 0x00000400
#define CLONE_SIGHAND 0x00000800
#define CLONE_THREAD 0x00010000
#define CLONE_CHILD_CLEARTID 0x00200000
#define CLONE_CHILD_SETTID 0x01000000

static inline i64 syscall6(i64 n, i64 a, i64 b, i64 c, i64 d, i64 e, i64 f) {
    register i64 r10 __asm__("r10") = d;
    register i64 r8 __asm__("r8") = e;
    register i64 r9 __asm__("r9") = f;
    i64 ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return ret;
}

static inline i64 syscall3(i64 n, i64 a, i64 b, i64 c) {
    return syscall6(n, a, b, c, 0, 0, 0);
}

static inline i64 sys_write(i64 fd, const void *buf, u64 len) {
    return syscall3(SYS_WRITE, fd, (i64)buf, (i64)len);
}
static inline i64 sys_getpid(void) { return syscall3(SYS_GETPID, 0, 0, 0); }
static inline i64 sys_gettid(void) { return syscall3(SYS_GETTID, 0, 0, 0); }
static inline i64 sys_fork(void) { return syscall3(SYS_FORK, 0, 0, 0); }
static inline i64 sys_execve(const char *path, char *const argv[], char *const envp[]) {
    return syscall3(SYS_EXECVE, (i64)path, (i64)argv, (i64)envp);
}
__attribute__((noreturn)) static inline void sys_exit(i64 code) {
    syscall3(SYS_EXIT, code, 0, 0);
    for (;;) {
    }
}
__attribute__((noreturn)) static inline void sys_exit_group(i64 code) {
    syscall3(SYS_EXIT_GROUP, code, 0, 0);
    for (;;) {
    }
}
static inline i64 sys_kill(i64 pid, i64 sig) {
    return syscall3(SYS_KILL, pid, sig, 0);
}
static inline i64 sys_wait4(i64 pid, i64 *status, i64 options, i64 rusage) {
    return syscall6(SYS_WAIT4, pid, (i64)status, options, rusage, 0, 0);
}
static inline i64 sys_setpgid(i64 pid, i64 pgid) {
    return syscall3(SYS_SETPGID, pid, pgid, 0);
}
static inline i64 sys_getpgid(i64 pid) { return syscall3(SYS_GETPGID, pid, 0, 0); }
static inline i64 sys_getsid(i64 pid) { return syscall3(SYS_GETSID, pid, 0, 0); }
static inline i64 sys_sched_yield(void) { return syscall3(SYS_SCHED_YIELD, 0, 0, 0); }
static inline i64 sys_openat(i64 dirfd, const char *path, u64 flags, u64 mode) {
    return syscall6(SYS_OPENAT, dirfd, (i64)path, (i64)flags, (i64)mode, 0, 0);
}
static inline i64 sys_close(i64 fd) { return syscall3(SYS_CLOSE, fd, 0, 0); }
static inline i64 sys_read(i64 fd, void *buffer, u64 length) {
    return syscall3(SYS_READ, fd, (i64)buffer, (i64)length);
}
static inline i64 sys_ioctl(i64 fd, u64 command, u64 argument) {
    return syscall3(SYS_IOCTL, fd, (i64)command, (i64)argument);
}
static inline i64 sys_rt_sigaction(i64 sig, const void *act, void *old, u64 size) {
    return syscall6(SYS_RT_SIGACTION, sig, (i64)act, (i64)old, size, 0, 0);
}
static inline i64 sys_rt_sigprocmask(i64 how, const u64 *set, u64 *old, u64 size) {
    return syscall6(SYS_RT_SIGPROCMASK, how, (i64)set, (i64)old, size, 0, 0);
}
static inline i64 sys_rt_sigpending(u64 *set, u64 size) {
    return syscall3(SYS_RT_SIGPENDING, (i64)set, size, 0);
}
static inline i64 sys_futex(u64 address, i64 op, u32 value, u64 timeout, u64 address2, u32 value3) {
    return syscall6(SYS_FUTEX, (i64)address, op, value, (i64)timeout, (i64)address2, value3);
}

extern i64 clone_thread(u64 flags, void *stack, volatile i32 *child_tid);
extern void signal_restorer(void);

#endif
