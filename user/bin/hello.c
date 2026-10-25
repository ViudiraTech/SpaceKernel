/*
 *       user/bin/hello.c
 *       Hello world for the SpaceOS-9 userland
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "splibc.h"

int main(int argc, char **argv, char **envp) {
    (void)argc;
    (void)argv;
    (void)envp;
    printf("hello from splibc (pid %d)\n", getpid());
    return 0;
}
