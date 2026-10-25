/*
 *       user/sh/sh.c
 *       POSIX-style shell with pipelines, redirection and job control
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "splibc.h"

#define MAX_TOKENS 64
#define MAX_ARGS 32
#define MAX_REDIRS 4
#define MAX_JOBS 16
#define MAX_LINE 1024

struct redirection {
    int fd;
    char path[256];
    int flags;
};

struct command {
    char *argv[MAX_ARGS];
    int argc;
    struct redirection redirs[MAX_REDIRS];
    int redir_count;
};

enum job_state { JOB_FREE = 0, JOB_RUNNING, JOB_STOPPED };

struct job {
    int state;
    pid_t pgid;
    char command[128];
};

static struct job jobs[MAX_JOBS];
static pid_t shell_pgid;
static int last_status;
static int interactive;

static void child_handler(int signal_number) {
    (void)signal_number;
}

static char *trim(char *text) {
    while (*text == ' ' || *text == '\t') {
        text++;
    }
    char *end = text + strlen(text);
    while (end > text && (end[-1] == ' ' || end[-1] == '\t')) {
        *--end = 0;
    }
    return text;
}

static int split_tokens(char *text, char *tokens[], int maximum) {
    int count = 0;
    char *cursor = text;
    while (*cursor != 0 && count + 1 < maximum) {
        while (*cursor == ' ' || *cursor == '\t') {
            cursor++;
        }
        if (*cursor == 0) {
            break;
        }
        char *start = cursor;
        char *write = cursor;
        int quote = 0;
        while (*cursor != 0) {
            if (*cursor == '"' || *cursor == '\'') {
                if (quote == 0) {
                    quote = *cursor;
                    cursor++;
                    continue;
                }
                if (quote == *cursor) {
                    quote = 0;
                    cursor++;
                    continue;
                }
            }
            if (!quote && (*cursor == ' ' || *cursor == '\t')) {
                break;
            }
            *write++ = *cursor++;
        }
        if (*cursor != 0) {
            cursor++;
        }
        *write = 0;
        tokens[count++] = start;
    }
    tokens[count] = NULL;
    return count;
}

static int parse_command(char *text, struct command *command) {
    char *tokens[MAX_TOKENS];
    int count = split_tokens(text, tokens, MAX_TOKENS);
    command->argc = 0;
    command->redir_count = 0;
    for (int index = 0; index < count; index++) {
        char *token = tokens[index];
        int fd = -1;
        int flags = 0;
        if (strcmp(token, ">") == 0) {
            fd = 1;
            flags = O_WRONLY | O_CREAT | O_TRUNC;
        } else if (strcmp(token, ">>") == 0) {
            fd = 1;
            flags = O_WRONLY | O_CREAT | O_APPEND;
        } else if (strcmp(token, "<") == 0) {
            fd = 0;
            flags = O_RDONLY;
        } else if (strcmp(token, "2>") == 0) {
            fd = 2;
            flags = O_WRONLY | O_CREAT | O_TRUNC;
        }
        if (fd >= 0) {
            if (index + 1 >= count || command->redir_count >= MAX_REDIRS) {
                return -1;
            }
            command->redirs[command->redir_count].fd = fd;
            command->redirs[command->redir_count].flags = flags;
            {
                size_t length = strlen(tokens[index + 1]);
                if (length >= sizeof(command->redirs[0].path)) {
                    return -1;
                }
                strcpy(command->redirs[command->redir_count].path, tokens[index + 1]);
            }
            command->redir_count++;
            index++;
            continue;
        }
        if (command->argc + 1 >= MAX_ARGS) {
            return -1;
        }
        command->argv[command->argc++] = token;
    }
    command->argv[command->argc] = NULL;
    return command->argc == 0 ? -1 : 0;
}

static int is_builtin(const char *name) {
    return strcmp(name, "cd") == 0 || strcmp(name, "exit") == 0
        || strcmp(name, "export") == 0 || strcmp(name, "jobs") == 0
        || strcmp(name, "fg") == 0 || strcmp(name, "bg") == 0
        || strcmp(name, "pwd") == 0 || strcmp(name, "echo") == 0
        || strcmp(name, "true") == 0 || strcmp(name, "false") == 0
        || strcmp(name, "help") == 0;
}

static int add_job(pid_t pgid, const char *command, int state) {
    for (int index = 0; index < MAX_JOBS; index++) {
        if (jobs[index].state == JOB_FREE) {
            jobs[index].state = state;
            jobs[index].pgid = pgid;
            size_t length = strlen(command);
            if (length >= sizeof(jobs[index].command)) {
                length = sizeof(jobs[index].command) - 1;
            }
            memcpy(jobs[index].command, command, length);
            jobs[index].command[length] = 0;
            return index + 1;
        }
    }
    return 0;
}

static const char *job_state_text(int state) {
    return state == JOB_STOPPED ? "Stopped" : "Running";
}

static void reap_jobs(void) {
    for (int index = 0; index < MAX_JOBS; index++) {
        struct job *job = &jobs[index];
        if (job->state == JOB_FREE) {
            continue;
        }
        int status = 0;
        pid_t result;
        for (;;) {
            result = waitpid(-job->pgid, &status, WNOHANG | WUNTRACED | WCONTINUED);
            if (result > 0) {
                if (WIFSTOPPED(status)) {
                    job->state = JOB_STOPPED;
                } else if (WIFCONTINUED(status)) {
                    job->state = JOB_RUNNING;
                }
                continue;
            }
            break;
        }
        if (result < 0 && errno == ECHILD) {
            printf("[%d]+  Done                    %s\n", index + 1, job->command);
            job->state = JOB_FREE;
        }
    }
}

static int run_builtin(struct command *command) {
    const char *name = command->argv[0];
    if (strcmp(name, "cd") == 0) {
        const char *target = command->argc > 1 ? command->argv[1] : getenv("HOME");
        if (target == NULL || *target == 0) {
            target = "/";
        }
        if (chdir(target) < 0) {
            fprintf(2, "sh: cd: %s: %s\n", target, strerror(errno));
            return 1;
        }
        char buffer[PATH_MAX];
        if (getcwd(buffer, sizeof(buffer)) != NULL) {
            setenv("PWD", buffer, 1);
        }
        return 0;
    }
    if (strcmp(name, "exit") == 0) {
        int status = command->argc > 1 ? atoi(command->argv[1]) : last_status;
        exit(status & 0xff);
    }
    if (strcmp(name, "export") == 0) {
        for (int index = 1; index < command->argc; index++) {
            char *entry = command->argv[index];
            char *equals = strchr(entry, '=');
            if (equals != NULL) {
                *equals = 0;
                setenv(entry, equals + 1, 1);
                *equals = '=';
            }
        }
        return 0;
    }
    if (strcmp(name, "pwd") == 0) {
        char buffer[PATH_MAX];
        if (getcwd(buffer, sizeof(buffer)) == NULL) {
            return 1;
        }
        puts(buffer);
        return 0;
    }
    if (strcmp(name, "echo") == 0) {
        int index = 1;
        int newline = 1;
        if (index < command->argc && strcmp(command->argv[index], "-n") == 0) {
            newline = 0;
            index++;
        }
        for (int position = index; position < command->argc; position++) {
            if (position > index) {
                putchar(' ');
            }
            fputs(command->argv[position], 1);
        }
        if (newline) {
            putchar('\n');
        }
        return 0;
    }
    if (strcmp(name, "true") == 0) {
        return 0;
    }
    if (strcmp(name, "false") == 0) {
        return 1;
    }
    if (strcmp(name, "help") == 0) {
        puts("builtins: cd exit export jobs fg bg pwd echo true false help");
        return 0;
    }
    if (strcmp(name, "jobs") == 0) {
        for (int index = 0; index < MAX_JOBS; index++) {
            if (jobs[index].state != JOB_FREE) {
                printf("[%d]  %-8s %s\n", index + 1,
                       job_state_text(jobs[index].state), jobs[index].command);
            }
        }
        return 0;
    }
    if (strcmp(name, "fg") == 0 || strcmp(name, "bg") == 0) {
        int which = command->argc > 1 ? atoi(command->argv[1]) - 1 : -1;
        if (which < 0) {
            for (int index = MAX_JOBS - 1; index >= 0; index--) {
                if (jobs[index].state != JOB_FREE) {
                    which = index;
                    break;
                }
            }
        }
        if (which < 0 || which >= MAX_JOBS || jobs[which].state == JOB_FREE) {
            fprintf(2, "sh: %s: no such job\n", name);
            return 1;
        }
        if (strcmp(name, "bg") == 0) {
            kill(-jobs[which].pgid, SIGCONT);
            jobs[which].state = JOB_RUNNING;
            printf("[%d]+ %s & %s\n", which + 1, job_state_text(JOB_RUNNING),
                   jobs[which].command);
            return 0;
        }
        if (interactive) {
            tcsetpgrp(0, jobs[which].pgid);
        }
        kill(-jobs[which].pgid, SIGCONT);
        jobs[which].state = JOB_RUNNING;
        int status = 0;
        while (waitpid(-jobs[which].pgid, &status, WUNTRACED) > 0) {
            if (WIFSTOPPED(status)) {
                jobs[which].state = JOB_STOPPED;
                break;
            }
        }
        if (interactive) {
            tcsetpgrp(0, shell_pgid);
        }
        last_status = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
        return last_status;
    }
    return 127;
}

static void apply_redirections(struct command *command) {
    for (int index = 0; index < command->redir_count; index++) {
        struct redirection *redirection = &command->redirs[index];
        int fd = open(redirection->path, redirection->flags, 0644);
        if (fd < 0) {
            fprintf(2, "sh: %s: %s\n", redirection->path, strerror(errno));
            _exit(1);
        }
        dup2(fd, redirection->fd);
        if (fd != redirection->fd) {
            close(fd);
        }
    }
}

static int split_top_level(char *text, char separator, char *parts[], int maximum) {
    int count = 0;
    char *start = text;
    int quote = 0;
    for (char *cursor = text;; cursor++) {
        char character = *cursor;
        if (character == '"' || character == '\'') {
            if (quote == 0) {
                quote = character;
            } else if (quote == character) {
                quote = 0;
            }
        }
        if ((character == separator && quote == 0) || character == 0) {
            if (count + 1 >= maximum) {
                return -1;
            }
            *cursor = 0;
            parts[count++] = trim(start);
            if (character == 0) {
                break;
            }
            start = cursor + 1;
        }
    }
    parts[count] = NULL;
    return count;
}

static int run_pipeline(char *text, int background) {
    char *parts[MAX_TOKENS];
    int count = split_top_level(text, '|', parts, MAX_TOKENS);
    if (count <= 0) {
        return 0;
    }
    struct command commands[MAX_TOKENS];
    for (int index = 0; index < count; index++) {
        if (parse_command(parts[index], &commands[index]) < 0) {
            fprintf(2, "sh: syntax error\n");
            return 2;
        }
    }
    if (count == 1 && !background && commands[0].redir_count == 0
        && is_builtin(commands[0].argv[0])) {
        last_status = run_builtin(&commands[0]);
        return last_status;
    }
    pid_t pids[MAX_TOKENS];
    pid_t pgid = 0;
    int previous_read = -1;
    for (int index = 0; index < count; index++) {
        int pipe_fds[2] = {-1, -1};
        if (index + 1 < count && pipe(pipe_fds) < 0) {
            fprintf(2, "sh: pipe: %s\n", strerror(errno));
            return 1;
        }
        pid_t pid = fork();
        if (pid < 0) {
            fprintf(2, "sh: fork: %s\n", strerror(errno));
            return 1;
        }
        if (pid == 0) {
            if (index == 0) {
                setpgid(0, 0);
            } else {
                setpgid(0, pgid);
            }
            if (previous_read >= 0) {
                dup2(previous_read, 0);
                close(previous_read);
            }
            if (pipe_fds[1] >= 0) {
                dup2(pipe_fds[1], 1);
                close(pipe_fds[1]);
                close(pipe_fds[0]);
            }
            apply_redirections(&commands[index]);
            if (is_builtin(commands[index].argv[0])) {
                _exit(run_builtin(&commands[index]));
            }
            execvp(commands[index].argv[0], commands[index].argv);
            fprintf(2, "sh: %s: %s\n", commands[index].argv[0], strerror(errno));
            _exit(127);
        }
        if (index == 0) {
            pgid = pid;
        }
        setpgid(pid, pgid);
        pids[index] = pid;
        if (previous_read >= 0) {
            close(previous_read);
        }
        if (pipe_fds[1] >= 0) {
            close(pipe_fds[1]);
            previous_read = pipe_fds[0];
        } else {
            previous_read = -1;
        }
    }
    if (previous_read >= 0) {
        close(previous_read);
    }
    if (background) {
        int number = add_job(pgid, text, JOB_RUNNING);
        printf("[%d] %d\n", number, pgid);
        return 0;
    }
    if (interactive) {
        tcsetpgrp(0, pgid);
    }
    int status = 0;
    int stopped = 0;
    for (int index = 0; index < count; index++) {
        int child_status = 0;
        if (waitpid(pids[index], &child_status, WUNTRACED) < 0) {
            break;
        }
        status = child_status;
        if (WIFSTOPPED(child_status)) {
            stopped = 1;
            break;
        }
    }
    if (interactive) {
        tcsetpgrp(0, shell_pgid);
    }
    if (stopped) {
        int number = add_job(pgid, text, JOB_STOPPED);
        printf("\n[%d]+  Stopped                 %s\n", number, text);
        last_status = 128 + WSTOPSIG(status);
    } else if (WIFEXITED(status)) {
        last_status = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
        int signal_number = WTERMSIG(status);
        last_status = 128 + signal_number;
        if (signal_number == SIGINT && interactive) {
            putchar('\n');
        }
    }
    return last_status;
}

static int find_operator(char *text, char **position) {
    int quote = 0;
    for (char *cursor = text; cursor[0] != 0; cursor++) {
        if (cursor[0] == '"' || cursor[0] == '\'') {
            if (quote == 0) {
                quote = cursor[0];
            } else if (quote == cursor[0]) {
                quote = 0;
            }
        }
        if (quote != 0) {
            continue;
        }
        if (cursor[0] == '&' && cursor[1] == '&') {
            *position = cursor;
            return 1;
        }
        if (cursor[0] == '|' && cursor[1] == '|') {
            *position = cursor;
            return 2;
        }
    }
    return 0;
}

static int run_sequence(char *text) {
    char *cursor = trim(text);
    int status = 0;
    for (;;) {
        char *position = NULL;
        int operator = find_operator(cursor, &position);
        if (operator == 0) {
            return run_pipeline(cursor, 0);
        }
        char saved = position[0];
        position[0] = 0;
        status = run_pipeline(cursor, 0);
        position[0] = saved;
        cursor = trim(position + 2);
        if (operator == 1 && status != 0) {
            return status;
        }
        if (operator == 2 && status == 0) {
            return status;
        }
    }
}

static int strip_background(char *text) {
    size_t length = strlen(text);
    int quote = 0;
    for (size_t index = 0; index < length; index++) {
        char character = text[index];
        if (character == '"' || character == '\'') {
            if (quote == 0) {
                quote = character;
            } else if (quote == character) {
                quote = 0;
            }
        }
    }
    if (quote != 0) {
        fprintf(2, "sh: unmatched quote\n");
        return -1;
    }
    while (length > 0 && (text[length - 1] == ' ' || text[length - 1] == '\t')) {
        text[--length] = 0;
    }
    if (length > 0 && text[length - 1] == '&') {
        text[length - 1] = 0;
        return 1;
    }
    return 0;
}

static int run_line(char *line) {
    char *parts[MAX_TOKENS];
    int count = split_top_level(line, ';', parts, MAX_TOKENS);
    if (count < 0) {
        return 1;
    }
    int status = 0;
    for (int index = 0; index < count; index++) {
        char *part = trim(parts[index]);
        if (*part == 0) {
            continue;
        }
        int background = strip_background(part);
        if (background < 0) {
            return 1;
        }
        part = trim(part);
        if (*part == 0) {
            continue;
        }
        if (background) {
            /* Background sequences are executed as one pipeline. */
            status = run_pipeline(part, 1);
        } else {
            status = run_sequence(part);
        }
    }
    return status;
}

static int read_line(int fd, char *buffer, int capacity) {
    int length = 0;
    while (length + 1 < capacity) {
        char byte;
        ssize_t count = read(fd, &byte, 1);
        if (count < 0) {
            if (errno == EINTR) {
                reap_jobs();
                continue;
            }
            return -1;
        }
        if (count == 0) {
            return length == 0 ? 0 : length;
        }
        if (byte == '\n') {
            break;
        }
        if (byte == '\r') {
            continue;
        }
        buffer[length++] = byte;
    }
    buffer[length] = 0;
    return length + 1;
}

int main(int argc, char **argv, char **envp) {
    (void)envp;
    shell_pgid = getpid();
    if (argc >= 3 && strcmp(argv[1], "-c") == 0) {
        interactive = 0;
        signal(SIGCHLD, child_handler);
        int status = run_line(argv[2]);
        return status & 0xff;
    }
    interactive = isatty(0);
    if (interactive) {
        if (tcgetpgrp(0) != shell_pgid) {
            setpgid(0, shell_pgid);
            tcsetpgrp(0, shell_pgid);
        }
        signal(SIGINT, SIG_IGN);
        signal(SIGQUIT, SIG_IGN);
        signal(SIGTSTP, SIG_IGN);
        signal(SIGTTIN, SIG_IGN);
        signal(SIGTTOU, SIG_IGN);
    }
    signal(SIGCHLD, child_handler);
    printf("SH_PROMPT\n");
    char line[MAX_LINE];
    for (;;) {
        if (interactive) {
            fputs("$ ", 1);
        }
        int result = read_line(0, line, sizeof(line));
        if (result <= 0) {
            break;
        }
        if (result > 1) {
            last_status = run_line(line);
        }
        reap_jobs();
    }
    return last_status & 0xff;
}
