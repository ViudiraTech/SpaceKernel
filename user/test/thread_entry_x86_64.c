/*
 *       user/test/thread_entry_x86_64.c
 *       CLONE_THREAD child entry with a fresh stack
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "include/test_syscall.h"

extern volatile i32 futex_gate;
extern volatile i32 thread_loop;
extern volatile i32 thread_waiting;
extern volatile i32 thread_done;

volatile i64 thread_tid;
volatile i64 thread_ran;

void thread_entry(void) {
    thread_tid = sys_gettid();
    thread_ran = 1;
    if (thread_loop) {
        for (;;) {
            __asm__ volatile("pause");
        }
    }
    thread_waiting = 1;
    while (futex_gate == 0) {
        sys_futex((u64)&futex_gate, FUTEX_WAIT | FUTEX_PRIVATE_FLAG, 0, 0, 0, 0);
    }
    thread_done = 1;
    sys_exit(5);
}
