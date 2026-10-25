<p align="center">
  <!-- 将生成的 Logo 保存为 docs/SpaceOS9.png；建议 1024x1024 PNG。 -->
  <img src="docs/SpaceOS9.png" alt="SpaceOS-9" width="420">
</p>

<h1 align="center">SpaceOS-9</h1>

<p align="center"><strong>面向 Linux ABI 的多架构完整操作系统</strong></p>

SpaceOS-9 是 ViudiraTech 自主研发的完整操作系统项目。目标不是构建一个只完成引导的内核样例，而是建立一套从固件引导、内存与进程管理、文件系统、设备驱动，到 Linux ABI 用户态和图形显示栈都自主可控的操作系统。

## Goal

SpaceOS-9 的长期目标：

- **一套核心覆盖多种架构**：以 x86_64、AArch64、RISC-V 64 为目标架构，共享架构无关核心；x86_64 优先完成完整设备链路。
- **保留 Linux ABI**：让 Linux 用户态程序、工具链和图形栈能够在 SpaceOS-9 上运行，而不是另造一套不兼容的用户接口。
- **成为完整操作系统**：提供进程与线程、虚拟内存、VFS、设备模型、同步与 IPC、TTY、网络与图形等系统服务，而不仅是内核入口。
- **建设 Linux DRM/KMS 兼容图形栈**：以 Linux DRM/KMS ioctl 语义为兼容目标，首批覆盖 VirtIO GPU 的 2D、VirGL 和 Venus 内核链路。
- **安全与可维护性优先**：内核使用 Rust，保持 soft-float，默认不依赖硬件 FP/SIMD；资源生命周期、失败路径和并发边界必须可审计。
- **可重复验证**：构建、启动、设备协议和用户态行为都要有可重复的测试；不使用常量、占位值或未运行的结果冒充真实能力。

## 系统蓝图

| 层次 | 目标 |
| --- | --- |
| 引导与固件 | Limine/UEFI 启动、ACPI 与 Device Tree 硬件描述、KASLR、三架构页表与异常入口 |
| 内核核心 | 物理/虚拟内存管理、SLAB、SMP、EEVDF 调度、时钟与中断、同步原语、日志 |
| 文件系统与设备 | VFS、devtmpfs、sysfs、procfs、设备注册/注销、IDE/AHCI/NVMe、TTY 与输入设备 |
| Linux ABI | ELF/PIE/PT_INTERP 装载、进程与线程、mmap/mprotect/brk、文件与元数据系统调用、信号与 futex 演进 |
| 图形与显示 | DRM/KMS、GEM、PRIME、syncobj、fbcon 控制台、VirtIO GPU 2D/VirGL/Venus |
| 构建与验证 | Kconfig、三架构 ISO、QEMU 启动矩阵、主机侧模型测试、协议/ABI 探针与资源回收检查 |

## 构建与运行

构建环境需要 Rust/rustup、GNU Make、Python 3、`tar`、`sha256sum`、`xorriso`，以及目标架构的 QEMU 和 UEFI 固件。`make menuconfig`/`make defconfig` 需要 Kconfig frontends 提供的 `kconfig-mconf`/`kconfig-conf`。主机侧 ABI 探针需要 `gcc` 和系统 DRM UAPI 头文件；GPU 图形测试需要宿主提供可用的 DRM render node 和 virglrenderer。

```sh
make menuconfig  # 选择架构、profile、KASLR、内核命令行与 QEMU 配置
make             # 构建启动 ISO
make run         # 在 QEMU 中启动
```

没有本地 `.config` 时，Makefile 使用 `.config-default`；运行过 menuconfig 后，`.config` 优先。`make` 会自动安装 Rust target、获取 Cargo 依赖，并下载校验过的 Limine 发行包。产物位于 `build/<arch>/<profile>/SpaceKernel-<arch>.iso`。

常用目标：

```sh
make check                 # rustfmt + 三架构 cargo check
make test                  # 主机侧模型测试 + 三架构 QEMU 启动矩阵
make test ARCH=x86_64      # 单架构启动测试
make debug                 # QEMU 暂停在启动处，GDB 端口 1234
```

`make test` 的 x86_64 项使用 VirtIO GPU/Venus 设备；如果宿主机没有 `/dev/dri/renderD128`，可用 `tools/qemu_test.py --gpu 2d` 运行无 3D 的显示链路测试。

## 保密与使用限制

**Confidential — Internal Use Only. Copyright (C) 2026 ViudiraTech. All rights reserved.**

本仓库及其中的源代码、设计文档、测试数据、构建产物、Logo、名称和商标均属于 ViudiraTech 的保密信息。本仓库未附带开源许可证，不构成任何形式的开源授权、分发许可或商业使用许可。

未经 ViudiraTech 书面授权，禁止：

- 向任何第三方披露、分发、转售或公开展示本仓库的任何部分；
- 将本仓库内容上传到公共代码托管、网盘、论坛、公共 AI 模型或其他不受控环境；
- 基于本仓库创建衍生作品并对外发布；
- 移除、修改或隐藏版权、保密、归属和商标标识。

内部使用应遵循最小权限和按需知悉原则。如发现泄露、未授权访问或安全问题，请立即通过私有渠道联系项目维护者。

## 文档

- [子系统设计与验收契约](docs/SPACEOS9_SUBSYSTEM_DESIGN.md)
- [SMP 与调度器](docs/SCHEDULER.md)
- [CPU 与扩展状态](docs/CPU_STATE.md)
- [源码约定](docs/CODING_STYLE.md)

---

Copyright (C) 2026 ViudiraTech. All rights reserved.
