//! A count of the work an instance has done, kept in the instance and read from it.
//!
//! A test that guards the cost of a pass ("a hover that changes no style builds no cache", "a
//! parent of many children is walked once") needs the number of steps the pass took. It used to
//! read them from thread-locals that the pass bumped, which any other pass of the thread bumped
//! too; the struct that does the work keeps its own [`WorkCount`] now, and the test asks that
//! struct.

#[cfg(test)]
use core::sync::atomic::{AtomicU64, Ordering};

/// How many steps of one kind of work the struct that holds it has done.
///
/// The steps are counted only in azul-core's own test build, which is where the counts are read:
/// the passes they guard (the cascade's matching, the inherited-value store, the key
/// reconciliation) run millions of steps per layout, and a count that costs a locked
/// instruction (or a shared cache line written by every reader) per step would be paid by every
/// app for the sake of a test. In any other build a `WorkCount` holds nothing, counts nothing and
/// reads 0.
///
/// It is no part of what the struct IS: two structs equal in content are equal whatever their
/// counts (a derived `PartialEq` over a struct that holds one still compares only content), and a
/// clone starts at the count of what it copies. Atomic so that a `&self` method, and a struct
/// that is shared between threads, can count.
// Outside the test build it has no field and could be `Copy`; with the atomic it cannot, and a
// `Copy` count would silently fork.
#[allow(missing_copy_implementations)]
#[derive(Default)]
pub struct WorkCount {
    #[cfg(test)]
    count: AtomicU64,
}

// Without the test build there is nothing to count into: `self` and `steps` go unused, and
// every method is empty.
#[allow(clippy::unused_self, clippy::missing_const_for_fn)]
impl WorkCount {
    /// A count of nothing done.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            #[cfg(test)]
            count: AtomicU64::new(0),
        }
    }

    /// `steps` more steps done.
    #[inline]
    pub fn add(&self, steps: u64) {
        #[cfg(test)]
        {
            let _previous = self.count.fetch_add(steps, Ordering::Relaxed);
        }
        #[cfg(not(test))]
        {
            let _ = steps;
        }
    }

    /// One more step done.
    #[inline]
    pub fn bump(&self) {
        self.add(1);
    }

    /// The steps done so far.
    #[must_use]
    pub fn get(&self) -> u64 {
        #[cfg(test)]
        {
            self.count.load(Ordering::Relaxed)
        }
        #[cfg(not(test))]
        {
            0
        }
    }

    /// Back to nothing done: what a test calls before the pass it means to count.
    pub fn reset(&self) {
        #[cfg(test)]
        self.count.store(0, Ordering::Relaxed);
    }
}

impl Clone for WorkCount {
    fn clone(&self) -> Self {
        let copy = Self::new();
        copy.add(self.get());
        copy
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
