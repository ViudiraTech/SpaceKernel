/*
 *
 *       src/sched/loadbalance.rs
 *       Topology domains, bounded idle pulling and affinity-safe migration
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Per-CPU domains follow discovered SMT/core and package topology, then the
//! whole system. Unknown topology uses only the system domain. Periodic pulls
//! are staggered; idle CPUs pull immediately. Paired locks are acquired in CPU
//! order with try_lock, and at most four inactive tasks move per balance pass.
//! Reference: https://docs.kernel.org/scheduler/sched-domains.html

use super::{CpuMask, Scheduler, TaskState, request_reschedule};
use crate::{smp, time};
use core::sync::atomic::Ordering;

const INTERVAL_US: u64 = 20_000;
const CACHE_HOT_US: u64 = 2_000;
const SCAN_BUDGET: usize = 32;
const MIGRATION_BUDGET: usize = 4;

pub(super) struct Domains {
    masks: [CpuMask; 3],
    len: usize,
    siblings: CpuMask,
}
impl Domains {
    pub fn system(cpu: usize) -> Self {
        Self {
            masks: [CpuMask::all(); 3],
            len: 1,
            siblings: CpuMask::one(cpu),
        }
    }
}
pub(super) fn build_domains(sched: &Scheduler) {
    let mut system = CpuMask::empty();
    for cpu in 0..sched.cpus.len() {
        system.insert(cpu);
    }
    for cpu in 0..sched.cpus.len() {
        let mut domains = Domains {
            masks: [CpuMask::empty(); 3],
            len: 0,
            siblings: CpuMask::one(cpu),
        };
        if let Some(topology) = smp::cpu_info(cpu).and_then(|info| info.topology) {
            for core_only in [true, false] {
                let mut mask = CpuMask::empty();
                let mut count = 0;
                for other in 0..sched.cpus.len() {
                    if smp::cpu_info(other)
                        .and_then(|info| info.topology)
                        .is_some_and(|peer| {
                            peer.package_id == topology.package_id
                                && (!core_only || peer.core_id == topology.core_id)
                        })
                    {
                        mask.insert(other);
                        count += 1;
                    }
                }
                if core_only {
                    domains.siblings = mask;
                }
                if count > 1 && (domains.len == 0 || domains.masks[domains.len - 1] != mask) {
                    domains.masks[domains.len] = mask;
                    domains.len += 1;
                }
            }
        }
        if domains.len == 0 || domains.masks[domains.len - 1] != system {
            domains.masks[domains.len] = system;
            domains.len += 1;
        }
        *sched.cpus[cpu].domains.lock() = domains;
        // Offset first periodic pass across cores to avoid synchronized scans.
        sched.cpus[cpu].last_balance.store(
            time::uptime_micros().saturating_add(cpu as u64 * 500),
            Ordering::Relaxed,
        );
    }
}

fn locality(a: usize, b: usize) -> u8 {
    if a == b {
        return 0;
    }
    match (
        smp::cpu_info(a).and_then(|i| i.topology),
        smp::cpu_info(b).and_then(|i| i.topology),
    ) {
        (Some(x), Some(y)) if x.package_id == y.package_id => 1,
        _ => 2,
    }
}
pub(super) fn select_cpu(sched: &Scheduler, mask: CpuMask, previous: usize) -> Option<usize> {
    (0..sched.cpus.len())
        .filter(|&cpu| mask.contains(cpu) && smp::online(cpu))
        .min_by_key(|&cpu| {
            let nr = sched.cpus[cpu].runnable.load(Ordering::Acquire);
            let load = sched.cpus[cpu].load.load(Ordering::Relaxed);
            let siblings = sched.cpus[cpu].domains.lock().siblings;
            let shared_core_busy = siblings
                .cpus()
                .filter(|&peer| peer != cpu)
                .filter(|&peer| sched.cpus[peer].runnable.load(Ordering::Acquire) != 0)
                .count();
            (
                nr,
                shared_core_busy,
                load,
                locality(previous, cpu),
                usize::from(cpu != previous),
            )
        })
}

pub(super) fn balance(sched: &Scheduler, cpu: usize, idle: bool) {
    let now = time::uptime_micros();
    let last = sched.cpus[cpu].last_balance.load(Ordering::Relaxed);
    if !idle && now.saturating_sub(last) < INTERVAL_US {
        return;
    }
    sched.cpus[cpu].last_balance.store(now, Ordering::Relaxed);
    let mut moved = 0;
    let domain_count = sched.cpus[cpu].domains.lock().len;
    for domain in 0..domain_count {
        let mask = sched.cpus[cpu].domains.lock().masks[domain];
        let mine = sched.cpus[cpu].runnable.load(Ordering::Acquire);
        let donor = (0..sched.cpus.len())
            .filter(|&other| other != cpu && mask.contains(other) && smp::online(other))
            .filter(|&other| sched.cpus[other].runnable.load(Ordering::Acquire) > mine + 1)
            .max_by_key(|&other| {
                (
                    sched.cpus[other].runnable.load(Ordering::Acquire),
                    sched.cpus[other].load.load(Ordering::Relaxed),
                )
            });
        let Some(donor) = donor else {
            continue;
        };
        let (first, second) = (cpu.min(donor), cpu.max(donor));
        let Some(mut low) = sched.cpus[first].queue.try_lock() else {
            continue;
        };
        let Some(mut high) = sched.cpus[second].queue.try_lock() else {
            continue;
        };
        let (source, target) = if donor == first {
            (&mut *low, &mut *high)
        } else {
            (&mut *high, &mut *low)
        };
        source.account();
        target.account();
        while moved < MIGRATION_BUDGET {
            let source_count =
                source.fair.ready.len() + usize::from(source.fair_current().is_some());
            let target_count =
                target.fair.ready.len() + usize::from(target.fair_current().is_some());
            if source_count <= target_count + 1 {
                break;
            }
            let source_weight = source.fair.weight(source.fair_current());
            let target_weight = target.fair.weight(target.fair_current());
            let mut budget = SCAN_BUDGET;
            let key = source.fair.ready.find(&mut budget,&mut |node| {
                let task = node.value.lock();
                task.state == TaskState::Ready && task.affinity.contains(cpu)
                    && (idle || now.saturating_sub(node.last_run_us) >= CACHE_HOT_US)
                    // Avoid a weighted load oscillation when one heavy entity
                    // is more expensive than the entire imbalance.
                    && (target_count == 0 || u64::from(node.weight) < source_weight.saturating_sub(target_weight))
            });
            let Some(key) = key else {
                break;
            };
            let mut node = source.fair.ready.remove(key).unwrap();
            source.fair.save_lag(&mut node, source.fair_current());
            node.value.lock().migrations += 1;
            target.admit(node, false);
            moved += 1;
        }
        source.publish();
        target.publish();
        drop(high);
        drop(low);
        if moved != 0 {
            request_reschedule(cpu);
        }
        if moved >= MIGRATION_BUDGET {
            break;
        }
    }
}
