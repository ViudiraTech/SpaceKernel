/*
 *       user/splibc/src/time.c
 *       Monotonic clocks and sleeping
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

int clock_gettime(int clockid, struct timespec *ts) {
    return __sc2(SYS_CLOCK_GETTIME, clockid, (long)ts);
}

int nanosleep(const struct timespec *request, struct timespec *remaining) {
    return __sc2(SYS_NANOSLEEP, (long)request, (long)remaining);
}

unsigned int sleep(unsigned int seconds) {
    struct timespec request;
    request.tv_sec = seconds;
    request.tv_nsec = 0;
    if (nanosleep(&request, NULL) < 0) {
        return seconds;
    }
    return 0;
}

int usleep(unsigned int microseconds) {
    struct timespec request;
    request.tv_sec = microseconds / 1000000;
    request.tv_nsec = (i64)(microseconds % 1000000) * 1000;
    return nanosleep(&request, NULL);
}
