/*
 *
 *       src/sched/mod.rs
 *       CPU-local preemptive scheduling and same-CPU switch lock handoff
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Lock order: ascending CPU runqueues -> wait/timer queue -> task metadata.
//! Wakers detach under the wait/timer lock, release it, then lock the owning
//! runqueue. A switch transfers that runqueue lock to the incoming stack. Thus
//! no CPU can migrate, wake or free the outgoing task before its registers and
//! stack pointer are saved. No ordinary spinlock guard survives a switch.

mod eevdf;
mod loadbalance;
#[cfg(feature = "boot-self-test")]
mod model_tests;
mod rbtree;
#[cfg(feature = "boot-self-test")]
pub mod sched_test;
mod task;
mod wait;

use crate::{
    arch, boot, fpu, smp,
    sync::{Guard, SpinLock},
    time,
};
use alloc::{boxed::Box, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
pub use eevdf::{NICE_0_WEIGHT, nice_to_weight};
use rbtree::{Node, RBTree};
use task::Task;
pub use task::{CpuMask, TaskRef, TaskState, TaskStats};
pub use wait::WaitQueue;

type Entity = Box<Node<TaskRef>>;
static SCHEDULER: AtomicPtr<Scheduler> = AtomicPtr::new(core::ptr::null_mut());
static TIMERS: SpinLock<RBTree<TaskRef>> = SpinLock::new(RBTree::new());
static COMPLETED: AtomicU64 = AtomicU64::new(0);

struct Scheduler {
    cpus: Vec<Cpu>,
}
struct Cpu {
    queue: SpinLock<CpuQueue>,
    reschedule: AtomicBool,
    active: AtomicBool,
    ticks: AtomicU64,
    load: AtomicU64,
    runnable: AtomicUsize,
    last_balance: AtomicU64,
    domains: SpinLock<loadbalance::Domains>,
}
struct CpuQueue {
    fair: eevdf::RunQueue<TaskRef>,
    current: Option<Entity>,
    idle: Option<Entity>,
    retired: Option<Entity>,
    clock_us: u64,
    cpu: usize,
}
impl CpuQueue {
    fn fair_current(&self) -> Option<&Node<TaskRef>> {
        self.current.as_deref().filter(|node| node.weight != 0)
    }
    fn account(&mut self) -> bool {
        let now = time::uptime_micros();
        let elapsed = now.saturating_sub(self.clock_us).saturating_mul(1000);
        self.clock_us = now;
        let mut expired = false;
        if let Some(node) = self.current.as_mut() {
            if node.weight != 0 {
                expired = elapsed >= node.remaining_ns;
                eevdf::RunQueue::account(node, elapsed);
                let mut task = node.value.lock();
                task.runtime_ns = task.runtime_ns.saturating_add(elapsed);
            }
            node.last_run_us = now;
        }
        let current = self.current.as_deref().filter(|n| n.weight != 0);
        self.fair.update_minimum(current);
        expired
    }
    fn publish(&self) {
        let cpu = &scheduler().cpus[self.cpu];
        cpu.load
            .store(self.fair.weight(self.fair_current()), Ordering::Relaxed);
        cpu.runnable.store(
            self.fair.ready.len() + usize::from(self.fair_current().is_some()),
            Ordering::Release,
        );
    }
    fn admit(&mut self, mut entity: Entity, fresh: bool) {
        self.account();
        self.fair.place(&mut entity, self.fair_current(), fresh);
        {
            let mut task = entity.value.lock();
            task.state = TaskState::Ready;
            task.on_cpu = self.cpu;
        }
        self.fair.ready.insert(entity);
        self.publish();
    }
}
fn try_scheduler() -> Option<&'static Scheduler> {
    let pointer = SCHEDULER.load(Ordering::Acquire);
    // SAFETY: init publishes a complete permanent Box exactly once.
    if pointer.is_null() {
        None
    } else {
        Some(unsafe { &*pointer })
    }
}
fn scheduler() -> &'static Scheduler {
    try_scheduler().expect("scheduler not initialized")
}

pub fn init(num_cpus: usize) {
    assert!((1..=smp::MAX_CPUS).contains(&num_cpus));
    assert!(
        SCHEDULER.load(Ordering::Acquire).is_null(),
        "scheduler initialized twice"
    );
    assert!(
        time::counter_frequency().is_some(),
        "scheduler requires a monotonic clock"
    );
    let mut cpus = Vec::with_capacity(num_cpus);
    for cpu in 0..num_cpus {
        let task = Task::create(None, cpu, CpuMask::one(cpu)).expect("idle task allocation failed");
        let tid = task.lock().tid;
        cpus.push(Cpu {
            queue: SpinLock::new(CpuQueue {
                fair: eevdf::RunQueue::new(),
                current: None,
                idle: Some(Node::new(tid, task, 0)),
                retired: None,
                clock_us: 0,
                cpu,
            }),
            reschedule: AtomicBool::new(false),
            active: AtomicBool::new(false),
            ticks: AtomicU64::new(0),
            load: AtomicU64::new(0),
            runnable: AtomicUsize::new(0),
            last_balance: AtomicU64::new(0),
            domains: SpinLock::new(loadbalance::Domains::system(cpu)),
        });
    }
    let pointer = Box::into_raw(Box::new(Scheduler { cpus }));
    SCHEDULER.store(pointer, Ordering::Release);
    arch::register_scheduler_irqs().expect("scheduler IRQ registration failed");
}

pub(crate) fn activate_cpu(cpu: usize) {
    let mut rq = scheduler().cpus[cpu].queue.lock();
    assert!(rq.current.is_none());
    let idle = rq.idle.take().unwrap();
    idle.value.lock().state = TaskState::Running;
    rq.current = Some(idle);
    rq.clock_us = time::uptime_micros();
    rq.publish();
}
pub(crate) fn build_domains() {
    loadbalance::build_domains(scheduler());
}
pub(crate) fn start_current_cpu() {
    let cpu = smp::current_cpu();
    scheduler().cpus[cpu].active.store(true, Ordering::Release);
    arch::arm_scheduler_tick().expect("scheduler timer arm failed");
    arch::enable_interrupts();
}

/// Spawn an owned, Send kernel closure; its stack and queue node are allocated
/// before acquiring any scheduler lock. Nice is clamped to [-20, 19].
pub fn spawn(entry: impl FnOnce() + Send + 'static, nice: i32) -> Result<u64, &'static str> {
    spawn_with_affinity(entry, nice, CpuMask::all())
}
pub fn spawn_on(
    cpu: usize,
    entry: impl FnOnce() + Send + 'static,
    nice: i32,
) -> Result<u64, &'static str> {
    if cpu >= boot::cpu_count() {
        return Err("invalid CPU");
    }
    spawn_with_affinity(entry, nice, CpuMask::one(cpu))
}
pub fn spawn_with_affinity(
    entry: impl FnOnce() + Send + 'static,
    nice: i32,
    affinity: CpuMask,
) -> Result<u64, &'static str> {
    let sched = try_scheduler().ok_or("scheduler not initialized")?;
    let cpu = loadbalance::select_cpu(sched, affinity, smp::current_cpu())
        .ok_or("no online CPU in affinity")?;
    let task = Task::create(Some(Box::new(entry)), cpu, affinity)?;
    let tid = task.lock().tid;
    let entity = Node::new(tid, task, nice_to_weight(nice));
    sched.cpus[cpu].queue.lock().admit(entity, true);
    request_reschedule(cpu);
    Ok(tid)
}

/// Obtain an ownership handle for this task with a consistent CPU-local lookup.
/// TaskRef::stats returns a snapshot without exposing context/FPU ownership.
pub fn current() -> Option<TaskRef> {
    let sched = try_scheduler()?;
    let flags = arch::irq_save();
    let task = sched.cpus[smp::current_cpu()]
        .queue
        .lock()
        .current
        .as_ref()
        .map(|node| node.value.clone());
    arch::irq_restore(flags);
    task
}

pub fn cpu_count() -> usize {
    scheduler().cpus.len()
}
pub fn cpu_ticks(cpu: usize) -> u64 {
    scheduler().cpus[cpu].ticks.load(Ordering::Relaxed)
}
pub fn completed_count() -> u64 {
    COMPLETED.load(Ordering::Acquire)
}
pub fn set_need_resched() {
    let flags = arch::irq_save();
    request_reschedule(smp::current_cpu());
    arch::irq_restore(flags);
}
pub fn need_resched() -> bool {
    let flags = arch::irq_save();
    let needed = try_scheduler().is_some_and(|sched| {
        sched.cpus[smp::current_cpu()]
            .reschedule
            .load(Ordering::Acquire)
    });
    arch::irq_restore(flags);
    needed
}

fn request_reschedule(cpu: usize) {
    let sched = scheduler();
    sched.cpus[cpu].reschedule.store(true, Ordering::Release);
    if cpu != smp::current_cpu() && smp::online(cpu) {
        arch::send_reschedule(cpu).expect("reschedule IPI failed");
    }
}
pub fn yield_now() {
    let flags = arch::irq_save();
    let cpu = smp::current_cpu();
    loadbalance::balance(scheduler(), cpu, false);
    let rq = scheduler().cpus[cpu].queue.lock();
    dispatch(rq, Disposition::Runnable, true);
    arch::irq_restore(flags);
}
pub fn schedule() {
    yield_now();
}

pub fn sleep_micros(micros: u64) {
    if micros == 0 {
        yield_now();
        return;
    }
    let flags = arch::irq_save();
    let cpu = smp::current_cpu();
    let rq = scheduler().cpus[cpu].queue.lock();
    let wake = time::uptime_micros().saturating_add(micros);
    dispatch(rq, Disposition::Sleep(wake), true);
    arch::irq_restore(flags);
}
pub fn exit(exit_code: i32) -> ! {
    arch::disable_interrupts();
    let rq = scheduler().cpus[smp::current_cpu()].queue.lock();
    dispatch(rq, Disposition::Exit(exit_code), true);
    panic!("dead task resumed")
}

enum Disposition<'a> {
    Runnable,
    Sleep(u64),
    Wait(Guard<'a, wait::Queue>),
    Exit(i32),
}

fn dispatch(mut rq: Guard<'static, CpuQueue>, disposition: Disposition<'_>, voluntary: bool) {
    let cpu = rq.cpu;
    rq.account();
    let mut previous = rq.current.take().expect("CPU not activated");
    let previous_tid = previous.key.1;
    let previous_context;
    let previous_fpu;
    {
        let mut task = previous.value.lock();
        if let Some(stack) = &task.stack {
            stack.check();
        }
        previous_context = task.context.get();
        previous_fpu = task.fpu_state.get();
        if voluntary {
            task.voluntary_switches += 1;
        } else {
            task.involuntary_switches += 1;
        }
    }
    match disposition {
        Disposition::Runnable if previous.weight == 0 => rq.idle = Some(previous),
        Disposition::Runnable => {
            previous.value.lock().state = TaskState::Ready;
            previous.key.0 = previous.deadline;
            rq.fair.ready.insert(previous);
        }
        Disposition::Sleep(wake) => {
            assert_ne!(previous.weight, 0, "idle cannot sleep");
            rq.fair.save_lag(&mut previous, None);
            previous.value.lock().state = TaskState::Blocked;
            previous.key.0 = i128::from(wake);
            TIMERS.lock().insert(previous);
        }
        Disposition::Wait(mut queue) => {
            assert_ne!(previous.weight, 0, "idle cannot wait");
            rq.fair.save_lag(&mut previous, None);
            previous.value.lock().state = TaskState::Blocked;
            previous.key.0 = i128::from(queue.sequence);
            queue.sequence = queue
                .sequence
                .checked_add(1)
                .expect("wait sequence overflow");
            queue.tree.insert(previous);
        }
        Disposition::Exit(code) => {
            assert_ne!(previous.weight, 0, "idle cannot exit");
            let mut task = previous.value.lock();
            task.state = TaskState::Dead;
            task.exit_code = code;
            drop(task);
            assert!(rq.retired.is_none());
            rq.retired = Some(previous);
        }
    }
    let next = rq
        .fair
        .pick_next()
        .or_else(|| rq.idle.take())
        .expect("missing idle context");
    let next_tid = next.key.1;
    let next_context;
    let next_fpu;
    {
        let mut task = next.value.lock();
        task.state = TaskState::Running;
        task.on_cpu = cpu;
        if previous_tid != next_tid {
            task.switches += 1;
        }
        next_context = task.context.get().cast_const();
        next_fpu = task.fpu_state.get().cast_const();
    }
    rq.current = Some(next);
    rq.publish();
    scheduler().cpus[cpu]
        .reschedule
        .store(false, Ordering::Release);
    if previous_tid == next_tid {
        return;
    }

    // SAFETY: queue/wait/retirement ownership pins both tasks. This runqueue
    // excludes migration and wakeup until the outgoing state is saved. These
    // private fields cannot be accessed through public task metadata handles.
    unsafe {
        if let (Some(previous), Some(next)) = ((&mut *previous_fpu).as_mut(), (&*next_fpu).as_ref())
        {
            fpu::switch(previous, next).expect("task FPU switch failed");
        }
    }
    // SAFETY: this transfers a lock, not an IRQ-restoring Guard. The new or
    // resumed task calls finish_switch on this CPU before it can be preempted.
    unsafe {
        rq.handoff();
        arch::switch(previous_context, next_context);
    }
    finish_switch();
}

pub(crate) fn finish_switch() {
    let cpu = smp::current_cpu();
    let queue = &scheduler().cpus[cpu].queue;
    // SAFETY: every arch::switch continuation and first-entry trampoline owns
    // exactly the handoff on the CPU where it is now executing.
    unsafe {
        queue.unlock_handoff();
    }
    let retired = queue.lock().retired.take();
    if let Some(retired) = retired {
        drop(retired); // Never free the stack while it is still executing.
        COMPLETED.fetch_add(1, Ordering::Release);
    }
}

pub(crate) fn wake_entity(entity: Entity) {
    let (source, affinity) = {
        let task = entity.value.lock();
        (task.on_cpu, task.affinity)
    };
    // Synchronize with the outgoing stack save before considering a new CPU.
    // The detached node is exclusively owned here, so after this barrier it
    // cannot run or be migrated until admission. No paired locks are needed.
    {
        let _source = scheduler().cpus[source].queue.lock();
        assert_eq!(entity.value.lock().state, TaskState::Blocked);
    }
    let cpu = loadbalance::select_cpu(scheduler(), affinity, source)
        .expect("blocked task has no online CPU");
    if cpu != source {
        entity.value.lock().migrations += 1;
    }
    scheduler().cpus[cpu].queue.lock().admit(entity, false);
    request_reschedule(cpu);
}

pub fn timer_interrupt(_irq: u32) {
    arch::arm_scheduler_tick().expect("scheduler tick rearm failed");
    tick();
}
pub fn reschedule_interrupt(_irq: u32) {
    scheduler().cpus[smp::current_cpu()]
        .reschedule
        .store(true, Ordering::Release);
}
pub fn tick() {
    let sched = scheduler();
    let cpu = smp::current_cpu();
    if !sched.cpus[cpu].active.load(Ordering::Acquire) {
        return;
    }
    sched.cpus[cpu].ticks.fetch_add(1, Ordering::Relaxed);
    {
        let mut rq = sched.cpus[cpu].queue.lock();
        let expired = rq.account();
        let virtual_time = rq.fair.virtual_time(rq.fair_current());
        if let Some(candidate) = rq.fair.ready.eligible(virtual_time) {
            if expired
                || rq.fair_current().is_none_or(|current| {
                    current.vruntime > virtual_time || candidate.deadline < current.deadline
                })
            {
                sched.cpus[cpu].reschedule.store(true, Ordering::Release);
            }
        }
    }
    let now = time::uptime_micros();
    for _ in 0..64 {
        let entity = {
            let mut timers = TIMERS.lock();
            if timers
                .first()
                .is_none_or(|node| node.key.0 > i128::from(now))
            {
                None
            } else {
                timers.pop_first()
            }
        };
        let Some(entity) = entity else {
            break;
        };
        wake_entity(entity);
    }
}

/// Called by architecture IRQ exits after EOI/complete and trap-frame capture.
pub(crate) fn irq_exit() {
    let Some(sched) = try_scheduler() else {
        return;
    };
    let cpu = smp::current_cpu();
    if !sched.cpus[cpu].active.load(Ordering::Acquire) {
        return;
    }
    loadbalance::balance(sched, cpu, false);
    if sched.cpus[cpu].reschedule.load(Ordering::Acquire) {
        let rq = sched.cpus[cpu].queue.lock();
        dispatch(rq, Disposition::Runnable, false);
    }
}
pub(crate) fn idle_loop() -> ! {
    loop {
        arch::disable_interrupts();
        let cpu = smp::current_cpu();
        loadbalance::balance(scheduler(), cpu, true);
        if need_resched() {
            let rq = scheduler().cpus[cpu].queue.lock();
            dispatch(rq, Disposition::Runnable, false);
        }
        arch::idle_wait();
    }
}

#[cfg(feature = "boot-self-test")]
fn spawn_placed(
    cpu: usize,
    entry: impl FnOnce() + Send + 'static,
    nice: i32,
    affinity: CpuMask,
) -> Result<u64, &'static str> {
    let task = Task::create(Some(Box::new(entry)), cpu, affinity)?;
    let tid = task.lock().tid;
    scheduler().cpus[cpu]
        .queue
        .lock()
        .admit(Node::new(tid, task, nice_to_weight(nice)), true);
    request_reschedule(cpu);
    Ok(tid)
}
