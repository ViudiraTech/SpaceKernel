/*
 *
 *       src/fpu/mod.rs
 *       Eager, architecture-independent extended-register state ownership
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Eager, architecture-independent extended-register state ownership.
//!
//! Ordinary kernel code uses a soft-float ABI. All managed instructions belong
//! inside an explicit `with_kernel` section. Interrupt/NMI handlers must never
//! use FP/SIMD. The scheduler owns one `State` per task and must disable local
//! interrupts and preemption around save/restore/activate operations.

#[cfg(feature = "boot-self-test")]
mod self_test;
#[cfg(feature = "boot-self-test")]
pub(crate) use self_test::run as self_test;
#[cfg(feature = "boot-self-test")]
pub(crate) use self_test::{assert_registers as assert_test_registers, pattern as test_pattern};

use crate::{
    arch,
    cpuid::{CpuInfo, Feature},
    sync::SpinLock,
};
use alloc::alloc::{alloc_zeroed, dealloc};
use core::{alloc::Layout, marker::PhantomData, ptr::NonNull};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Unavailable,
    Fxsave,
    Xsave,
    FpSimd,
    RiscvF,
    RiscvD,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub format: Format,
    pub size: usize,
    pub alignment: usize,
    pub xcr0: u64,
    pub mxcsr_mask: u32,
}

impl Config {
    pub(crate) const fn unavailable() -> Self {
        Self {
            format: Format::Unavailable,
            size: 0,
            alignment: 64,
            xcr0: 0,
            mxcsr_mask: 0,
        }
    }

    /// OS-managed availability. The caller still needs an explicit scope;
    /// this query does not grant register ownership or enable the hardware.
    pub fn supports(self, feature: Feature, cpu: &CpuInfo) -> bool {
        if !cpu.features.contains(feature) {
            return false;
        }
        match feature {
            Feature::Fpu | Feature::DoublePrecision => self.format != Format::Unavailable,
            Feature::Mmx
            | Feature::Sse
            | Feature::Sse2
            | Feature::Sse3
            | Feature::Ssse3
            | Feature::Sse41
            | Feature::Sse42
            | Feature::Sse4a => matches!(self.format, Format::Fxsave | Format::Xsave),
            Feature::Avx | Feature::Avx2 | Feature::Fma | Feature::F16c => self.xcr0 & 6 == 6,
            Feature::Avx512f
            | Feature::Avx512dq
            | Feature::Avx512cd
            | Feature::Avx512bw
            | Feature::Avx512vl => self.xcr0 & 0xe6 == 0xe6,
            Feature::AdvSimd => self.format == Format::FpSimd,
            Feature::RiscvF => matches!(self.format, Format::RiscvF | Format::RiscvD),
            Feature::RiscvD => self.format == Format::RiscvD,
            Feature::Simd => matches!(self.format, Format::Fxsave | Format::Xsave | Format::FpSimd),
            Feature::Aes | Feature::Pclmul | Feature::Sha => match cpu.architecture {
                crate::cpuid::Architecture::X86_64 => {
                    matches!(self.format, Format::Fxsave | Format::Xsave)
                }
                crate::cpuid::Architecture::Aarch64 => self.format == Format::FpSimd,
                crate::cpuid::Architecture::Riscv64 => false,
            },
            Feature::Sve | Feature::RiscvV => false, // No variable-length state ABI yet.
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NotReady,
    Unsupported,
    OutOfMemory,
    InvalidState,
    IncompatibleCpu,
    AlreadyInitialized,
}

static CONFIG: SpinLock<Option<Config>> = SpinLock::new(None);
static INITIAL: SpinLock<Option<State>> = SpinLock::new(None);

/// Opaque, aligned task-owned state. Raw input can enter only through the
/// validating import API; callers cannot create an unchecked restore image.
pub struct State {
    pointer: NonNull<u8>,
    layout: Layout,
    config: Config,
}

// SAFETY: the allocation is exclusively owned; register access requires the
// caller's explicit CPU ownership contract. Moving an inactive image is safe.
unsafe impl Send for State {}

impl State {
    fn allocate(config: Config) -> Result<Self, Error> {
        if config.format == Format::Unavailable {
            return Err(Error::Unsupported);
        }
        let layout = Layout::from_size_align(config.size, config.alignment)
            .map_err(|_| Error::InvalidState)?;
        // SAFETY: validated nonzero layout; null is handled without dereferencing.
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(Error::OutOfMemory)?;
        Ok(Self {
            pointer,
            layout,
            config,
        })
    }

    /// Allocate a clean task state, before entering any critical section.
    pub fn new() -> Result<Self, Error> {
        let config = config().ok_or(Error::NotReady)?;
        let mut state = Self::allocate(config)?;
        state.reset()?;
        Ok(state)
    }

    /// Reset for exec or reuse. No previous task's register contents survive.
    pub fn reset(&mut self) -> Result<(), Error> {
        let template = INITIAL.lock();
        let initial = template.as_ref().ok_or(Error::NotReady)?;
        if self.config != initial.config {
            return Err(Error::InvalidState);
        }
        // SAFETY: independent owned allocations with identical checked layout.
        unsafe {
            core::ptr::copy_nonoverlapping(
                initial.pointer.as_ptr(),
                self.pointer.as_ptr(),
                self.layout.size(),
            )
        };
        Ok(())
    }

    /// Clone an inactive image for fork. Save an active parent before calling.
    pub fn try_clone(&self) -> Result<Self, Error> {
        let target = Self::allocate(self.config)?;
        // SAFETY: both buffers have identical layout and cannot overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.pointer.as_ptr(),
                target.pointer.as_ptr(),
                self.layout.size(),
            )
        };
        Ok(target)
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the live allocation is initialized and borrowed immutably.
        unsafe { core::slice::from_raw_parts(self.pointer.as_ptr(), self.layout.size()) }
    }

    /// Import an untrusted signal/ptrace image transactionally. Unsupported
    /// components and faulting restore metadata never reach hardware.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() != self.layout.size() || !arch::fpu::validate(self.config, bytes) {
            return Err(Error::InvalidState);
        }
        // SAFETY: Rust borrows exclude aliasing with self's owned allocation.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.pointer.as_ptr(), bytes.len())
        };
        Ok(())
    }

    fn check(&self) -> Result<Config, Error> {
        let current = config().ok_or(Error::NotReady)?;
        if self.config != current {
            return Err(Error::InvalidState);
        }
        Ok(current)
    }
}

impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: State exclusively owns this allocation and its original layout.
        unsafe {
            for index in 0..self.layout.size() {
                self.pointer.as_ptr().add(index).write_volatile(0);
            }
            dealloc(self.pointer.as_ptr(), self.layout);
        }
    }
}

pub fn config() -> Option<Config> {
    *CONFIG.lock()
}

/// Configure the BSP with interrupts masked, after the allocator is available.
pub fn init() -> Result<(), Error> {
    let mut published = CONFIG.lock();
    if published.is_some() {
        return Err(Error::AlreadyInitialized);
    }
    let cpu = crate::cpuid::info().ok_or(Error::NotReady)?;
    let result = arch::fpu::init(&cpu);
    arch::fpu::disable();
    let configured = result?;
    if configured.format != Format::Unavailable {
        let initial = State::allocate(configured)?;
        // SAFETY: a fresh, zeroed allocation with the backend's exact layout.
        unsafe {
            arch::fpu::initialize_image(configured, initial.pointer.as_ptr());
        }
        if !arch::fpu::validate(configured, initial.bytes()) {
            return Err(Error::InvalidState);
        }
        // SAFETY: the template is a validated image and BSP IRQs are masked.
        unsafe {
            let _ = arch::fpu::enable();
            arch::fpu::restore(configured, initial.pointer.as_ptr());
            arch::fpu::disable();
        }
        *INITIAL.lock() = Some(initial);
    }
    *published = Some(configured);
    Ok(())
}

/// Initialize an AP before publishing it online. Reject a different state ABI.
/// # Safety
/// Must execute on `cpu` with local IRQs/preemption disabled, before any task
/// can own FP registers. Startup must serialize this against BSP initialization.
pub unsafe fn init_current_cpu(cpu: &CpuInfo) -> Result<(), Error> {
    let expected = config().ok_or(Error::NotReady)?;
    let result = arch::fpu::init(cpu);
    arch::fpu::disable();
    let actual = result?;
    if expected != actual {
        return Err(Error::IncompatibleCpu);
    }
    if actual.format != Format::Unavailable {
        let initial = INITIAL.lock();
        let initial = initial.as_ref().ok_or(Error::NotReady)?;
        // SAFETY: caller owns this CPU; template is immutable and compatible.
        unsafe {
            let _ = arch::fpu::enable();
            arch::fpu::restore(actual, initial.pointer.as_ptr());
            arch::fpu::disable();
        }
    }
    Ok(())
}

/// Save the outgoing task and close the hardware gate before kernel work.
/// # Safety
/// IRQs and preemption are disabled; this CPU owns the outgoing task's state.
/// No active kernel FP section or NMI FP code may exist on this CPU.
pub unsafe fn save_current(state: &mut State) -> Result<(), Error> {
    let config = state.check()?;
    // SAFETY: the caller owns live registers and State owns the save area.
    unsafe {
        let _ = arch::fpu::enable();
        arch::fpu::save(config, state.pointer.as_ptr());
        arch::fpu::disable();
    }
    Ok(())
}

/// Restore an incoming task, keeping the hardware gate closed for kernel code.
/// # Safety
/// IRQs and preemption are disabled; save the previous owner first. The task
/// must not execute concurrently or migrate while its registers are live.
pub unsafe fn restore_current(state: &State) -> Result<(), Error> {
    let config = state.check()?;
    // SAFETY: import and internal templates ensure valid hardware metadata.
    unsafe {
        let _ = arch::fpu::enable();
        arch::fpu::restore(config, state.pointer.as_ptr());
        arch::fpu::disable();
    }
    Ok(())
}

/// Eager scheduler hook. Always save the outgoing owner, avoiding lazy-FPU
/// register disclosure. Hardware remains disabled until the user-return hook.
/// # Safety
/// Same CPU ownership requirements as save_current and restore_current.
pub unsafe fn switch(previous: &mut State, next: &State) -> Result<(), Error> {
    previous.check()?;
    next.check()?;
    // SAFETY: caller supplies exclusive scheduler ownership of both contexts.
    unsafe {
        save_current(previous)?;
        restore_current(next)
    }
}

/// Open the gate immediately before returning to the active user task.
/// # Safety
/// Only the currently restored task may execute with the gate open. All kernel
/// entry paths must close it, and scheduling must preserve that task first.
pub unsafe fn enable_user() -> Result<(), Error> {
    if config().ok_or(Error::NotReady)?.format == Format::Unavailable {
        return Err(Error::Unsupported);
    }
    // SAFETY: the caller implements the user-return ownership boundary.
    unsafe {
        let _ = arch::fpu::enable();
    }
    Ok(())
}

/// Close the gate at kernel entry without modifying the user's register file.
pub fn disable_current() {
    arch::fpu::disable();
}

struct KernelGuard<'a> {
    scratch: &'a mut State,
    gate: u64,
    irq_flags: u64,
    _cpu_local: PhantomData<*mut ()>,
}

impl Drop for KernelGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: the section owns the CPU until this guard restores its state.
        unsafe {
            let _ = arch::fpu::enable();
            arch::fpu::restore(self.scratch.config, self.scratch.pointer.as_ptr());
            arch::fpu::restore_gate(self.gate);
        }
        arch::irq_restore(self.irq_flags);
    }
}

/// Explicit kernel-FPU helper. Scratch is allocated by the caller beforehand;
/// nesting works with distinct scratch areas and preserves the outer scope.
/// # Safety
/// The closure must not enable IRQs, sleep, schedule, migrate or escape the
/// scope. It may use only instructions supported by Config::supports. NMI and
/// exception paths must use the soft-float ABI and never touch these registers.
pub unsafe fn with_kernel<T>(
    scratch: &mut State,
    operation: impl FnOnce() -> T,
) -> Result<T, Error> {
    let config = scratch.check()?;
    let flags = arch::irq_save();
    // SAFETY: interrupts are masked and the caller prevents migration/NMI use.
    let gate = unsafe { arch::fpu::enable() };
    // SAFETY: preserve the previous owner before giving clean state to kernel.
    unsafe {
        arch::fpu::save(config, scratch.pointer.as_ptr());
    }
    let guard = KernelGuard {
        scratch,
        gate,
        irq_flags: flags,
        _cpu_local: PhantomData,
    };
    {
        let initial = INITIAL.lock();
        // SAFETY: initialization precedes any State construction.
        unsafe {
            arch::fpu::restore(
                config,
                initial
                    .as_ref()
                    .expect("FPU template required")
                    .pointer
                    .as_ptr(),
            );
        }
    }
    let result = operation();
    drop(guard);
    Ok(result)
}
