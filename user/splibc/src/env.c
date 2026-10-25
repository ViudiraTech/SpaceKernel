/*
 *       user/splibc/src/env.c
 *       Environment variable access
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

char **environ;

static size_t environment_count(void) {
    size_t count = 0;
    while (environ != NULL && environ[count] != NULL) {
        count++;
    }
    return count;
}

char *getenv(const char *name) {
    size_t length = strlen(name);
    for (char **entry = environ; entry != NULL && *entry != NULL; entry++) {
        if (strncmp(*entry, name, length) == 0 && (*entry)[length] == '=') {
            return *entry + length + 1;
        }
    }
    return NULL;
}

int setenv(const char *name, const char *value, int overwrite) {
    if (name == NULL || *name == 0 || strchr(name, '=') != NULL) {
        errno = EINVAL;
        return -1;
    }
    size_t name_length = strlen(name);
    size_t value_length = strlen(value);
    for (size_t index = 0; environ != NULL && environ[index] != NULL; index++) {
        if (strncmp(environ[index], name, name_length) == 0
            && environ[index][name_length] == '=') {
            if (!overwrite) {
                return 0;
            }
            char *entry = malloc(name_length + value_length + 2);
            if (entry == NULL) {
                return -1;
            }
            memcpy(entry, name, name_length);
            entry[name_length] = '=';
            memcpy(entry + name_length + 1, value, value_length + 1);
            environ[index] = entry;
            return 0;
        }
    }
    size_t count = environment_count();
    char **replacement = malloc((count + 2) * sizeof(char *));
    if (replacement == NULL) {
        return -1;
    }
    for (size_t index = 0; index < count; index++) {
        replacement[index] = environ[index];
    }
    char *entry = malloc(name_length + value_length + 2);
    if (entry == NULL) {
        free(replacement);
        return -1;
    }
    memcpy(entry, name, name_length);
    entry[name_length] = '=';
    memcpy(entry + name_length + 1, value, value_length + 1);
    replacement[count] = entry;
    replacement[count + 1] = NULL;
    environ = replacement;
    return 0;
}

int unsetenv(const char *name) {
    size_t length = strlen(name);
    size_t write = 0;
    for (size_t read = 0; environ != NULL && environ[read] != NULL; read++) {
        if (strncmp(environ[read], name, length) == 0 && environ[read][length] == '=') {
            continue;
        }
        environ[write++] = environ[read];
    }
    if (environ != NULL) {
        environ[write] = NULL;
    }
    return 0;
}
