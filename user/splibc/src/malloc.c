/*
 *       user/splibc/src/malloc.c
 *       Boundary-tag heap over anonymous mmap
 *       2026/10/25 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 */

#include "libc.h"

typedef struct Block {
    size_t size;
    struct Block *next;
    struct Block *previous;
    unsigned long free;
    unsigned long reserved;
} Block;

static Block *blocks;

static void insert_block(Block *block) {
    Block *previous = NULL;
    Block *current = blocks;
    while (current != NULL && (unsigned long)current < (unsigned long)block) {
        previous = current;
        current = current->next;
    }
    block->previous = previous;
    block->next = current;
    if (previous != NULL) {
        previous->next = block;
    } else {
        blocks = block;
    }
    if (current != NULL) {
        current->previous = block;
    }
}

static Block *extend_heap(size_t size) {
    size_t total = size + sizeof(Block);
    total = (total + 4095) & ~4095UL;
    void *memory = mmap(NULL, total, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (memory == MAP_FAILED) {
        return NULL;
    }
    Block *block = memory;
    block->size = total - sizeof(Block);
    block->next = NULL;
    block->previous = NULL;
    block->free = 1;
    insert_block(block);
    return block;
}

void *malloc(size_t size) {
    if (size == 0) {
        size = 1;
    }
    size = (size + 15) & ~15UL;
    for (Block *block = blocks; block != NULL; block = block->next) {
        if (!block->free || block->size < size) {
            continue;
        }
        if (block->size >= size + sizeof(Block) + 16) {
            Block *tail = (Block *)((char *)block + sizeof(Block) + size);
            tail->size = block->size - size - sizeof(Block);
            tail->free = 1;
            block->size = size;
            insert_block(tail);
        }
        block->free = 0;
        return (char *)block + sizeof(Block);
    }
    Block *block = extend_heap(size);
    if (block == NULL) {
        errno = ENOMEM;
        return NULL;
    }
    block->free = 0;
    return (char *)block + sizeof(Block);
}

void free(void *pointer) {
    if (pointer == NULL) {
        return;
    }
    Block *block = (Block *)((char *)pointer - sizeof(Block));
    block->free = 1;
    Block *next = block->next;
    if (next != NULL && next->free
        && (char *)next == (char *)block + sizeof(Block) + block->size) {
        block->size += sizeof(Block) + next->size;
        block->next = next->next;
        if (next->next != NULL) {
            next->next->previous = block;
        }
    }
    Block *previous = block->previous;
    if (previous != NULL && previous->free
        && (char *)block == (char *)previous + sizeof(Block) + previous->size) {
        previous->size += sizeof(Block) + block->size;
        previous->next = block->next;
        if (block->next != NULL) {
            block->next->previous = previous;
        }
    }
}

void *calloc(size_t count, size_t size) {
    if (size != 0 && count > (~0UL) / size) {
        errno = ENOMEM;
        return NULL;
    }
    size_t total = count * size;
    void *pointer = malloc(total);
    if (pointer != NULL) {
        memset(pointer, 0, total);
    }
    return pointer;
}

void *realloc(void *pointer, size_t size) {
    if (pointer == NULL) {
        return malloc(size);
    }
    if (size == 0) {
        free(pointer);
        return NULL;
    }
    Block *block = (Block *)((char *)pointer - sizeof(Block));
    if (block->size >= size) {
        return pointer;
    }
    void *replacement = malloc(size);
    if (replacement == NULL) {
        return NULL;
    }
    memcpy(replacement, pointer, block->size);
    free(pointer);
    return replacement;
}
