/*
 *       user/systemg/systemg.c
 *       Minimal systemd-subset service manager running as PID 1
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "splibc.h"

#define MAX_UNITS 16
#define MAX_DEPS 8

enum unit_type { TYPE_SIMPLE, TYPE_EXEC, TYPE_ONESHOT };
enum restart_policy { RESTART_NO, RESTART_ON_FAILURE, RESTART_ALWAYS };

struct unit {
    char name[64];
    char description[96];
    char exec[256];
    char after[MAX_DEPS][64];
    int after_count;
    char requires[MAX_DEPS][64];
    int requires_count;
    char wants[MAX_DEPS][64];
    int wants_count;
    int type;
    int restart;
    int restart_sec;
    pid_t pid;
    int started;
    int failed;
    int pending_restart;
    int restarts;
    int ever_started;
};

static struct unit units[MAX_UNITS];
static int unit_count;
static volatile int child_changed;

static void child_handler(int signal_number) {
    (void)signal_number;
    child_changed = 1;
}

static const char *unit_ident(const struct unit *unit) {
    return unit->description[0] != 0 ? unit->description : unit->name;
}

static const char *exit_status_name(int status) {
    switch (status) {
    case 0: return "SUCCESS";
    case 1: return "FAILURE";
    case 2: return "INVALIDARGUMENT";
    case 3: return "NOTIMPLEMENTED";
    case 4: return "NOPERMISSION";
    case 5: return "NOTINSTALLED";
    case 6: return "NOTCONFIGURED";
    case 7: return "NOTRUNNING";
    case 203: return "EXEC";
    case 204: return "NOENT";
    case 205: return "NOMEM";
    default: return "UNKNOWN";
    }
}

static const char *signal_name(int signal_number) {
    switch (signal_number) {
    case SIGHUP: return "HUP";
    case SIGINT: return "INT";
    case SIGQUIT: return "QUIT";
    case SIGILL: return "ILL";
    case SIGTRAP: return "TRAP";
    case SIGABRT: return "ABRT";
    case SIGBUS: return "BUS";
    case SIGFPE: return "FPE";
    case SIGKILL: return "KILL";
    case SIGUSR1: return "USR1";
    case SIGSEGV: return "SEGV";
    case SIGUSR2: return "USR2";
    case SIGPIPE: return "PIPE";
    case SIGALRM: return "ALRM";
    case SIGTERM: return "TERM";
    case SIGCHLD: return "CHLD";
    case SIGCONT: return "CONT";
    case SIGSTOP: return "STOP";
    case SIGTSTP: return "TSTP";
    case SIGTTIN: return "TTIN";
    case SIGTTOU: return "TTOU";
    default: return "UNKNOWN";
    }
}

static void log_process_exit(const struct unit *unit, const char *kind, int status) {
    if (WIFEXITED(status)) {
        int code = WEXITSTATUS(status);
        printf("systemg[1]: %s: %s exited, code=exited, status=%d/%s%s\n",
               unit->name, kind, code, exit_status_name(code),
               code == 0 ? " (success)" : "");
    } else if (WIFSIGNALED(status)) {
        int signal_number = WTERMSIG(status);
        printf("systemg[1]: %s: %s exited, code=%s, status=%d/%s\n",
               unit->name, kind,
               WIFSIGNALED(status) && (status & 0x80) != 0 ? "dumped" : "killed",
               signal_number, signal_name(signal_number));
    }
}

static char *trim(char *text) {
    while (*text == ' ' || *text == '\t' || *text == '\r') {
        text++;
    }
    char *end = text + strlen(text);
    while (end > text && (end[-1] == ' ' || end[-1] == '\t' || end[-1] == '\r'
                          || end[-1] == '\n')) {
        *--end = 0;
    }
    return text;
}

static void copy_name(char *destination, size_t size, const char *source) {
    size_t index = 0;
    while (index + 1 < size && source[index] != 0) {
        destination[index] = source[index];
        index++;
    }
    destination[index] = 0;
}

static void add_dependency(char list[MAX_DEPS][64], int *count, const char *value) {
    char buffer[256];
    copy_name(buffer, sizeof(buffer), value);
    char *cursor = buffer;
    while (*cursor != 0) {
        while (*cursor == ' ' || *cursor == '\t') {
            cursor++;
        }
        if (*cursor == 0) {
            break;
        }
        char *end = cursor;
        while (*end != 0 && *end != ' ' && *end != '\t') {
            end++;
        }
        char saved = *end;
        *end = 0;
        if (*count < MAX_DEPS) {
            char *dot = strchr(cursor, '.');
            if (dot != NULL) {
                *dot = 0;
            }
            copy_name(list[*count], 64, cursor);
            *count += 1;
        }
        *end = saved;
        cursor = end;
    }
}

static int load_unit(const char *path, const char *name) {
    if (unit_count >= MAX_UNITS) {
        return -1;
    }
    int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return -1;
    }
    char content[4096];
    ssize_t count = read(fd, content, sizeof(content) - 1);
    close(fd);
    if (count <= 0) {
        return -1;
    }
    content[count] = 0;
    struct unit *unit = &units[unit_count];
    memset(unit, 0, sizeof(*unit));
    copy_name(unit->name, sizeof(unit->name), name);
    unit->restart = RESTART_NO;
    unit->restart_sec = 1;
    int section = 0;
    char *line = content;
    while (line != NULL && *line != 0) {
        char *next = strchr(line, '\n');
        if (next != NULL) {
            *next = 0;
            next++;
        }
        char *text = trim(line);
        if (*text == '[') {
            if (strncmp(text, "[Unit]", 6) == 0) {
                section = 1;
            } else if (strncmp(text, "[Service]", 9) == 0) {
                section = 2;
            } else {
                section = 0;
            }
        } else if (*text != 0 && *text != '#') {
            char *equals = strchr(text, '=');
            if (equals != NULL) {
                *equals = 0;
                char *key = trim(text);
                char *value = trim(equals + 1);
                if (section == 1) {
                    if (strcmp(key, "Description") == 0) {
                        copy_name(unit->description, sizeof(unit->description), value);
                    } else if (strcmp(key, "After") == 0) {
                        add_dependency(unit->after, &unit->after_count, value);
                    } else if (strcmp(key, "Requires") == 0) {
                        add_dependency(unit->requires, &unit->requires_count, value);
                    } else if (strcmp(key, "Wants") == 0) {
                        add_dependency(unit->wants, &unit->wants_count, value);
                    }
                } else if (section == 2) {
                    if (strcmp(key, "ExecStart") == 0) {
                        copy_name(unit->exec, sizeof(unit->exec), value);
                    } else if (strcmp(key, "Type") == 0) {
                        if (strcmp(value, "oneshot") == 0) {
                            unit->type = TYPE_ONESHOT;
                        } else if (strcmp(value, "exec") == 0) {
                            unit->type = TYPE_EXEC;
                        } else {
                            unit->type = TYPE_SIMPLE;
                        }
                    } else if (strcmp(key, "Restart") == 0) {
                        if (strcmp(value, "always") == 0) {
                            unit->restart = RESTART_ALWAYS;
                        } else if (strcmp(value, "on-failure") == 0) {
                            unit->restart = RESTART_ON_FAILURE;
                        } else {
                            unit->restart = RESTART_NO;
                        }
                    } else if (strcmp(key, "RestartSec") == 0) {
                        unit->restart_sec = atoi(value);
                    }
                }
            }
        }
        line = next;
    }
    if (unit->exec[0] == 0) {
        return -1;
    }
    unit_count++;
    return 0;
}

static int split_arguments(char *command, char *arguments[], int maximum) {
    int count = 0;
    char *cursor = command;
    while (*cursor != 0 && count + 1 < maximum) {
        while (*cursor == ' ' || *cursor == '\t') {
            cursor++;
        }
        if (*cursor == 0) {
            break;
        }
        char *start = cursor;
        int quoted = 0;
        char *write = cursor;
        while (*cursor != 0) {
            if (*cursor == '"') {
                quoted = !quoted;
                cursor++;
                continue;
            }
            if (!quoted && (*cursor == ' ' || *cursor == '\t')) {
                break;
            }
            *write++ = *cursor++;
        }
        if (*cursor != 0) {
            cursor++;
        }
        *write = 0;
        arguments[count++] = start;
    }
    arguments[count] = NULL;
    return count;
}

static int unit_index(const char *name) {
    for (int index = 0; index < unit_count; index++) {
        if (strcmp(units[index].name, name) == 0) {
            return index;
        }
    }
    return -1;
}

static int dependencies_ready(const struct unit *unit) {
    for (int index = 0; index < unit->after_count; index++) {
        int other = unit_index(unit->after[index]);
        if (other >= 0 && !units[other].started) {
            return 0;
        }
    }
    for (int index = 0; index < unit->requires_count; index++) {
        int other = unit_index(unit->requires[index]);
        if (other >= 0 && !units[other].started) {
            return 0;
        }
    }
    return 1;
}

static pid_t start_unit(struct unit *unit) {
    if (unit->pid > 0) {
        return unit->pid;
    }
    char command[256];
    copy_name(command, sizeof(command), unit->exec);
    char *arguments[16];
    int count = split_arguments(command, arguments, 16);
    if (count == 0) {
        unit->failed = 1;
        return -1;
    }
    printf("         Starting %s...\n", unit_ident(unit));
    pid_t pid = fork();
    if (pid == 0) {
        setpgid(0, 0);
        signal(SIGINT, SIG_DFL);
        signal(SIGQUIT, SIG_DFL);
        signal(SIGTSTP, SIG_DFL);
        signal(SIGTTIN, SIG_DFL);
        signal(SIGTTOU, SIG_DFL);
        signal(SIGCHLD, SIG_DFL);
        execvp(arguments[0], arguments);
        fprintf(2, "systemg: exec %s failed: %s\n", arguments[0], strerror(errno));
        _exit(127);
    }
    if (pid < 0) {
        printf("systemg: fork failed for %s\n", unit->name);
        unit->failed = 1;
        return -1;
    }
    unit->pid = pid;
    unit->started = 1;
    unit->ever_started = 1;
    unit->failed = 0;
    unit->pending_restart = 0;
    setpgid(pid, pid);
    if (unit->type != TYPE_ONESHOT) {
        printf("[  OK  ] Started %s.\n", unit_ident(unit));
    }
    if (strcmp(unit->name, "console") == 0) {
        tcsetpgrp(0, pid);
    }
    if (unit->type == TYPE_ONESHOT) {
        int status = 0;
        if (waitpid(pid, &status, 0) == pid) {
            unit->pid = 0;
            unit->failed = !(WIFEXITED(status) && WEXITSTATUS(status) == 0);
            log_process_exit(unit, "Control process", status);
        } else {
            unit->pid = 0;
            unit->failed = 1;
        }
        if (unit->failed) {
            printf("[FAILED] Failed to start %s.\n", unit_ident(unit));
            printf("systemg[1]: %s: Failed with result 'exit-code'.\n", unit->name);
        } else {
            printf("[  OK  ] Started %s.\n", unit_ident(unit));
        }
    }
    return pid;
}

static void start_ready_units(void) {
    int progress = 1;
    while (progress) {
        progress = 0;
        for (int index = 0; index < unit_count; index++) {
            struct unit *unit = &units[index];
            if (unit->started) {
                continue;
            }
            if (!dependencies_ready(unit)) {
                continue;
            }
            start_unit(unit);
            progress = 1;
        }
    }
    for (int index = 0; index < unit_count; index++) {
        struct unit *unit = &units[index];
        if (unit->started) {
            continue;
        }
        for (int dependency = 0; dependency < unit->requires_count; dependency++) {
            int other = unit_index(unit->requires[dependency]);
            if (other >= 0 && (!units[other].started || units[other].failed)) {
                printf("[DEPEND] Dependency failed for %s.\n", unit_ident(unit));
                unit->failed = 1;
                unit->started = 1;
                break;
            }
        }
    }
}

static struct unit *unit_for_pid(pid_t pid) {
    for (int index = 0; index < unit_count; index++) {
        if (units[index].pid == pid) {
            return &units[index];
        }
    }
    return NULL;
}

static int should_restart(const struct unit *unit) {
    if (unit->pending_restart) {
        return 1;
    }
    if (unit->restart == RESTART_ALWAYS) {
        return 1;
    }
    if (unit->restart == RESTART_ON_FAILURE && unit->failed) {
        return 1;
    }
    return 0;
}

int main(int argc, char **argv, char **envp) {
    (void)argc;
    (void)argv;
    (void)envp;
    int console = open("/dev/console", O_RDWR);
    if (console >= 0) {
        dup2(console, 0);
        dup2(console, 1);
        dup2(console, 2);
        if (console > 2) {
            close(console);
        }
    }
    setenv("PATH", "/bin:/sbin", 1);
    setenv("HOME", "/root", 1);
    signal(SIGCHLD, child_handler);
    signal(SIGINT, SIG_IGN);
    signal(SIGQUIT, SIG_IGN);
    signal(SIGTERM, SIG_IGN);
    signal(SIGTSTP, SIG_IGN);
    signal(SIGTTIN, SIG_IGN);
    signal(SIGTTOU, SIG_IGN);
    printf("SYSTEMG_OK pid=%d\n", getpid());

    DIR *directory = opendir("/etc/systemg");
    if (directory != NULL) {
        struct dirent *entry;
        while ((entry = readdir(directory)) != NULL) {
            if (entry->d_name[0] == '.') {
                continue;
            }
            size_t length = strlen(entry->d_name);
            if (length < 9 || strcmp(entry->d_name + length - 8, ".service") != 0) {
                continue;
            }
            char path[256];
            snprintf(path, sizeof(path), "/etc/systemg/%s", entry->d_name);
            char name[64];
            copy_name(name, sizeof(name), entry->d_name);
            name[length - 8] = 0;
            load_unit(path, name);
        }
        closedir(directory);
    } else {
        printf("systemg: no /etc/systemg directory\n");
    }
    start_ready_units();

    for (;;) {
        int status = 0;
        pid_t pid;
        while ((pid = waitpid(-1, &status, WNOHANG | WUNTRACED | WCONTINUED)) > 0) {
            if (WIFSTOPPED(status)) {
                continue;
            }
            if (WIFCONTINUED(status)) {
                continue;
            }
            struct unit *unit = unit_for_pid(pid);
            if (unit != NULL) {
                unit->pid = 0;
                unit->failed = !(WIFEXITED(status) && WEXITSTATUS(status) == 0);
                log_process_exit(unit, "Main process", status);
                if (unit->failed) {
                    printf("systemg[1]: %s: Failed with result 'exit-code'.\n",
                           unit->name);
                } else {
                    printf("systemg[1]: %s: Deactivated successfully.\n", unit->name);
                }
                printf("[  OK  ] Stopped %s.\n", unit_ident(unit));
                if (should_restart(unit)) {
                    unit->pending_restart = 1;
                }
            }
        }
        for (int index = 0; index < unit_count; index++) {
            struct unit *unit = &units[index];
            if (unit->pid == 0 && unit->started && should_restart(unit)) {
                unit->restarts += 1;
                printf("systemg[1]: %s: Scheduled restart job, restart counter is at %d.\n",
                       unit->name, unit->restarts);
                sleep((unsigned int)unit->restart_sec);
                start_unit(unit);
            }
        }
        child_changed = 0;
        usleep(100000);
    }
}
