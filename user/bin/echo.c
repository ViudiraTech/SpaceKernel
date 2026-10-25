/*
 *       user/bin/echo.c
 *       Echo arguments
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "splibc.h"

int main(int argc, char **argv, char **envp) {
    (void)envp;
    int newline = 1;
    int index = 1;
    if (index < argc && strcmp(argv[index], "-n") == 0) {
        newline = 0;
        index++;
    }
    for (; index < argc; index++) {
        if (index > (newline ? 1 : 2)) {
            putchar(' ');
        }
        fputs(argv[index], 1);
    }
    if (newline) {
        putchar('\n');
    }
    return 0;
}
