/*
 *
 *       src/sched/eevdf.rs
 *       Weighted virtual time, eligible deadlines and lag-preserving placement
 *
 *       2026/9/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! EEVDF service accounting; V = sum(w*v) / sum(w), eligibility v <= V.
//! See https://docs.kernel.org/scheduler/sched-eevdf.html and the weighted
//! virtual-time/placement derivations in Linux kernel/sched/fair.c.
//! Fixed requests retain their deadline across preemption and voluntary yields.

use super::rbtree::{Node, RBTree};
use alloc::boxed::Box;

pub const NICE_0_WEIGHT: u32 = 1024;
pub const SLICE_NS: u64 = 4_000_000;

pub struct RunQueue<V> {
    pub ready: RBTree<V>,
    pub minimum: i128,
}
impl<V> RunQueue<V> {
    pub const fn new() -> Self {
        Self {
            ready: RBTree::new(),
            minimum: 0,
        }
    }
    pub fn weight(&self, current: Option<&Node<V>>) -> u64 {
        self.ready.weight() + current.map_or(0, |n| u64::from(n.weight))
    }
    pub fn virtual_time(&self, current: Option<&Node<V>>) -> i128 {
        let weight = self.weight(current);
        if weight == 0 {
            return self.minimum;
        }
        let sum =
            self.ready.weighted_sum() + current.map_or(0, |n| n.vruntime * i128::from(n.weight));
        sum.div_euclid(i128::from(weight))
    }
    pub fn update_minimum(&mut self, current: Option<&Node<V>>) {
        let value = match (self.ready.minimum_vruntime(), current) {
            (Some(min), Some(n)) => min.min(n.vruntime),
            (Some(min), None) => min,
            (None, Some(n)) => n.vruntime,
            (None, None) => return,
        };
        self.minimum = self.minimum.max(value);
    }
    pub fn renew(node: &mut Node<V>) {
        node.remaining_ns = SLICE_NS;
        node.deadline = node.vruntime
            + i128::from(SLICE_NS) * i128::from(NICE_0_WEIGHT) / i128::from(node.weight);
        node.key.0 = node.deadline;
    }
    pub fn account(node: &mut Node<V>, delta_ns: u64) {
        let scaled = u128::from(delta_ns) * u128::from(NICE_0_WEIGHT) + u128::from(node.remainder);
        node.vruntime += (scaled / u128::from(node.weight)) as i128;
        node.remainder = (scaled % u128::from(node.weight)) as u64;
        node.remaining_ns = node.remaining_ns.saturating_sub(delta_ns);
        if node.remaining_ns == 0 {
            Self::renew(node);
        }
    }
    pub fn save_lag(&self, node: &mut Node<V>, other_current: Option<&Node<V>>) {
        // Include the departing entity even if the caller has detached current.
        let weight = self.weight(other_current) + u64::from(node.weight);
        let sum = self.ready.weighted_sum()
            + other_current.map_or(0, |n| n.vruntime * i128::from(n.weight))
            + node.vruntime * i128::from(node.weight);
        let bound = i128::from(2 * SLICE_NS) * i128::from(NICE_0_WEIGHT) / i128::from(node.weight);
        node.lag = (sum.div_euclid(i128::from(weight)) - node.vruntime).clamp(-bound, bound);
    }
    pub fn place(&self, node: &mut Node<V>, current: Option<&Node<V>>, fresh: bool) {
        let weight = self.weight(current);
        let virtual_time = self.virtual_time(current);
        let lag = if fresh || weight == 0 {
            0
        } else {
            // Adding w changes V; compensate by (W+w)/W to preserve V-v.
            node.lag * i128::from(weight + u64::from(node.weight)) / i128::from(weight)
        };
        let request_left = node.deadline - node.vruntime;
        node.vruntime = virtual_time - lag;
        if fresh || node.remaining_ns == 0 {
            Self::renew(node);
        } else {
            node.deadline = node.vruntime + request_left;
            node.key.0 = node.deadline;
        }
    }
    pub fn pick_next(&mut self) -> Option<Box<Node<V>>> {
        let virtual_time = self.virtual_time(None);
        // A weighted average always has at least one eligible member.
        let key = self.ready.eligible(virtual_time)?.key;
        self.ready.remove(key)
    }
}

pub fn nice_to_weight(nice: i32) -> u32 {
    const WEIGHTS: [u32; 40] = [
        88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916, 9548, 7620, 6100,
        4904, 3906, 3121, 2501, 1991, 1586, 1277, 1024, 820, 655, 526, 423, 335, 272, 215, 172,
        137, 110, 87, 70, 56, 45, 36, 29, 23, 18, 15,
    ];
    WEIGHTS[(nice.clamp(-20, 19) + 20) as usize]
}
