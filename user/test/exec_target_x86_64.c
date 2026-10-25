/*
 *       user/test/exec_target_x86_64.c
 *       Execve replacement image: proves PID continuity and signal reset
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "include/test_syscall.h"

int main(void) {
    const char message[] = "exec target ok\n";
    sys_write(1, message, sizeof(message) - 1);
    for (;;) {
        __asm__ volatile("pause");
    }
}
