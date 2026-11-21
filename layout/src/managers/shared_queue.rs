//! A queue that the owners holding a clone of it share.
//!
//! The platform answers later and on its own threads: an OS permission prompt, a
//! fingerprint sheet, a gamepad poll, a D-Bus signal. Whoever hands the platform
//! its callback clones a [`SharedQueue`] into it; the manager that owns the other
//! end drains the queue on the event-loop thread. Nothing looks the queue up by
//! name: two apps in one process, or two tests in one binary, each build their own
//! and never see each other's items.

use alloc::{sync::Arc, vec::Vec};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// A queue of `T` that every clone of it shares.
///
/// Pushing and draining take `&self` and never panic (a poisoned lock is
/// recovered: a push runs inside an OS callback, where unwinding would abort
/// the process).
///
/// Two queues compare EQUAL whatever they hold: a queue is the connection of a
/// manager to the thread that feeds it, not part of the manager's state, so a
/// manager that derives `PartialEq` compares its state alone.
pub struct SharedQueue<T> {
    items: Arc<Mutex<Vec<T>>>,
}

impl<T> SharedQueue<T> {
    /// A new, empty queue that no one else shares yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<T>> {
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queue `item` behind the ones already here.
    pub fn push(&self, item: T) {
        self.lock().push(item);
    }

    /// Queue `item` unless `bound` items already wait: then it is handed back.
    ///
    /// # Errors
    /// The `item` itself, when the queue is full.
    pub fn push_up_to(&self, item: T, bound: usize) -> Result<(), T> {
        let mut items = self.lock();
        if items.len() >= bound {
            return Err(item);
        }
        items.push(item);
        Ok(())
    }

    /// Queue `item` unless an equal item already waits, and unless `bound`
    /// items wait. `true` when it was queued.
    pub fn push_unique_up_to(&self, item: T, bound: usize) -> bool
    where
        T: PartialEq,
    {
        let mut items = self.lock();
        if items.len() >= bound || items.contains(&item) {
            return false;
        }
        items.push(item);
        true
    }

    /// Take every queued item, oldest first.
    pub fn drain(&self) -> Vec<T> {
        core::mem::take(&mut *self.lock())
    }

    /// Take the oldest `max` queued items, leaving the rest.
    pub fn drain_up_to(&self, max: usize) -> Vec<T> {
        let mut items = self.lock();
        let take = max.min(items.len());
        items.drain(..take).collect()
    }

    /// Take the queued items `wanted` answers `true` for, leaving the others
    /// queued in their order.
    pub fn drain_matching(&self, mut wanted: impl FnMut(&T) -> bool) -> Vec<T> {
        let mut items = self.lock();
        let (taken, left): (Vec<T>, Vec<T>) =
            core::mem::take(&mut *items).into_iter().partition(|item| wanted(item));
        *items = left;
        taken
    }

    /// Take the oldest queued item `wanted` answers `true` for, if any.
    pub fn take_first(&self, mut wanted: impl FnMut(&T) -> bool) -> Option<T> {
        let mut items = self.lock();
        let at = items.iter().position(|item| wanted(item))?;
        Some(items.remove(at))
    }

    /// Is nothing queued?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// How many items are queued.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Do `self` and `other` share one queue (are they clones of each other)?
    #[must_use]
    pub fn is_shared_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.items, &other.items)
    }
}

impl<T> Clone for SharedQueue<T> {
    /// Another handle on the SAME queue.
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
        }
    }
}

impl<T> Default for SharedQueue<T> {
    /// A new, empty queue of its own (not a handle on any other).
    fn default() -> Self {
        Self::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for SharedQueue<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `try_lock`: printing a window while this thread holds the queue must not deadlock.
        match self.items.try_lock() {
            Ok(items) => f.debug_tuple("SharedQueue").field(&*items).finish(),
            Err(_) => f.write_str("SharedQueue(<locked>)"),
        }
    }
}

impl<T> PartialEq for SharedQueue<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Eq for SharedQueue<T> {}

#[cfg(test)]
#[path = "shared_queue_test.rs"]
mod shared_queue_test;
