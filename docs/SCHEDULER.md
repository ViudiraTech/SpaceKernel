# SMP 与调度器

2026/9/30 By JiTianYu391

## CPU 上线

`src/smp.rs` 统一管理 x86_64、AArch64、RISC-V 的上线流程，逻辑 CPU 编号是 Limine MP 列表中的索引，硬件 ID 单独保存。BSP 可以位于列表中的任意位置，不把 APIC ID、MPIDR 或 hart ID 当作数组下标。

BSP 先准备共享内核页表、各核异常设施、FPU 初始模板和运行队列。`MpInfo::bootstrap` 先写 `extra_argument`，再用 release 原子写发布 AP 入口。保留 Limine 的 `reserved` 字段及启动栈。

AP 在中断关闭时安装内核页表和本核异常设施，探测本核 CPU 能力，检查与 BSP 的能力、地址宽度和 FPU 保存区 ABI 是否兼容，再初始化本地中断控制器和定时器。全部成功后才以 release 发布 online。失败会报告阶段并停留在离线状态，BSP 的上线等待有超时。各核通过 BSP 的最后一道屏障后开始调度。

| 架构 | CPU 本地状态 | 异常设施 | 抢占定时器 | 重调度通知 |
| --- | --- | --- | --- | --- |
| x86_64 | GS 指向永久 CPU 索引 | 每核 GDT、TSS、IDT、IST 栈 | TSC deadline；否则按参考计数器校准 LAPIC one-shot | LAPIC IPI |
| AArch64 | TPIDR_EL1 指向 CPU 状态 | 每核异常栈、VBAR_EL1、EL1h 栈 | CNTV timer、PPI 27 | GICv2/v3 SGI |
| RISC-V | sscratch 指向 hart 状态 | 每核异常栈、stvec | SBI TIME timer | SBI IPI，支持 mask base |

AP 不重复初始化共享 I/O APIC、GIC distributor 或 PLIC 路由。启动栈和异常设施永久保留；普通任务栈在退出切换后回收。

## EEVDF

每核使用独立运行队列。虚拟时间是所有可运行实体的加权平均 `V = sum(weight * vruntime) / sum(weight)`，包含当前任务。只有 `vruntime <= V` 的任务 eligible，从中选择最早虚拟 deadline。负数使用向下取整，避免 eligibility 在整数边界处失真。

默认实际请求为 4 ms，虚拟请求长度是 `4 ms * 1024 / weight`，nice 权重采用完整 40 项表。实际执行时间按权重转换成虚拟执行时间，并保留除法余数。抢占和主动 yield 保留尚未完成的请求及 deadline；请求耗尽时续期。运行队列最小 vruntime 根据当前任务和就绪树共同推进。

睡眠和迁移保存有界的 `V - vruntime`，新队列放置时按 `(W+w)/W` 补偿加入实体导致的虚拟时间偏移，同时保留剩余请求。短暂睡眠不会直接把负 lag 清零。这是项目自己的有界 lag 策略，不实现 Linux 当前的 deferred dequeue 睡眠衰减机制。

就绪队列采用 2-3 左倾红黑树，完整实现旋转、颜色翻转、top-down 删除和黑高修复。每个子树维护最小 vruntime、加权和、总权重及节点数，eligible 选择、插入和删除均为 O(log n)。节点在创建任务时预先分配，调度、等待、睡眠及迁移复用同一节点，不在 IRQ 中分配树节点。

## 切换、等待与生命周期

锁顺序是 CPU 编号递增的运行队列锁 → 等待/定时队列锁 → 任务元数据锁。唤醒者先取出等待节点并释放等待锁，再与源运行队列同步，之后根据亲和性和负载选择目标 CPU。

上下文切换把源运行队列锁交给同一 CPU 上的新栈，在 `finish_switch` 中释放。其他核不能在旧栈指针、callee-saved 寄存器及 FPU 状态保存前迁移或唤醒该任务。切换中不保留普通的 IRQ 恢复 guard。任务的 context/FPU 存储是私有 `UnsafeCell`，访问受运行队列所有权约束；公开句柄只提供元数据快照。

抢占只在架构 IRQ 出口执行，控制器先完成 EOI/complete。AArch64 将 ELR_EL1、SPSR_EL1 保存在任务栈上的 IRQ frame 中；RISC-V 同样保留 sepc、sstatus。暂停在 IRQ 出口的任务恢复时使用自己的返回状态。

`WaitQueue::wait_until` 在等待锁保护下检查条件并登记任务；条件先发布，再调用 wake。FIFO 队列避免先入后出；wake-all 只唤醒调用时已存在的批次。`sleep_micros` 将任务放入按到期时间排序的树，CPU 可以执行其他任务。退出任务在另一栈上的切换收尾中释放资源，不留下永久持有任务的全局列表。

## 负载均衡与 API

创建和唤醒优先选择亲和性允许的空闲 CPU，并在负载相近时考虑 SMT 核共享和包内缓存亲近性。实际探测到的拓扑生成 SMT/core、package 和 system 调度域；未知拓扑仅使用 system 域。重复的域会合并。

空闲核主动拉取工作；周期拉取按核错开，默认 20 ms。迁移只处理就绪任务，检查亲和性，按 CPU 编号顺序 try-lock 两个队列，每次最多迁移 4 个任务、每次候选扫描最多 32 个节点。普通均衡避免迁移最近 2 ms 内执行过的任务；空闲核可忽略缓存热度。权重检查避免一次迁移把负载差反向放大。

```rust
let tid = sched::spawn(|| {
    // 拥有 Send + 'static 闭包，也支持普通 fn()。
    sched::sleep_micros(10_000);
}, 0)?;

let affinity = sched::CpuMask::empty().with_cpu(0).with_cpu(2);
sched::spawn_with_affinity(work, -5, affinity)?;
let snapshot = sched::current().unwrap().stats();
```

当前仅提供内核线程、固定的创建时亲和性和启动时 CPU 上线。用户进程、fork/COW、信号、VFS、CPU 热插拔及跨核 TLB shootdown 尚未实现。普通任务使用 64 KiB 分配器栈和底部 canary 检查，不声称提供未映射的 guard page。

## 统一验证

```sh
make test-sched-model  # 同一份算法代码的宿主机测试
make check            # 格式与三个目标的 all-features 检查
make test-all         # 三架构 debug/release × 1/4 核，再加 GICv3、Sv48：16 个启动用例
```

`make test` 要求 CPU/FPU、内存/硬件、scheduler 和 `BOOT_OK` 标记，并要求在线 CPU 数等于 QEMU 配置。每个 ELF 都做普通内核代码 FP/SIMD 指令审计。统一测试日志和 JSON 汇总在 `build/sched-tests/`。

相同的内核测试包含随机红黑树操作及颜色/黑高/增广字段校验、加权 EEVDF 模型、真实 CPU 时间份额、本核真实定时器、无 yield 的硬件抢占、FIFO 和跨核唤醒竞态、定时阻塞、FP/SIMD/FPU 图像隔离、实际迁移、携带已保存 FPU 状态的跨核恢复，以及 64 任务/8192 次 yield 和退出回收。单核配置运行相同入口，跳过需要另一核的迁移部分。

## 算法与协议资料

- [Limine MP 协议](https://github.com/Limine-Bootloader/limine-protocol/blob/trunk/PROTOCOL.md#mp-multiprocessor-feature)：AP 发布、入口状态、保留字段。
- [Linux EEVDF 文档](https://docs.kernel.org/scheduler/sched-eevdf.html) 与 [fair.c](https://github.com/torvalds/linux/blob/master/kernel/sched/fair.c)：eligibility、加权虚拟时间、lag 放置推导。
- [Princeton / Sedgewick & Wayne 红黑树算法](https://algs4.cs.princeton.edu/33balanced/RedBlackBST.java.html)：LLRB 不变量及完整删除算法。
- [Linux 调度域文档](https://docs.kernel.org/scheduler/sched-domains.html)：拓扑域、负载拉取及运行队列锁顺序。
