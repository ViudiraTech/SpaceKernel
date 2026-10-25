# SpaceOS-9 Linux ABI test payloads

This directory contains the minimal user-mode payloads used by the kernel
self-test:

| File | Purpose |
| --- | --- |
| `test_x86_64.S` / `test_x86_64.elf` | x86_64 Linux ABI test program |
| `test_aarch64.S` / `test_aarch64.elf` | AArch64 Linux ABI test program |
| `test_riscv64.S` / `test_riscv64.elf` | RISC-V 64 Linux ABI test program |

Each program writes a fixed greeting with the architecture's `write` syscall,
then exits with status 42 (`exit_group`/`exit`). It deliberately does not use
libc, a dynamic loader, or any SpaceOS-9-specific user library.

`src/user/test_binaries.rs` embeds the prebuilt `.elf` file for the active
target with `include_bytes!`. The ELF files are checked in because the kernel
build does not require cross binutils. When changing a `.S` file, rebuild the
matching `.elf` with the target toolchain and preserve the existing load
address and entry layout.
