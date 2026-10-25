/*
 *       user/splibc/src/stdio.c
 *       Minimal printf family and character output
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

struct output {
    char *buffer;
    size_t capacity;
    size_t length;
};

static void emit_char(struct output *out, char c) {
    if (out->length + 1 < out->capacity) {
        out->buffer[out->length] = c;
    }
    out->length++;
}

static void emit_repeat(struct output *out, char c, int count) {
    for (int i = 0; i < count; i++) {
        emit_char(out, c);
    }
}

static void emit_number(struct output *out, unsigned long value, int base, int upper,
                        int negative, int width, int left, int zero, int plus,
                        int space, int alternate, int precision) {
    const char *alphabet = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char digits[64];
    int count = 0;
    if (value == 0 && precision != 0) {
        digits[count++] = '0';
    }
    while (value != 0) {
        digits[count++] = alphabet[value % (unsigned)base];
        value /= (unsigned)base;
    }
    while (count < precision) {
        digits[count++] = '0';
    }
    char prefix[4];
    int prefix_length = 0;
    if (negative) {
        prefix[prefix_length++] = '-';
    } else if (plus) {
        prefix[prefix_length++] = '+';
    } else if (space) {
        prefix[prefix_length++] = ' ';
    }
    if (alternate && base == 16 && !(count == 1 && digits[0] == '0')) {
        prefix[prefix_length++] = '0';
        prefix[prefix_length++] = upper ? 'X' : 'x';
    } else if (alternate && base == 8 && !(count == 1 && digits[0] == '0')) {
        prefix[prefix_length++] = '0';
    }
    int total = prefix_length + count;
    int padding = width > total ? width - total : 0;
    if (!left && !zero) {
        emit_repeat(out, ' ', padding);
    }
    for (int i = 0; i < prefix_length; i++) {
        emit_char(out, prefix[i]);
    }
    if (!left && zero) {
        emit_repeat(out, '0', padding);
    }
    for (int i = count - 1; i >= 0; i--) {
        emit_char(out, digits[i]);
    }
    if (left) {
        emit_repeat(out, ' ', padding);
    }
}

static void emit_string(struct output *out, const char *text, int length, int width,
                        int left, int precision) {
    if (text == NULL) {
        text = "(null)";
    }
    int size = (int)strlen(text);
    if (precision >= 0 && size > precision) {
        size = precision;
    }
    if (length >= 0 && size > length) {
        size = length;
    }
    int padding = width > size ? width - size : 0;
    if (!left) {
        emit_repeat(out, ' ', padding);
    }
    for (int i = 0; i < size; i++) {
        emit_char(out, text[i]);
    }
    if (left) {
        emit_repeat(out, ' ', padding);
    }
}

int vsnprintf(char *buffer, size_t size, const char *format, __builtin_va_list arguments) {
    struct output out;
    out.buffer = buffer;
    out.capacity = size;
    out.length = 0;
    for (const char *p = format; *p != 0; p++) {
        if (*p != '%') {
            emit_char(&out, *p);
            continue;
        }
        p++;
        int left = 0;
        int zero = 0;
        int plus = 0;
        int space = 0;
        int alternate = 0;
        for (;; p++) {
            if (*p == '-') {
                left = 1;
            } else if (*p == '0') {
                zero = 1;
            } else if (*p == '+') {
                plus = 1;
            } else if (*p == ' ') {
                space = 1;
            } else if (*p == '#') {
                alternate = 1;
            } else {
                break;
            }
        }
        int width = 0;
        while (*p >= '0' && *p <= '9') {
            width = width * 10 + (*p - '0');
            p++;
        }
        int precision = -1;
        if (*p == '.') {
            p++;
            precision = 0;
            while (*p >= '0' && *p <= '9') {
                precision = precision * 10 + (*p - '0');
                p++;
            }
        }
        int length = 0;
        if (*p == 'l') {
            p++;
            length = 1;
            if (*p == 'l') {
                length = 2;
                p++;
            }
        } else if (*p == 'z' || *p == 'j' || *p == 't') {
            length = 3;
            p++;
        } else if (*p == 'h') {
            p++;
            if (*p == 'h') {
                p++;
            }
        }
        char specifier = *p;
        if (specifier == 0) {
            break;
        }
        switch (specifier) {
        case '%':
            emit_char(&out, '%');
            break;
        case 'c':
            emit_string(&out, (char[2]){(char)__builtin_va_arg(arguments, int), 0}, -1,
                        width, left, -1);
            break;
        case 's': {
            const char *text = __builtin_va_arg(arguments, const char *);
            emit_string(&out, text, -1, width, left, precision);
            break;
        }
        case 'd':
        case 'i': {
            long long value;
            if (length == 0) {
                value = __builtin_va_arg(arguments, int);
            } else if (length == 1) {
                value = __builtin_va_arg(arguments, long);
            } else if (length == 2) {
                value = __builtin_va_arg(arguments, long long);
            } else {
                value = (long)__builtin_va_arg(arguments, ssize_t);
            }
            int negative = value < 0;
            unsigned long magnitude = negative ? (unsigned long)(-(value + 1)) + 1UL
                                               : (unsigned long)value;
            emit_number(&out, magnitude, 10, 0, negative, width, left,
                        zero && precision < 0 && !left, plus, space, 0, precision);
            break;
        }
        case 'u':
        case 'x':
        case 'X':
        case 'o': {
            unsigned long value;
            if (length == 0) {
                value = __builtin_va_arg(arguments, unsigned int);
            } else if (length == 1) {
                value = __builtin_va_arg(arguments, unsigned long);
            } else if (length == 2) {
                value = __builtin_va_arg(arguments, unsigned long long);
            } else {
                value = __builtin_va_arg(arguments, size_t);
            }
            int base = specifier == 'o' ? 8 : (specifier == 'u' ? 10 : 16);
            emit_number(&out, value, base, specifier == 'X', 0, width, left,
                        zero && precision < 0 && !left, 0, 0,
                        alternate, precision);
            break;
        }
        case 'p': {
            unsigned long value = (unsigned long)__builtin_va_arg(arguments, void *);
            if (value == 0) {
                emit_string(&out, "(nil)", -1, width, left, -1);
            } else {
                emit_number(&out, value, 16, 0, 0, width, left, zero, 0, 0, 1, -1);
            }
            break;
        }
        default:
            emit_char(&out, '%');
            emit_char(&out, specifier);
            break;
        }
    }
    if (out.capacity != 0) {
        size_t index = out.length < out.capacity ? out.length : out.capacity - 1;
        out.buffer[index] = 0;
    }
    return (int)out.length;
}

static int vformat_to_fd(int fd, const char *format, __builtin_va_list arguments) {
    char stack[512];
    __builtin_va_list copy;
    __builtin_va_copy(copy, arguments);
    int length = vsnprintf(stack, sizeof(stack), format, copy);
    __builtin_va_end(copy);
    if (length < 0) {
        return length;
    }
    if ((size_t)length < sizeof(stack)) {
        ssize_t written = write(fd, stack, (size_t)length);
        return written == length ? length : -1;
    }
    char *heap = malloc((size_t)length + 1);
    if (heap == NULL) {
        errno = ENOMEM;
        return -1;
    }
    __builtin_va_copy(copy, arguments);
    vsnprintf(heap, (size_t)length + 1, format, copy);
    __builtin_va_end(copy);
    ssize_t written = write(fd, heap, (size_t)length);
    free(heap);
    return written == length ? length : -1;
}

int fprintf(int fd, const char *format, ...) {
    __builtin_va_list arguments;
    __builtin_va_start(arguments, format);
    int result = vformat_to_fd(fd, format, arguments);
    __builtin_va_end(arguments);
    return result;
}

int dprintf(int fd, const char *format, ...) {
    __builtin_va_list arguments;
    __builtin_va_start(arguments, format);
    int result = vformat_to_fd(fd, format, arguments);
    __builtin_va_end(arguments);
    return result;
}

int printf(const char *format, ...) {
    __builtin_va_list arguments;
    __builtin_va_start(arguments, format);
    int result = vformat_to_fd(1, format, arguments);
    __builtin_va_end(arguments);
    return result;
}

int snprintf(char *buffer, size_t size, const char *format, ...) {
    __builtin_va_list arguments;
    __builtin_va_start(arguments, format);
    int result = vsnprintf(buffer, size, format, arguments);
    __builtin_va_end(arguments);
    return result;
}

int puts(const char *text) {
    size_t length = strlen(text);
    if (write(1, text, length) != (ssize_t)length) {
        return EOF;
    }
    if (write(1, "\n", 1) != 1) {
        return EOF;
    }
    return 0;
}

int fputs(const char *text, int fd) {
    size_t length = strlen(text);
    return write(fd, text, length) == (ssize_t)length ? 0 : EOF;
}

int putchar(int c) {
    char byte = (char)c;
    return write(1, &byte, 1) == 1 ? c : EOF;
}

int isatty(int fd) {
    struct termios settings;
    return ioctl(fd, TCGETS, &settings) == 0;
}
