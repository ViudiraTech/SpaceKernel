/*
 *
 *       user/test/unix_stream_large_write.c
 *       Unix stream partial-write and backpressure regression test
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

#include <errno.h>
#include <stdio.h>
#include <sys/socket.h>
#include <unistd.h>

static unsigned char source[256 * 1024];
static unsigned char sink[256 * 1024];

int main(void) {
    int fd[2];
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, fd)) return 1;
    for (size_t i = 0; i < sizeof(source); ++i) source[i] = (unsigned char)(i * 37);
    ssize_t first = write(fd[0], source, sizeof(source));
    if (first <= 0) return 2; /* Previously blocked or returned EAGAIN forever. */
    size_t sent = (size_t)first, received = 0;
    int backpressure = 0;
    while (received < sizeof(sink)) {
        if (sent < sizeof(source)) {
            ssize_t n = write(fd[0], source + sent, sizeof(source) - sent);
            if (n > 0) sent += (size_t)n;
            else if (n < 0 && errno == EAGAIN) backpressure = 1;
            else return 3;
        }
        ssize_t n = read(fd[1], sink + received, sizeof(sink) - received);
        if (n > 0) received += (size_t)n;
        else return 4;
    }
    for (size_t i = 0; i < sizeof(source); ++i) if (sink[i] != source[i]) return 5;
    close(fd[0]); close(fd[1]);
    printf("UNIX_LARGE_WRITE_OK first=%zd bytes=%zu backpressure=%d\n", first, received, backpressure);
    return 0;
}
