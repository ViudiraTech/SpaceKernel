<p align="center">
  <img src="docs/SpaceKernel.png" alt="SpaceKernel logo" width="420">
</p>

<h1 align="center">SpaceKernel</h1>

<p align="center">基于 Rust 和 Limine 的多架构内核项目，按可编译、可启动、可测试的阶段持续开发。</p>

目前可为 **x86_64、aarch64 和 RISC-V 64** 构建启动 ISO，并在 QEMU 中运行。RISC-V 默认使用 **Sv39**，Sv48 可在 Kconfig 中选择。CPU 能力查询与扩展寄存器状态管理提供统一接口，普通内核代码默认不使用硬件 FP/SIMD。源码按架构、内存管理、终端、日志和硬件描述分模块组织。项目仍处于基础启动阶段；下文明确列出了当前边界。

## 快速开始

构建环境需要 Rust/rustup、GNU Make、Python 3、`tar`、`sha256sum`、`xorriso`，以及目标架构的 QEMU 和 UEFI 固件。`make test` 的机器码检查另需 LLVM objdump（支持 `llvm-objdump`、`llvm-objdump-21` 或 `llvm-objdump-19`）。`make menuconfig`、`make defconfig` 分别需要 Kconfig frontends 提供的 `kconfig-mconf`、`kconfig-conf`。`make` 会安装所选 Rust target、下载 Cargo 依赖，并自动获取校验过的 Limine 二进制发行包。Limine 使用 8 路并行 HTTP Range 下载；可用 `make DOWNLOAD_JOBS=16` 调整并发数。

```sh
make menuconfig  # 选择架构、debug/release、KASLR、内核命令行及 QEMU 配置
make             # 生成启动 ISO
make run         # 在 GTK 窗口中启动 QEMU；串口输出显示在当前终端
```

首次不运行 menuconfig 也可以构建：Makefile 会使用仓库内的 `.config-default`。运行 menuconfig 后，本地 `.config` 优先；用 `make defconfig` 可按 `Kconfig` 默认值重新生成它。默认命令行为 `console=tty0 console=ttyS0`，同时输出到 framebuffer 和串口；改成 `console=ttyS0` 可只使用串口。没有可用 framebuffer 时会回退到串口。

生成文件位于 `build/<arch>/<profile>/SpaceKernel-<arch>.iso`，例如 `build/x86_64/debug/SpaceKernel-x86_64.iso`。可临时覆盖配置中的架构：

```sh
make run ARCH=aarch64
make test ARCH=riscv64
```

## 测试与调试

```sh
make test-all             # 三架构统一调度测试：debug/release、1/4 核、GICv3、Sv48
make check                # rustfmt + 三架构 cargo check
make test ARCH=x86_64     # 构建 ISO，在无图形 QEMU 中等待串口 BOOT_OK
make test ARCH=aarch64
make test ARCH=riscv64
make test ARCH=riscv64 CONFIG_RISCV_SV48=y  # Sv48
make test ARCH=aarch64 MACHINE=virt,gic-version=3
make debug                # QEMU 暂停在启动处，GDB 端口 1234
```

启用 `CONFIG_BOOT_SELF_TEST` 时，启动自测检查 PMM/VMM/SLAB/TTY/IRQ/FDT、扩展寄存器保存恢复与隔离、嵌套内核借用、非法状态导入，以及真实 timer IRQ。调度测试同时验证各核上线、硬件抢占、EEVDF 份额、跨核等待唤醒、FPU 迁移隔离与任务退出回收。`make test` 检查最终 ELF 的普通代码无 FP/SIMD 指令，并要求自测标记与 `BOOT_OK`；统一矩阵的逐项日志和 JSON 汇总保存在 `build/sched-tests/`；QEMU 验证仍不等同于实机验证。

## 当前实现

| 子系统 | 当前范围 |
| --- | --- |
| 启动与架构 | Limine 请求、KASLR 配置、三架构串口和页表接口、x86_64 GDT/IDT 与其他架构的异常向量 |
| SMP 与调度 | Limine AP 启动、每核异常/FPU/控制器初始化、增广红黑树 EEVDF、定时抢占、亲和性、拓扑域拉取、等待/定时睡眠、切换后回收 |
| 内存 | PMM 位图、按架构几何的内核 VMM（含 Sv39/Sv48）、单次映射、SLAB 及大块分配 |
| CPU 能力与扩展状态 | 无锁 BSP 能力快照、三架构探测、x87/SSE/AVX XSAVE 或 FXSAVE、AArch64 FP/AdvSIMD、RISC-V F/D 状态管理与显式内核借用 |
| 输出 | 有界 printk 环形记录、`console=` 路由、8 个 framebuffer 虚拟终端、ANSI 基础控制与滚屏历史 |
| 硬件描述 | ACPI RSDP/SDT 校验及 MADT、FADT、MCFG、SRAT 解析；Device Tree 头部校验 |
| 构建 | Kconfig 菜单、debug/release、三架构 ISO、QEMU 运行与启动测试 |

## 源码目录

| 路径 | 职责 |
| --- | --- |
| `src/boot.rs`、`src/arch/` | Limine 响应；各架构的 CPU、分页、串口、异常与中断入口 |
| `src/smp.rs`、`src/sched/` | 三架构 CPU 上线、每核运行队列、EEVDF、跨核迁移与调度自测 |
| `src/cpuid/`、`src/fpu/` | 架构无关 CPU 能力与扩展寄存器所有权接口 |
| `src/mm/` | PMM、VMM、SLAB 和大块堆分配 |
| `src/pci/` | MCFG/ECAM 配置访问、固件配置总线枚举、设备快照和 capability 链解析 |
| `src/printk/` | 日志记录与有序控制台输出 |
| `src/tty/console/`、`src/tty/line/` | 控制台选择、TTY 设备分发、输入行规程 |
| `src/tty/fbcon/` | 虚拟终端状态、ANSI 解析、滚屏与像素绘制 |
| `src/hardware/acpi/`、`src/hardware/fdt.rs` | 固件硬件描述解析 |
| `src/self_test.rs`、`tools/` | 启动自测与镜像/QEMU 辅助脚本 |

## 并发约束与后续工作

共享状态由关闭本地中断的自旋锁和 acquire/release 原子操作保护。VMM 锁与各 SLAB 类锁可以获取 PMM 锁，反向获取禁止；printk 在写控制台前释放日志环锁；持有 TTY 锁时不调用分配器或日志接口。AP 已按统一流程上线并参与调度。运行队列锁按 CPU 编号排序；切换把源队列锁交给新栈释放，保证旧栈保存后才能迁移或唤醒。

跨核 TLB shootdown、VMM 解除映射、用户态的扩展状态接入、可返回的用户异常处理、键盘/UART 中断输入以及用户空间设备接口仍需实现。已有 APIC/GIC/PLIC 与 timer IRQ，以及 ACPI/FDT 驱动的 PCI 固件拓扑枚举。PLIC 的 hart 上下文目前需要设备树的 `interrupts-extended`，不会猜测 ACPI-only 平台的上下文布局。

调度算法、接口、锁顺序和统一测试见 [SMP 与调度器](docs/SCHEDULER.md)。接口契约与当前边界见 [CPU 与扩展状态说明](docs/CPU_STATE.md)，文件头和注释规范见 [源码约定](docs/CODING_STYLE.md)，本轮问题修复与验证见 [审查记录](docs/REVIEW_2026-09-30.md)。
