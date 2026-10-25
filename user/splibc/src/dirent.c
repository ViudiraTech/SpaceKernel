/*
 *       user/splibc/src/dirent.c
 *       getdents64-backed directory iteration
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

DIR *opendir(const char *path) {
    int fd = open(path, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) {
        return NULL;
    }
    DIR *dir = malloc(sizeof(DIR));
    if (dir == NULL) {
        close(fd);
        errno = ENOMEM;
        return NULL;
    }
    dir->fd = fd;
    dir->index = 0;
    dir->size = 0;
    return dir;
}

struct dirent *readdir(DIR *dir) {
    for (;;) {
        if (dir->index >= dir->size) {
            long count = __sc3(SYS_GETDENTS64, dir->fd, (long)dir->buffer,
                               (long)sizeof(dir->buffer));
            if (count <= 0) {
                return NULL;
            }
            dir->index = 0;
            dir->size = (int)count;
        }
        struct linux_dirent64 *entry =
            (struct linux_dirent64 *)(dir->buffer + dir->index);
        if (entry->d_reclen == 0) {
            return NULL;
        }
        dir->index += entry->d_reclen;
        dir->entry.d_ino = entry->d_ino;
        dir->entry.d_off = entry->d_off;
        dir->entry.d_reclen = entry->d_reclen;
        dir->entry.d_type = entry->d_type;
        size_t length = entry->d_reclen > 19 ? (size_t)(entry->d_reclen - 19) : 0;
        if (length > 255) {
            length = 255;
        }
        size_t name_length = 0;
        while (name_length < length && entry->d_name[name_length] != 0) {
            name_length++;
        }
        memcpy(dir->entry.d_name, entry->d_name, name_length);
        dir->entry.d_name[name_length] = 0;
        return &dir->entry;
    }
}

int closedir(DIR *dir) {
    if (dir == NULL) {
        return -1;
    }
    int result = close(dir->fd);
    free(dir);
    return result;
}
