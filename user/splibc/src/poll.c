/*
 *       user/splibc/src/poll.c
 *       poll, epoll, eventfd, signalfd, timerfd and memfd wrappers
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

int poll(struct pollfd *fds, unsigned long nfds, int timeout) {
    return __sc3(SYS_POLL, (long)fds, (long)nfds, timeout);
}

int epoll_create1(int flags) {
    return __sc1(SYS_EPOLL_CREATE1, flags);
}

int epoll_ctl(int epfd, int op, int fd, struct epoll_event *event) {
    return __sc4(SYS_EPOLL_CTL, epfd, op, fd, (long)event);
}

int epoll_wait(int epfd, struct epoll_event *events, int maxevents, int timeout) {
    return __sc4(SYS_EPOLL_WAIT, epfd, (long)events, maxevents, timeout);
}

int eventfd(unsigned int initval, int flags) {
    return __sc2(SYS_EVENTFD2, initval, flags);
}

int signalfd(int fd, const sigset_t *mask, size_t size) {
    return __sc4(SYS_SIGNALFD4, fd, (long)mask, (long)size, 0);
}

int timerfd_create(int clockid, int flags) {
    return __sc2(SYS_TIMERFD_CREATE, clockid, flags);
}

int timerfd_settime(int fd, int flags, const struct itimerspec *new_value,
                    struct itimerspec *old_value) {
    return __sc4(SYS_TIMERFD_SETTIME, fd, flags, (long)new_value, (long)old_value);
}

int timerfd_gettime(int fd, struct itimerspec *old_value) {
    return __sc2(SYS_TIMERFD_GETTIME, fd, (long)old_value);
}

int memfd_create(const char *name, unsigned int flags) {
    return __sc2(SYS_MEMFD_CREATE, (long)name, flags);
}
