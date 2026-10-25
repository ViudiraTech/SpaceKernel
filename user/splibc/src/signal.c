/*
 *       user/splibc/src/signal.c
 *       POSIX signal wrappers with the libc restorer
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

int sigaction(int sig, const struct sigaction *act, struct sigaction *old) {
    struct sigaction prepared;
    const struct sigaction *pointer = act;
    if (act != NULL) {
        prepared = *act;
        prepared.sa_flags |= SA_RESTORER;
        prepared.sa_restorer = splibc_sigreturn;
        pointer = &prepared;
    }
    return __sc4(SYS_RT_SIGACTION, sig, (long)pointer, (long)old, 8);
}

sighandler_t signal(int sig, sighandler_t handler) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = handler;
    action.sa_flags = SA_RESTART;
    struct sigaction old;
    if (sigaction(sig, &action, &old) < 0) {
        return SIG_ERR;
    }
    return old.sa_handler;
}

int sigprocmask(int how, const sigset_t *set, sigset_t *old) {
    return __sc4(SYS_RT_SIGPROCMASK, how, (long)set, (long)old, 8);
}

int sigpending(sigset_t *set) {
    return __sc2(SYS_RT_SIGPENDING, (long)set, 8);
}

int sigsuspend(const sigset_t *mask) {
    return __sc2(SYS_RT_SIGSUSPEND, (long)mask, 8);
}

int sigaltstack(const stack_t *ss, stack_t *old) {
    return __sc2(SYS_SIGALTSTACK, (long)ss, (long)old);
}

int pause(void) {
    return __sc0(SYS_PAUSE);
}
