/*
 *       user/splibc/src/socket.c
 *       AF_UNIX socket wrappers
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

int socket(int domain, int type, int protocol) {
    return __sc3(SYS_SOCKET, domain, type, protocol);
}

int socketpair(int domain, int type, int protocol, int fds[2]) {
    return __sc4(SYS_SOCKETPAIR, domain, type, protocol, (long)fds);
}

int bind(int fd, const struct sockaddr_un *addr, socklen_t len) {
    return __sc3(SYS_BIND, fd, (long)addr, len);
}

int listen(int fd, int backlog) {
    return __sc2(SYS_LISTEN, fd, backlog);
}

int connect(int fd, const struct sockaddr_un *addr, socklen_t len) {
    return __sc3(SYS_CONNECT, fd, (long)addr, len);
}

int accept(int fd, struct sockaddr_un *addr, socklen_t *len) {
    return __sc3(SYS_ACCEPT, fd, (long)addr, (long)len);
}

int accept4(int fd, struct sockaddr_un *addr, socklen_t *len, int flags) {
    return __sc4(SYS_ACCEPT4, fd, (long)addr, (long)len, flags);
}

int getsockname(int fd, struct sockaddr_un *addr, socklen_t *len) {
    return __sc3(SYS_GETSOCKNAME, fd, (long)addr, (long)len);
}

int getpeername(int fd, struct sockaddr_un *addr, socklen_t *len) {
    return __sc3(SYS_GETPEERNAME, fd, (long)addr, (long)len);
}

int sendto(int fd, const void *buf, size_t len, int flags,
           const struct sockaddr_un *addr, socklen_t addrlen) {
    return __sc6(SYS_SENDTO, fd, (long)buf, (long)len, flags, (long)addr, addrlen);
}

int recvfrom(int fd, void *buf, size_t len, int flags,
             struct sockaddr_un *addr, socklen_t *addrlen) {
    return __sc6(SYS_RECVFROM, fd, (long)buf, (long)len, flags, (long)addr,
                 (long)addrlen);
}

int send(int fd, const void *buf, size_t len, int flags) {
    return sendto(fd, buf, len, flags, NULL, 0);
}

int recv(int fd, void *buf, size_t len, int flags) {
    return recvfrom(fd, buf, len, flags, NULL, NULL);
}

int sendmsg(int fd, const struct msghdr *msg, int flags) {
    return __sc3(SYS_SENDMSG, fd, (long)msg, flags);
}

int recvmsg(int fd, struct msghdr *msg, int flags) {
    return __sc3(SYS_RECVMSG, fd, (long)msg, flags);
}

int shutdown(int fd, int how) {
    return __sc2(SYS_SHUTDOWN, fd, how);
}

int setsockopt(int fd, int level, int name, const void *value, socklen_t len) {
    return __sc5(SYS_SETSOCKOPT, fd, level, name, (long)value, len);
}

int getsockopt(int fd, int level, int name, void *value, socklen_t *len) {
    return __sc5(SYS_GETSOCKOPT, fd, level, name, (long)value, (long)len);
}
