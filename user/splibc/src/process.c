/*
 *       user/splibc/src/process.c
 *       Process creation, waiting and job control
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

pid_t fork(void) {
    return __sc0(SYS_FORK);
}

int execve(const char *path, char *const argv[], char *const envp[]) {
    return __sc3(SYS_EXECVE, (long)path, (long)argv, (long)envp);
}

int execvp(const char *file, char *const argv[]) {
    if (strchr(file, '/') != NULL) {
        return execve(file, argv, environ);
    }
    const char *path = getenv("PATH");
    if (path == NULL || *path == 0) {
        path = "/bin:/sbin:/usr/bin:/usr/sbin";
    }
    static char buffer[PATH_MAX];
    int last_error = ENOENT;
    const char *cursor = path;
    while (1) {
        const char *separator = strchr(cursor, ':');
        size_t length = separator ? (size_t)(separator - cursor) : strlen(cursor);
        size_t total = length + 1 + strlen(file) + 1;
        if (total <= sizeof(buffer)) {
            memcpy(buffer, cursor, length);
            buffer[length] = '/';
            strcpy(buffer + length + 1, file);
            execve(buffer, argv, environ);
            if (errno != ENOENT && errno != ENOTDIR) {
                last_error = errno;
            }
        }
        if (separator == NULL) {
            break;
        }
        cursor = separator + 1;
    }
    errno = last_error;
    return -1;
}

void _exit(int status) {
    __sc1(SYS_EXIT, status);
    for (;;) {
    }
}

void exit(int status) {
    __sc1(SYS_EXIT_GROUP, status);
    for (;;) {
    }
}

pid_t getpid(void) {
    return __sc0(SYS_GETPID);
}

pid_t gettid(void) {
    return __sc0(SYS_GETTID);
}

pid_t getppid(void) {
    return __sc0(SYS_GETPPID);
}

pid_t waitpid(pid_t pid, int *status, int options) {
    return __sc4(SYS_WAIT4, pid, (long)status, options, 0);
}

pid_t wait(int *status) {
    return waitpid(-1, status, 0);
}

int waitid(int idtype, int id, struct siginfo *info, int options, void *rusage) {
    return __sc5(SYS_WAITID, idtype, id, (long)info, options, (long)rusage);
}

pid_t setsid(void) {
    return __sc0(SYS_SETSID);
}

int setpgid(pid_t pid, pid_t pgid) {
    return __sc2(SYS_SETPGID, pid, pgid);
}

pid_t getpgid(pid_t pid) {
    return __sc1(SYS_GETPGID, pid);
}

pid_t getpgrp(void) {
    return getpgid(0);
}

pid_t getsid(pid_t pid) {
    return __sc1(SYS_GETSID, pid);
}

pid_t tcgetpgrp(int fd) {
    i32 pgrp = 0;
    if (ioctl(fd, TIOCGPGRP, &pgrp) < 0) {
        return -1;
    }
    return pgrp;
}

int tcsetpgrp(int fd, pid_t pgrp) {
    i32 value = pgrp;
    return ioctl(fd, TIOCSPGRP, &value);
}

int kill(pid_t pid, int sig) {
    return __sc2(SYS_KILL, pid, sig);
}
