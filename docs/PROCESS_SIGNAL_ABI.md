# SpaceOS-9 进程、信号与 init 设计

日期：2026-10-25。状态：设计已选型；PMM 引用计数与 CoW 地址空间复制已落地内核自测，系统调用/用户页故障尚未接线。

## 1. 选型结论

| 主题 | 选择 |
| --- | --- |
| 进程 ABI | 完整 Linux `clone`：进程与线程（CLONE_VM/THREAD/SIGHAND/FILES）、CoW fork、wait4/waitid、kill、进程组/会话 |
| 信号 | 完整 POSIX 信号：sigaction、sigprocmask、sigsuspend、rt_sigreturn、siginfo、altstack、实时信号、作业控制信号 |
| init | systemg：轻量 systemd 子集（unit INI、Before/After/Requires/Wants、Type=simple/exec/oneshot、Restart=no/on-failure/always、控制台日志） |
| C 运行库 | 自研 splibc；先静态，随后 `libsplibc.so` + `ld.so` + PT_INTERP/DT_NEEDED |
| 交付 | FAT32 分区作为根 `/`；`/sbin/systemg`、`/bin/sh`、`/etc/systemg/*.service` |
| sh | 自研 POSIX 风格 shell，带管道/重定向/作业控制 |
| 顺序 | 内核、splibc、systemg、sh 并行推进；以下里程碑用于保证依赖和验收，不作为串行等待 |

调研依据：

- [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)、[systemd.target](https://www.freedesktop.org/software/systemd/man/latest/systemd.target.html)：unit 依赖、Type、Restart、target 语义。
- [runit](https://smarden.org/runit/)：最小 PID 1、三段式启动、监督树；用于约束 PID 1 只做最小工作。
- [clone(2)](https://man7.org/linux/man-pages/man2/clone.2.html)、[wait(2)](https://man7.org/linux/man-pages/man2/wait.2.html)：clone flags、exit_signal、僵尸与 wait4 语义。
- [fork(2)](https://man7.org/linux/man-pages/man2/fork.2.html)、[signal(7)](https://man7.org/linux/man-pages/man7/signal.7.html)、[futex(2)](https://man7.org/linux/man-pages/man2/futex.2.html)。

## 2. 进程/线程模型

- 调度实体仍是 `Task`，但增加用户进程身份：`pid`（线程组 ID，tgid）、`tid`、`ppid`、`pgid`、`sid`、`exit_signal`、退出状态。
- 内核线程 `pid = 0`，不进入 Linux PID 表；PID 1 预留给 systemg。用户 PID 从独立计数器分配，不复用启动自测的内核 tid。
- 线程共享：地址空间、文件描述符表、fs 上下文、信号处理表；每线程独立：寄存器/FPU、信号掩码、pending 信号、TLS、内核栈、clear_child_tid。
- `clone` 参数按 x86_64/AArch64/RISC-V 原始系统调用顺序解析，校验非法组合；支持 CLONE_VM、FS、FILES、SIGHAND、THREAD、PARENT、CHILD_CLEARTID、CHILD_SETTID、SETTLS 和退出信号。
- `fork` 实现为 `clone(SIGCHLD, stack=0)`；`vfork` 首轮可映射到 `clone(CLONE_VM|CLONE_VFORK|SIGCHLD)` 并在 exec 前挂起父进程，若不实现则返回 ENOSYS，不能用错误语义冒充。
- 进程退出后保留最小僵尸记录，直到父进程 wait；父进程先退出时，孤儿（含僵尸）改挂 PID 1，由 systemg 的 `wait(-1)` 回收。
- 进程组/会话：`setpgid`、`getpgid`、`setsid`、`getsid`；控制终端记录 session 和 foreground pgrp。

## 3. CoW 设计

已落地的基础：

- PMM 每个 4 KiB 帧一个 `u16` 引用计数，元数据在 PMM bitmap 之后预留并清零。
- `alloc` 置 1；`retain` 增加；`release` 减少并在 0 时归还；`free` 只允许引用计数为 1，防止共享帧被误释放。
- `UserAddressSpace::fork_copy()`：对每个私有页 `retain`，子进程映射同一物理页并写保护；全部子映射成功后才把父进程 PTE 改为只读 + COW，失败路径先回滚。
- `cow_break()`：引用计数为 1 时直接恢复写权限；否则分配新页、复制、减旧引用、替换 PTE，并做 TLB 刷新。
- `write_bytes()` 在内核代写用户缓冲前先 `cow_break`，避免 read/write/execve 等系统调用绕过 COW 改写共享页。
- `unmap`/`Drop` 对私有页使用 `release`；COW 页在最后一个引用释放时归还 PMM。
- 自测已覆盖：fork 后双方数据隔离、父子各自写入、单侧退出后另一侧独占复用、PMM 帧数回到基线。

后续：

- x86_64 `#PF` 已接入 `cow_break`；AArch64 `ESR/FAR`、RISC-V `scause/stval` 的用户写故障入口仍需改造。非 COW 故障目前终止进程，完整 SIGSEGV 投递随信号子系统实现。
- 建立 VMA 抽象：当前 `mapped_frames` 按页记录，后续需要合并为 VMA（权限、文件偏移、共享/私有、增长方向），以支持 mmap/mprotect/munmap 的 Linux 语义和 `/proc/pid/maps`。
- 文件私有映射当前由 `mmap` 复制到匿名页；接入 VMA 后改为按页 COW，避免一次性复制。
- 页面错误与 OOM 路径不能持有 VMM/PMM 锁进入调度；需要可睡眠的缺页路径和失败回滚。

## 4. 用户页故障与信号

- 用户态缺页分为：COW 写、匿名按需零页、文件页读入、栈增长、权限错误。
- x86_64：CR2 + PageFaultErrorCode（P/W/U）；AArch64：ESR_EL1 EC/ISS + FAR_EL1；RISC-V：scause 12/13/15 + stval。
- COW 只处理用户态、写、地址落在已映射 VMA 内的故障；内核态访问用户地址必须走显式 uaccess，不得用缺页处理绕过。
- 无法解析的故障按信号规则投递 SIGSEGV/SIGBUS；没有处理器时终止进程并记录 wait 状态。
- 信号帧保存完整用户寄存器、FPU 状态、信号掩码和 altstack；`rt_sigreturn` 恢复。AArch64/RISC-V 需要新增从异常返回用户态的可恢复路径，当前异常入口是 `-> !`，必须改造。

## 5. wait、kill 与 PID 1

- 新增 `wait4`/`waitid`：pid > 0、pid == 0、pid == -1、pid < -1 的进程组语义；WNOHANG、WUNTRACED、WCONTINUED、WNOWAIT。
- 等待队列按父进程/子进程关系唤醒；SIGCHLD 默认动作是忽略，但显式 SIG_IGN/SA_NOCLDWAIT 影响僵尸保留。
- `kill`/`tkill`/`tgkill` 校验权限和进程组/会话；SIGKILL/SIGSTOP 不可捕获/阻塞。
- PID 1 语义：systemg 必须回收所有孤儿；PID 1 退出时内核 panic/reboot，而不是留下无 init 系统。
- 子进程退出信号、核心转储标志、停止/继续状态都编码进 wait status。

## 6. 信号、线程与 futex

- 进程级 `SignalDisposition` 表 + 每线程 `pending`/`blocked` 掩码 + 共享 pending 队列；实时信号按编号和排队顺序投递。
- `sigaction` 保存 handler、flags、restorer、mask；`sigprocmask` 修改阻塞集；`sigsuspend` 原子替换掩码并等待。
- `futex` 首轮实现 WAIT、WAKE、REQUEUE、WAIT_BITSET、WAKE_OP 的常用子集；支持 `clear_child_tid` 和 pthread 加入/退出。
- 线程退出用 `exit_group` 终止整个线程组；最后一个线程退出才产生进程退出状态。
- TTY 作业控制：后台进程读/写终端时产生 SIGTTIN/SIGTTOU；`tcsetpgrp` 只允许同会话；SIGTSTP/SIGCONT 驱动 stopped/continued 状态。

## 7. splibc、systemg、sh

### splibc

- 先提供静态 `libsplibc.a`：`crt0`、原始 syscall 包装、`_start`/`exit`、字符串/内存、`printf` 子集、`malloc`（brk/mmap）、`argv/env`、`fork/exec/wait` 包装。
- 按 PIC 设计，随后补 `libsplibc.so`、`ld.so`、`PT_INTERP`、`DT_NEEDED`、GOT/PLT 重定位和符号解析。
- 不引入 glibc；不把内核头文件直接暴露给用户态。

### systemg

- 启动：PID 1，挂载/检查根文件系统，打开 `/dev/console`，设置 stdout/stderr，加载 `/etc/systemg/*.service`。
- unit：`[Unit] Description/Before/After/Requires/Wants`，`[Service] ExecStart/Type/Restart/RestartSec`，可选的 `ExecStop`。
- 依赖：构建 DAG，检测环，按拓扑顺序启动；`Requires` 失败传播，`Wants` 失败不阻塞。
- 监督：fork/exec；SIGCHLD 回收；按 Restart 策略重启；控制台日志带服务名前缀；速率限制。
- 首轮不做 D-Bus、cgroup、journal、socket activation、sd_notify；这些在后续里程碑补。

### sh

- 交互式命令行：提示符、历史可选、内建 `cd/exit/export`。
- 外部命令：fork/execvp/wait；管道、重定向、`&&`/`||`/`;`。
- 作业控制：`&`、`fg`、`bg`、`jobs`、Ctrl-C/Ctrl-Z，使用进程组、会话、TTY foreground pgrp 和 SIGCHLD/SIGTSTP/SIGCONT。

## 8. FAT32 根文件系统

- 构建时生成 FAT32 镜像，目录布局：
  - `/sbin/systemg`
  - `/bin/sh`
  - `/etc/systemg/*.service`
  - `/dev`、`/proc`、`/sys` 作为覆盖挂载点
- 内核启动时把首选 FAT32 分区挂到 `/`，再覆盖挂载 devtmpfs/procfs/sysfs；移除当前固定 `/mnt` 路径对 init 的影响。
- 镜像生成使用宿主 `mkfs.vfat`/`mtools`，构建脚本把编译产物写入镜像；QEMU 测试直接挂载该镜像。
- 没有可启动根盘时，内核记录明确错误，不伪造 init 成功。

## 9. 里程碑与验收

| 里程碑 | 内容 | 验收 |
| --- | --- | --- |
| P1 | PMM 引用计数、CoW 地址空间复制、内核代写 COW | 已通过内核自测（数据隔离、引用归零、PMM 基线） |
| P2 | 用户页故障、clone/fork、wait4、僵尸/收养、PID 1 | x86_64 fork+wait+CoW 用户程序已通过；execve/PID1/孤儿回收未完成 |
| P3 | 完整信号、线程 clone、futex、kill、进程组/会话、TTY 作业控制 | 多线程共享/隔离、信号掩码/处理器/altstack、Ctrl-C/Ctrl-Z、wait 状态 |
| P3b | 核心 IPC：AF_UNIX、SCM_RIGHTS、eventfd/signalfd/timerfd/epoll、memfd、futex | eventfd2 已通过；socketpair/abstract/pathname、fd 传递、LT/ET、fd 继承、poll/epoll 未完成 |
| P4 | 静态 splibc、systemg、sh、FAT32 根盘 | QEMU 从 FAT32 启动 systemg，启动 sh，执行管道/重定向/作业控制 |
| P5 | 动态 splibc/ld.so、三架构、故障注入 | 三架构编译/启动；OOM、exec 失败、信号竞争、fork 失败回滚 |

验收必须使用实际运行的 QEMU 日志和退出码；未运行的项目写入“未验收”，不能用模型或占位值代替。

## 10. 当前进度

- 已完成：PMM `u16` 帧引用计数、`retain/release/refcount`、CoW `fork_copy`/`cow_break`、写路径 COW 处理、`unmap`/`Drop` 引用释放、内核自测。
- 已完成（x86_64）：用户写页故障的 COW 解析；`SYS_CLONE`/`SYS_FORK`/`SYS_WAIT4`/`SYS_GETPPID`；僵尸保留、wait4 回收、父进程退出后孤儿 ppid 改挂 1；内核自测回收自身子进程。
- 已通过的真实验证：`user/test/fork_wait_x86_64.S` 在 QEMU x86_64 2D 启动中输出 `child ok`、`parent ok`，父进程 wait 到子进程退出码 7 后以 42 退出，`SELFTEST_OK`/`BOOT_OK` 通过。该测试覆盖了 fork 返回、COW 数据页写入、wait4 和僵尸回收。
- 已开始 IPC：`src/ipc/mod.rs` 实现 `eventfd2`（counter、EFD_SEMAPHORE、EFD_NONBLOCK、EFD_CLOEXEC、poll 可读可写），内核 syscall 自测覆盖 counter/read/write/poll/EAGAIN/信号量模式。
- 未完成：CLONE_VM/THREAD、execve 后 PID 保持、PID 1 与真正孤儿回收、kill/信号、进程组/会话/TTY 作业控制、futex、AF_UNIX/SCM_RIGHTS/epoll/signalfd/timerfd/memfd、splibc/systemg/sh、FAT32 根盘。AArch64/RISC-V 的 clone/fork 目前返回 ENOSYS，只保证编译。
- 下一项：补 execve/PID 1 与 kill/信号基础，同时落 IPC 的 `eventfd` + `socketpair(AF_UNIX, SOCK_STREAM)` 最小闭环。

## 11. IPC 子系统（已选型，尚未实现）

选型：核心 IPC 全量，单 IPC namespace，首轮不做 netlink。范围包括：

- 管道/FIFO（现有 `pipe2` 作为起点，补齐阻塞/非阻塞、poll/epoll 就绪与 fork/exec 继承）。
- AF_UNIX：pathname/abstract/unnamed；SOCK_STREAM/SOCK_DGRAM/SOCK_SEQPACKET；`socketpair`、bind/listen/connect/accept、send/recv/sendmsg/recvmsg、shutdown、backlog、close/unlink/GC。
- SCM_RIGHTS、SCM_CREDENTIALS、SO_PEERCRED、`SCM_MAX_FD=253`、`RLIMIT_NOFILE`/in-flight fd 限制、cmsg 截断 `MSG_CTRUNC` 与多余 fd 关闭。
- eventfd（含 `EFD_SEMAPHORE`/`EFD_CLOEXEC`/`EFD_NONBLOCK`）、signalfd（依赖完整信号）、timerfd（依赖内核定时器）、epoll（LT/ET、ONESHOT、epoll fd 可 poll、fork 继承）。
- memfd（`MFD_CLOEXEC`/`MFD_ALLOW_SEALING`、`F_ADD_SEALS`/`F_GET_SEALS`、ftruncate、`MAP_SHARED`）、共享 mmap、futex。
- 统一 `Socket`/`Inode` 文件对象、等待队列驱动的 readiness、fd 传递使用 `Arc<OpenFile>`、权限与单 namespace 内的地址注册表。

调研依据：[unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html)、[epoll(7)](https://man7.org/linux/man-pages/man7/epoll.7.html)、[eventfd(2)](https://man7.org/linux/man-pages/man2/eventfd.2.html)、[memfd_create(2)](https://man7.org/linux/man-pages/man2/memfd_create.2.html)；SysV IPC/`mq_overview` 已调研但不在首轮范围：[sysvipc(7)](https://man7.org/linux/man-pages/man7/sysvipc.7.html)、[mq_overview(7)](https://man7.org/linux/man-pages/man7/mq_overview.7.html)。

当前状态：`eventfd2` 已实现并通过内核 syscall 自测；AF_UNIX、SCM_RIGHTS、epoll、signalfd、timerfd、memfd 尚未开始。未实现项继续返回真实错误，不以空对象冒充。
