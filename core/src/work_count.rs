//! A count of the work an instance has done, kept in the instance and read from it.
//!
//! A test that guards the cost of a pass ("a hover that changes no style builds no cache", "a
//! parent of many children is walked once") needs the number of steps the pass took. It used to
//! read them from thread-locals that the pass bumped, which any other pass of the thread bumped
//! too; the struct that does the work keeps its own [`WorkCount`] now, and the test asks that
//! struct.

use core::sync::atomic::{AtomicU64, Ordering};

/// How many steps of one kind of work the struct that holds it has done.
///
/// It is not part of what the struct IS: two structs equal in content are equal whatever their
/// counts (a derived `PartialEq` over a struct that holds one still compares only content), and
/// a clone starts at the count of what it copies. Atomic so that a `&self` method, and a struct
/// that is shared between threads, can count.
#[derive(Default)]
pub struct WorkCount {
    count: AtomicU64,
}

impl WorkCount {
    /// A count of nothing done.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }

    /// `steps` more steps done.
    pub fn add(&self, steps: u64) {
        self.count.fetch_add(steps, Ordering::Relaxed);
    }

    /// One more step done.
    pub fn bump(&self) {
        self.add(1);
    }

    /// The steps done so far.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Back to nothing done: what a test calls before the pass it means to count.
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }
}

impl Clone for WorkCount {
    fn clone(&self) -> Self {
        Self {
            count: AtomicU64::new(self.get()),
        }
    }
}

impl PartialEq for WorkCount {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for WorkCount {}

impl core::fmt::Debug for WorkCount {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "WorkCount({})", self.get())
    }
}

#[cfg(test)]
#[path = "work_count_test.rs"]
mod work_count_test;
