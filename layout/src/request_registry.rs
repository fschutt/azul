//! Requests that wait for the platform to answer, owned by the App: the picker that is on the screen, the export whose bytes wait
//! for the place the person chooses.
//!
//! A phone answers a request LATER and on another thread (an activity result, a delegate): the shell parks what it needs to
//! finish the request here under an id, hands the id to the platform (an Android request code, an iOS delegate's tag) and takes
//! the entry back when the answer arrives. The registry replaces the process-wide `static Mutex<BTreeMap>` and `AtomicU64` the
//! pickers used to keep: it is a field of `AppServices` (`pickers`), so two Apps in one process (a test, a companion) never share
//! an id, and the JNI / Objective-C entry points reach it through the one anchor the OS forces on them
//! (`process_app::services()`).
//!
//! What it parks is anything `Send`: the registry is type-erased, so the layout crate does not have to know the shells' request
//! types. A request is taken as the type it was registered as; asking for another type leaves it where it is.

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::any::Any;
use std::{
    collections::BTreeMap,
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

/// How many requests may wait at once: a person has one picker on the screen. Over the limit a request is refused and handed back
/// to the caller, who answers it with the reason (never a silent drop, never a crash).
pub const MAX_PENDING: usize = 16;

/// How long a request may wait before the shell may take it for lost (an activity that was destroyed never answers).
pub const LOST_AFTER: Duration = Duration::from_secs(60 * 60);

/// The largest id: an id travels as the low 16 bits of an Android request code (`startActivityForResult`), so the ids count up to this
/// and then start again at 1, skipping every id that still waits (at most [`MAX_PENDING`] do).
pub const MAX_ID: u64 = 0xFFFF;

struct Entry {
    value: Box<dyn Any + Send>,
    parked: Instant,
}

struct Inner {
    next: u64,
    entries: BTreeMap<u64, Entry>,
}

/// The parked requests of one App. Cloning gives another handle on the SAME registry.
#[derive(Clone)]
pub struct RequestRegistry {
    inner: Arc<Mutex<Inner>>,
}

impl Default for RequestRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                next: 1,
                entries: BTreeMap::new(),
            })),
        }
    }
}

impl core::fmt::Debug for RequestRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RequestRegistry").field("pending", &self.len()).finish()
    }
}

impl RequestRegistry {
    /// A registry of its own.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Park `value`; its id (1 to [`MAX_ID`], and never the id of a request that still waits), or the value back when [`MAX_PENDING`]
    /// requests wait.
    ///
    /// # Errors
    ///
    /// The value itself, when the registry is full.
    pub fn register<T: Any + Send>(&self, value: T) -> Result<u64, T> {
        let mut inner = self.lock();
        if inner.entries.len() >= MAX_PENDING {
            return Err(value);
        }
        // the next id that nobody waits on (there are at most MAX_PENDING waiting, so this is found at once)
        let mut id = inner.next;
        while inner.entries.contains_key(&id) {
            id = if id >= MAX_ID { 1 } else { id + 1 };
        }
        inner.next = if id >= MAX_ID { 1 } else { id + 1 };
        inner.entries.insert(
            id,
            Entry {
                value: Box::new(value),
                parked: Instant::now(),
            },
        );
        drop(inner);
        Ok(id)
    }

    /// Take the request `id` back, as the type it was parked as. `None` for an id nobody parked (an answer that comes twice, an
    /// answer for a request that was taken for lost) and for another type: that request stays parked.
    #[must_use]
    pub fn take<T: Any + Send>(&self, id: u64) -> Option<T> {
        let mut inner = self.lock();
        match inner.entries.remove(&id) {
            Some(entry) => match entry.value.downcast::<T>() {
                Ok(value) => Some(*value),
                Err(other) => {
                    // not the type asked for: put it back where it was, with its age
                    inner.entries.insert(
                        id,
                        Entry {
                            value: other,
                            parked: entry.parked,
                        },
                    );
                    None
                }
            },
            None => None,
        }
    }

    /// How many requests wait.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Whether no request waits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Take every request that waited longer than `older_than` out of the registry and hand them over (so the shell answers each
    /// with the reason instead of leaving the app waiting for ever).
    #[must_use]
    pub fn take_lost(&self, older_than: Duration) -> Vec<Box<dyn Any + Send>> {
        let mut inner = self.lock();
        let lost: Vec<u64> = inner
            .entries
            .iter()
            .filter(|(_, entry)| entry.parked.elapsed() > older_than)
            .map(|(id, _)| *id)
            .collect();
        let taken: Vec<Box<dyn Any + Send>> =
            lost.into_iter().filter_map(|id| inner.entries.remove(&id)).map(|entry| entry.value).collect();
        drop(inner);
        taken
    }
}

#[cfg(test)]
#[path = "request_registry_test.rs"]
mod request_registry_test;
