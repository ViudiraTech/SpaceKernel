/*
 *       user/test/process_signal_x86_64.c
 *       Threads, signals, process groups and cross-process kill
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "include/test_syscall.h"

extern volatile i64 thread_tid;
extern volatile i64 thread_ran;
void thread_entry(void);

static volatile i64 handler_sig;
static volatile i64 handler_count;
static char thread_stack[65536] __attribute__((aligned(16)));

static u64 string_length(const char *text) {
    u64 length = 0;
    while (text[length] != 0) {
        length++;
    }
    return length;
}

static void fail(const char *why) {
    sys_write(1, why, string_length(why));
    sys_exit_group(1);
}

static void handler(int sig) {
    handler_sig = sig;
    handler_count++;
}

static void terminate_handler(int sig) {
    (void)sig;
    sys_exit(7);
}

struct kernel_sigaction {
    void *handler;
    u64 flags;
    void *restorer;
    u64 mask;
};

static void install_handler(int sig, void (*fn)(int)) {
    struct kernel_sigaction action;
    action.handler = (void *)fn;
    action.flags = SA_SIGINFO | SA_RESTORER;
    action.restorer = (void *)signal_restorer;
    action.mask = 0;
    if (sys_rt_sigaction(sig, &action, 0, 8) != 0) {
        fail("E:action\n");
    }
}

static void wait_frames(int count) {
    for (int i = 0; i < count; i++) {
        sys_sched_yield();
    }
}

int main(void) {
    i64 me = sys_getpid();
    if (me <= 0 || sys_gettid() != me) {
        fail("E:ident\n");
    }
    if (sys_getpgid(0) != me || sys_getsid(0) != me) {
        fail("E:session\n");
    }

    install_handler(SIGUSR1, handler);
    install_handler(SIGUSR2, handler);

    if (sys_kill(me, SIGUSR1) != 0) {
        fail("E:kill1\n");
    }
    if (handler_sig != SIGUSR1 || handler_count != 1) {
        fail("E:handler1\n");
    }

    u64 blocked = 1ul << (SIGUSR2 - 1);
    if (sys_rt_sigprocmask(SIG_BLOCK, &blocked, 0, 8) != 0) {
        fail("E:block\n");
    }
    if (sys_kill(me, SIGUSR2) != 0) {
        fail("E:kill2\n");
    }
    u64 pending = 0;
    if (sys_rt_sigpending(&pending, 8) != 0) {
        fail("E:pending\n");
    }
    if ((pending & blocked) == 0) {
        fail("E:notpending\n");
    }
    if (handler_sig == SIGUSR2) {
        fail("E:blockedran\n");
    }
    if (sys_rt_sigprocmask(SIG_UNBLOCK, &blocked, 0, 8) != 0) {
        fail("E:unblock\n");
    }
    if (handler_sig != SIGUSR2 || handler_count != 2) {
        fail("E:handler2\n");
    }

    /* A CLONE_THREAD child shares memory; CHILD_SETTID publishes its TID and
     * CHILD_CLEARTID clears the word when the thread exits. */
    volatile i32 child_tid = -1;
    u64 flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD |
                CLONE_CHILD_SETTID | CLONE_CHILD_CLEARTID;
    i64 tid = clone_thread(flags, thread_stack + sizeof(thread_stack), &child_tid);
    if (tid <= 0) {
        fail("E:clone\n");
    }
    for (long i = 0; i < 200000 && thread_ran == 0; i++) {
        sys_sched_yield();
    }
    if (thread_ran == 0) {
        fail("E:nothread\n");
    }
    if (thread_tid <= 0 || thread_tid != tid || thread_tid == me) {
        fail("E:threadtid\n");
    }
    if (child_tid != (i32)tid) {
        fail("E:settid\n");
    }
    for (long i = 0; i < 200000 && child_tid != 0; i++) {
        sys_sched_yield();
    }
    if (child_tid != 0) {
        fail("E:cleartid\n");
    }

    /* Child installs a SIGTERM handler; kill must run it and wait4 must report
     * its exit code. */
    i64 child = sys_fork();
    if (child == 0) {
        install_handler(SIGTERM, terminate_handler);
        if (sys_setpgid(0, 0) != 0) {
            fail("E:childpgid\n");
        }
        if (sys_getpgid(0) != sys_getpid()) {
            fail("E:childpgid2\n");
        }
        for (;;) {
            __asm__ volatile("pause");
        }
    }
    if (child < 0) {
        fail("E:fork\n");
    }
    wait_frames(20000);
    if (sys_kill(child, SIGTERM) != 0) {
        fail("E:killchild\n");
    }
    i64 status = 0;
    i64 reaped = sys_wait4(child, &status, 0, 0);
    if (reaped != child) {
        fail("E:wait1\n");
    }
    if (((status >> 8) & 0xff) != 7) {
        fail("E:status1\n");
    }

    /* A SIGKILL death is reported as a signaled wait status. */
    child = sys_fork();
    if (child == 0) {
        for (;;) {
            __asm__ volatile("pause");
        }
    }
    if (child < 0) {
        fail("E:fork2\n");
    }
    wait_frames(20000);
    if (sys_kill(child, 9) != 0) {
        fail("E:kill2child\n");
    }
    status = 0;
    reaped = sys_wait4(child, &status, 0, 0);
    if (reaped != child) {
        fail("E:wait2\n");
    }
    if ((status & 0x7f) != 9) {
        fail("E:status2\n");
    }

    const char ok[] = "process/signal ok\n";
    sys_write(1, ok, sizeof(ok) - 1);
    sys_exit_group(42);
}
