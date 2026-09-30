# CPU 能力、分页与扩展寄存器状态

2026/9/30 By JiTianYu391

## 能力接口

`cpuid::init()` 在 BSP 上发布不可变的 CPU 快照。`cpuid::info()` 提供身份、地址宽度、能力位、可用的缓存/拓扑描述和计数器频率；`cpuid::has()` 无锁读取硬件能力，可用于 NMI 中的能力判断。字符串有容量限制并转换为可显示的 ASCII；未知的地址宽度、缓存和拓扑用 `None` 表示。原生数值厂商/修订 ID 保持完整 64 位；RISC-V 的 SBI mvendorid/mimpid 获取失败也用 `None` 表示，不截断成 x86 family 字段。

x86 后端检查基本、扩展、虚拟机 CPUID 叶范围，解析 family/model/stepping、品牌、确定性缓存和扩展拓扑，并提供 `query_x86(leaf, subleaf)` 作为原始查询接口。原始接口只处理公共终止规则；其他叶的专有子叶语义由调用方解释。AArch64 使用 EL1 ID 寄存器，RISC-V 使用 SBI BASE 和对应 hart 的设备树节点，避免在 S 模式读取 `misa` 等机器态 CSR。

硬件支持与 OS 管理是两件事。执行扩展指令前，使用 `fpu::config().supports(feature, &cpu)` 判断寄存器状态是否受到管理，再取得寄存器所有权。SVE、SME、RVV、AMX、MPX、PKRU 等状态没有在本次实现中开放，不能因为硬件报告支持就执行它们。AArch64/RISC-V 不会被错误标记为支持 x86 SSE/AVX。

AP 上线前，需要在正确 CPU 上调用 `unsafe cpuid::detect_current(index)`，检查 `cpuid::compatible` 所需能力与地址宽度，并调用 `unsafe fpu::init_current_cpu(&local_info)`。地址宽度低于 BSP、已知宽度变成未知，或扩展状态格式、大小/掩码不匹配时拒绝上线。当前仓库尚未启动 AP，不能把这些入口的存在等同于完成 SMP 验证。

## 分页模式

RISC-V 默认请求 Sv39，使用三级页表、39 位规范虚拟地址与 1 GiB 的空闲根槽。`CONFIG_RISCV_SV48=y` 或 Cargo 的 `riscv-sv48` feature 请求 Sv48，使用四级页表和 512 GiB 根槽。启动器响应必须与请求相同，切换根表保留实际 SATP 模式。

VMM 根据后端几何计算地址和遍历级数，克隆启动根表，保持引导器内核/HHDM 映射。新子树先在未发布的页框中完成；分配失败归还页框，完成后用释放屏障发布父链接并执行架构 TLB 同步。非叶链接与叶页采用不同刷新接口；RISC-V 发布非叶项使用 rs1=rs2=x0 的 SFENCE.VMA，覆盖可能缓存的无效项及所有后代地址。页表遍历拒绝把大页当成下级表；映射拒绝未对齐或超出描述符/硬件地址范围的物理地址，设备范围先检查溢出。

当前 VMM 仍是单次填充、单调保留的内核映射器，没有用户空间页权限接口、解除映射或跨核 TLB shootdown。成功映射的设备范围保持到内核结束；部分设备范围映射失败也可能保留已成功的页面。这些页面不会被释放或重新分配成其他用途。

## FPU/SIMD 状态所有权

普通内核和依赖使用不自动生成 FP/SIMD 的目标：`x86_64-unknown-none`、`aarch64-unknown-none-softfloat`、`riscv64imac-unknown-none-elf`。硬件指令只放在架构扩展状态后端；不要为了某个辅助函数给整个内核打开 SSE/AVX/F/D codegen。

| 后端 | 管理的状态 | 默认禁用机制 |
| --- | --- | --- |
| x86 FXSAVE | x87、MMX、16 个 XMM、MXCSR | CR0.TS |
| x86 XSAVE 标准格式 | 上述状态、支持时的 YMM、opmask 和 ZMM | CR0.TS；仅开放被管理的 XCR0 位 |
| AArch64 | 32 个 Q 寄存器、FPCR、FPSR | CPACR_EL1.FPEN |
| RISC-V F/D | 32 个 F 寄存器、FCSR | sstatus.FS；RVV 保持关闭 |

x86 在 CPUID 检查后配置 CR0/CR4，写入实际 XCR0 后重新读取 CPUID.0D 的保存区大小，并验证所选组件的范围。保存区按 64 字节对齐，动态分配；使用标准 XSAVE，避免 XSAVEOPT 对保存目的区和初始化状态的额外假设。初始化模板设置 x87 FCW=0x037f、MXCSR=0x1f80，所有寄存器负载清零。

每个任务持有一个不透明 `fpu::State`：

- `State::new` 创建干净状态；`try_clone` 复制已保存的父任务；`reset` 用于 exec。
- `save_current` 保存旧所有者并关闭硬件门；`restore_current` 恢复新所有者并保持内核禁用。
- `switch(previous, next)` 提供主动保存/恢复的调度接入点，避免 lazy-FPU 所有权泄露。
- `enable_user` 只能在已恢复的用户任务返回前调用；所有内核入口需关闭门，并在调度前保存该任务。
- `bytes` 和 `import` 提供未来 signal/ptrace 接口的内部存储层。导入要求精确长度，检查 MXCSR、XSAVE 掩码/标准格式/保留字段或本架构控制字段，失败保持原状态。

保存、恢复、切换和 AP 初始化为 `unsafe` 接口：调用者必须持有当前 CPU/任务的所有权，关闭中断及抢占，防止迁移和并发修改。它们不能代替尚不存在的调度器、用户返回路径或 signal ABI。状态缓冲区在释放前用 volatile 写清零。

## 显式内核借用

辅助入口声明为：

```rust
pub unsafe fn with_kernel<T>(
    scratch: &mut State,
    operation: impl FnOnce() -> T,
) -> Result<T, Error>;
```

调用前分配 scratch 并检查所需能力。函数在本 CPU 关闭中断，保存原所有者，恢复干净模板，执行回调，然后恢复原状态和原硬件门/中断状态。嵌套借用必须使用不同 scratch。回调不能打开 IRQ、睡眠、调度、迁移或让寄存器所有权逃出作用域；NMI/异常/IRQ 处理代码始终使用普通整数 ABI。本项目没有实际业务代码借用 FP/SIMD，仅状态管理与启动自测使用该接口。

## 验证与边界

启动自测对所有受管理的寄存器负载和控制状态执行模式写入、恢复、保存、两份上下文切换、克隆、重置和嵌套借用，并检查错误长度、保留控制位和非法 XSAVE 元数据。真实 timer IRQ 测试覆盖架构中断入口/返回，并验证返回后 FP 门仍关闭。

`make test` 运行 `tools/check_no_simd.py` 检查最终 ELF，再要求 CPU/FPU 与原有内存/TTY/IRQ/FDT 自测标记及 `BOOT_OK`。检查器需要 LLVM objdump；可用 `tools/qemu_test.py --cpu ... --machine ... --require ...` 验证兼容路径。QEMU 不能替代实机、SMP、用户态 ABI 或长时间负载验证；当前阶段不能宣称整个内核或该子系统已获得工业可靠性认证。

设计依据包括 [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)、[RISC-V 特权规范](https://docs.riscv.org/reference/isa/priv/supervisor.html)、[RISC-V SBI 规范](https://github.com/riscv-non-isa/riscv-sbi-doc) 与架构寄存器定义。文件头风格参考 [Uinxed-Kernel 的 FPU 接口](https://github.com/ViudiraTech/Uinxed-Kernel/blob/master/include/arch/fpu.h)，本实现没有复制其代码。
