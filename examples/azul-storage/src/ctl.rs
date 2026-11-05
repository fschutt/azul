//! What a running transfer is told and what it asks: the bytes done so far, and whether
//! anyone wants it stopped (ARCH28 AR5a-8).
//!
//! The copy loops of this crate take a [`Ctl`] and look at it once per chunk: a transfer that is
//! cancelled makes nothing visible (a file copy removes its temporary file, a bucket upload is
//! aborted and its state forgotten) and answers [`crate::DriveError::Cancelled`] or
//! [`crate::durable::PublishError::Cancelled`]. A closure `Fn(u64) + Sync` that only hears the
//! progress is a `Ctl` that is never cancelled, so the callers that pass one need no change.

/// A transfer's two-way line to whoever started it.
pub trait Ctl: Sync {
    /// The bytes done so far (not the bytes since the last call), from whichever thread moved
    /// them. Never a smaller number than before from one transfer.
    fn bytes(&self, done: u64);

    /// Whether the transfer must stop at its next chunk.
    fn cancelled(&self) -> bool {
        false
    }
}

impl<F: Fn(u64) + Sync> Ctl for F {
    fn bytes(&self, done: u64) {
        self(done);
    }
}

/// A [`Ctl`] that hears nothing and never cancels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Quiet;

impl Ctl for Quiet {
    fn bytes(&self, _done: u64) {}
}

/// A [`Ctl`] that forwards the progress to `inner` and cancels when `stop` says so: how a caller
/// that holds a stop flag and a progress closure makes one `Ctl` of them.
pub struct WithStop<'a, S: Fn() -> bool + Sync> {
    inner: &'a dyn Ctl,
    stop: S,
}

impl<'a, S: Fn() -> bool + Sync> WithStop<'a, S> {
    pub fn new(inner: &'a dyn Ctl, stop: S) -> Self {
        WithStop { inner, stop }
    }
}

impl<S: Fn() -> bool + Sync> Ctl for WithStop<'_, S> {
    fn bytes(&self, done: u64) {
        self.inner.bytes(done);
    }

    fn cancelled(&self) -> bool {
        (self.stop)() || self.inner.cancelled()
    }
}
