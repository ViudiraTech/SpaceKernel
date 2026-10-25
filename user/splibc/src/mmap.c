/*
 *       user/splibc/src/mmap.c
 *       Anonymous and file mappings
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset) {
    long result = __sc6(SYS_MMAP, (long)addr, (long)length, prot, flags, fd, offset);
    if (result < 0) {
        return MAP_FAILED;
    }
    return (void *)result;
}

int munmap(void *addr, size_t length) {
    return __sc2(SYS_MUNMAP, (long)addr, (long)length);
}

int mprotect(void *addr, size_t length, int prot) {
    return __sc3(SYS_MPROTECT, (long)addr, (long)length, prot);
}
