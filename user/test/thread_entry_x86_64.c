/*
 *       user/test/thread_entry_x86_64.c
 *       CLONE_THREAD child entry with a fresh stack
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "include/test_syscall.h"

volatile i64 thread_tid;
volatile i64 thread_ran;

void thread_entry(void) {
    thread_tid = sys_gettid();
    thread_ran = 1;
    sys_exit(5);
}
