/*
 *       user/bin/cat.c
 *       Concatenate standard input or files
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "splibc.h"

static int copy_fd(int fd) {
    char buffer[4096];
    for (;;) {
        ssize_t count = read(fd, buffer, sizeof(buffer));
        if (count < 0) {
            return 1;
        }
        if (count == 0) {
            return 0;
        }
        ssize_t done = 0;
        while (done < count) {
            ssize_t written = write(1, buffer + done, (size_t)(count - done));
            if (written <= 0) {
                return 1;
            }
            done += written;
        }
    }
}

int main(int argc, char **argv, char **envp) {
    (void)envp;
    if (argc < 2) {
        return copy_fd(0);
    }
    int result = 0;
    for (int index = 1; index < argc; index++) {
        int fd = open(argv[index], O_RDONLY);
        if (fd < 0) {
            fprintf(2, "cat: %s: %s\n", argv[index], strerror(errno));
            result = 1;
            continue;
        }
        if (copy_fd(fd) != 0) {
            result = 1;
        }
        close(fd);
    }
    return result;
}
