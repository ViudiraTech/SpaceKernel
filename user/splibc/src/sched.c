/*
 *       user/splibc/src/sched.c
 *       Scheduler yield
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

int sched_yield(void) {
    return __sc0(SYS_SCHED_YIELD);
}
