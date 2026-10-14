//! A* over any graph whose neighbours a closure hands out, on integer costs.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    hash::Hash,
};

/// What a search found: the nodes from start to goal and the cost, plus how
/// many nodes it expanded (for the stats line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<N> {
    pub path: Vec<N>,
    pub cost: u64,
    pub expanded: usize,
}

/// The cheapest path from `start` to `goal`. `successors(n)` hands out `n`'s
/// neighbours with the cost of the step to each; `estimate(n)` must never
/// exceed the true remaining cost (then the path found is the cheapest).
/// `stop()` is asked every few thousand expansions and ends the search with
/// `None` when it says so. `None` also when there is no path.
pub fn astar<N, S, I, E>(
    start: N,
    goal: N,
    mut successors: S,
    mut estimate: E,
    stop: &mut dyn FnMut() -> bool,
) -> Option<Found<N>>
where
    N: Copy + Eq + Hash + Ord,
    S: FnMut(N) -> I,
    I: IntoIterator<Item = (N, u64)>,
    E: FnMut(N) -> u64,
{
    // Best known cost to each node and the node it came from.
    let mut best: HashMap<N, (u64, Option<N>)> = HashMap::new();
    let mut open = BinaryHeap::new();
    best.insert(start, (0, None));
    open.push(Reverse((estimate(start), 0_u64, start)));
    let mut expanded = 0usize;
    while let Some(Reverse((_, cost, node))) = open.pop() {
        if best.get(&node).is_some_and(|(c, _)| *c < cost) {
            continue; // a stale entry: the node was reached cheaper since
        }
        if node == goal {
            let mut path = vec![goal];
            let mut at = goal;
            while let Some(&(_, Some(prev))) = best.get(&at) {
                path.push(prev);
                at = prev;
            }
            path.reverse();
            return Some(Found {
                path,
                cost,
                expanded,
            });
        }
        expanded += 1;
        if expanded % 4096 == 0 && stop() {
            return None;
        }
        for (next, step) in successors(node) {
            let through = cost.saturating_add(step);
            if best.get(&next).is_none_or(|(c, _)| through < *c) {
                best.insert(next, (through, Some(node)));
                open.push(Reverse((through.saturating_add(estimate(next)), through, next)));
            }
        }
    }
    None
}
