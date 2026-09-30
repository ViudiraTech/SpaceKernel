/*
 *
 *       src/sched/task.rs
 *       Kernel task ownership, CPU affinity and checked stack lifetime
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Only kernel threads exist today. User processes, fork, signals and VFS
//! handles are deliberately absent until their resource ownership is implemented.

use crate::{arch, fpu, sync::SpinLock};
use alloc::{
    alloc::{alloc, dealloc},
    boxed::Box,
    sync::Arc,
};
use core::{
    alloc::Layout,
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};

/// Opaque ownership handle; callers can inspect metadata without borrowing the
/// scheduler-private context or changing task/queue identity.
#[derive(Clone)]
pub struct TaskRef(Arc<SpinLock<Task>>);
#[derive(Clone, Copy, Debug)]
pub struct TaskStats {
    pub tid: u64,
    pub state: TaskState,
    pub cpu: usize,
    pub affinity: CpuMask,
    pub runtime_ns: u64,
    pub switches: u64,
    pub voluntary_switches: u64,
    pub involuntary_switches: u64,
    pub migrations: u64,
    pub exit_code: i32,
}
impl TaskRef {
    pub(crate) fn lock(&self) -> crate::sync::Guard<'_, Task> {
        self.0.lock()
    }
    pub fn stats(&self) -> TaskStats {
        let task = self.lock();
        TaskStats {
            tid: task.tid,
            state: task.state,
            cpu: task.on_cpu,
            affinity: task.affinity,
            runtime_ns: task.runtime_ns,
            switches: task.switches,
            voluntary_switches: task.voluntary_switches,
            involuntary_switches: task.involuntary_switches,
            migrations: task.migrations,
            exit_code: task.exit_code,
        }
    }
}
const STACK_SIZE: usize = 64 * 1024;
const STACK_CANARY: u64 = 0x5350_4143_4553_544b;
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuMask([u64; 4]);
impl CpuMask {
    pub const fn all() -> Self {
        Self([u64::MAX; 4])
    }
    pub fn one(cpu: usize) -> Self {
        assert!(cpu < crate::smp::MAX_CPUS);
        let mut mask = Self([0; 4]);
        mask.0[cpu / 64] = 1 << (cpu % 64);
        mask
    }
    pub(crate) fn insert(&mut self, cpu: usize) {
        self.0[cpu / 64] |= 1 << (cpu % 64);
    }
    pub const fn empty() -> Self {
        Self([0; 4])
    }
    pub fn with_cpu(mut self, cpu: usize) -> Self {
        assert!(cpu < crate::smp::MAX_CPUS);
        self.insert(cpu);
        self
    }
    pub(crate) fn cpus(self) -> impl Iterator<Item = usize> {
        self.0
            .into_iter()
            .enumerate()
            .flat_map(|(group, mut bits)| {
                core::iter::from_fn(move || {
                    if bits == 0 {
                        return None;
                    }
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    Some(group * 64 + bit)
                })
            })
    }
    pub fn contains(self, cpu: usize) -> bool {
        cpu < 256 && self.0[cpu / 64] & (1 << (cpu % 64)) != 0
    }
}

pub(crate) struct Task {
    pub tid: u64,
    pub(crate) state: TaskState,
    pub(crate) on_cpu: usize,
    pub(crate) affinity: CpuMask,
    pub runtime_ns: u64,
    pub switches: u64,
    pub voluntary_switches: u64,
    pub involuntary_switches: u64,
    pub migrations: u64,
    pub exit_code: i32,
    pub(crate) context: UnsafeCell<arch::Context>,
    pub(crate) fpu_state: UnsafeCell<Option<fpu::State>>,
    pub(crate) entry: Option<Box<dyn FnOnce() + Send>>,
    pub(crate) stack: Option<KernelStack>,
}

pub(crate) struct KernelStack {
    base: *mut u8,
}
// SAFETY: stacks move only as inactive owned allocations; switch handoff keeps
// the source runqueue locked until the outgoing stack is no longer executing.
unsafe impl Send for KernelStack {}
impl KernelStack {
    fn new() -> Result<Self, &'static str> {
        let layout = Layout::from_size_align(STACK_SIZE, 16).unwrap();
        // SAFETY: valid layout, allocation failure is returned to the caller.
        let base = unsafe { alloc(layout) };
        if base.is_null() {
            return Err("kernel stack allocation failed");
        }
        // SAFETY: the bottom word is reserved exclusively for overflow checking.
        unsafe {
            (base as *mut u64).write(STACK_CANARY);
        }
        Ok(Self { base })
    }
    fn top(&self) -> usize {
        self.base as usize + STACK_SIZE
    }
    pub fn check(&self) {
        // SAFETY: allocation remains owned until the task is retired off-stack.
        assert_eq!(
            unsafe { (self.base as *const u64).read() },
            STACK_CANARY,
            "kernel stack overflow"
        );
    }
}
impl Drop for KernelStack {
    fn drop(&mut self) {
        self.check();
        // SAFETY: retirement runs on another stack; this is the original layout.
        unsafe {
            dealloc(self.base, Layout::from_size_align(STACK_SIZE, 16).unwrap());
        }
    }
}
impl Task {
    pub fn cpu(&self) -> usize {
        self.on_cpu
    }
    pub(crate) fn create(
        entry: Option<Box<dyn FnOnce() + Send>>,
        cpu: usize,
        affinity: CpuMask,
    ) -> Result<TaskRef, &'static str> {
        let idle = entry.is_none();
        let stack = if idle {
            None
        } else {
            Some(KernelStack::new()?)
        };
        let fpu_state = match fpu::State::new() {
            Ok(state) => Some(state),
            Err(fpu::Error::Unsupported) => None,
            Err(_) => return Err("task FPU allocation failed"),
        };
        let mut context = arch::Context::new();
        if let Some(stack) = &stack {
            // SAFETY: the exclusively owned stack is aligned and the no-argument
            // trampoline obtains its closure from this CPU's current task.
            unsafe {
                arch::context_init(&mut context, stack.top(), task_entry as *const () as usize);
            }
        }
        Ok(TaskRef(Arc::new(SpinLock::new(Self {
            tid: NEXT_TID.fetch_add(1, Ordering::Relaxed),
            state: TaskState::Ready,
            on_cpu: cpu,
            affinity,
            runtime_ns: 0,
            switches: 0,
            voluntary_switches: 0,
            involuntary_switches: 0,
            migrations: 0,
            exit_code: 0,
            context: UnsafeCell::new(context),
            fpu_state: UnsafeCell::new(fpu_state),
            entry,
            stack,
        }))))
    }
}
extern "C" fn task_entry() -> ! {
    super::finish_switch();
    let entry = super::current()
        .expect("task entry without current task")
        .lock()
        .entry
        .take()
        .expect("task entered twice");
    arch::enable_interrupts();
    entry();
    super::exit(0)
}
