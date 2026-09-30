/*
 *
 *       src/sched/wait.rs
 *       Predicate-checked FIFO waits and race-free scheduler wakeup
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

use super::{Disposition, rbtree::RBTree};
use crate::{arch, smp, sync::SpinLock};
use alloc::sync::Arc;

pub struct WaitQueue {
    state: Arc<State>,
}
pub(super) struct State {
    pub queue: SpinLock<Queue>,
}
pub(super) struct Queue {
    pub tree: RBTree<super::TaskRef>,
    pub sequence: u64,
}
impl WaitQueue {
    pub fn new() -> Self {
        Self {
            state: Arc::new(State {
                queue: SpinLock::new(Queue {
                    tree: RBTree::new(),
                    sequence: 0,
                }),
            }),
        }
    }
    /// Predicate must not sleep or acquire scheduler locks. Publish the condition
    /// before wake_one/wake_all. Enrolment and predicate checking share the wait
    /// lock, so a wake racing the actual stack switch cannot be lost.
    pub fn wait_until(&self, ready: impl Fn() -> bool) {
        loop {
            let flags = arch::irq_save();
            let rq = super::scheduler().cpus[smp::current_cpu()].queue.lock();
            let queue = self.state.queue.lock();
            if ready() {
                drop(queue);
                drop(rq);
                arch::irq_restore(flags);
                return;
            }
            // Keep the predicate lock through waiter insertion inside dispatch.
            // dispatch releases it only after the entity is published.
            super::dispatch(rq, Disposition::Wait(queue), true);
            arch::irq_restore(flags);
        }
    }
    pub fn wake_one(&self) -> bool {
        let entity = self.state.queue.lock().tree.pop_first();
        if let Some(entity) = entity {
            super::wake_entity(entity);
            true
        } else {
            false
        }
    }
    pub fn wake_all(&self) -> usize {
        // Snapshot the waiters so a task that reblocks cannot prolong this call
        // or receive repeated wakeups in the same wake-all operation.
        let mut batch = {
            let mut queue = self.state.queue.lock();
            core::mem::replace(&mut queue.tree, RBTree::new())
        };
        let mut count = 0;
        while let Some(entity) = batch.pop_first() {
            super::wake_entity(entity);
            count += 1;
        }
        count
    }
    pub fn len(&self) -> usize {
        self.state.queue.lock().tree.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}
