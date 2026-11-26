//! The microphone kept open between calls. Opening one takes about 430 ms on a Mac (the device
//! 160 ms, its first sound 270 ms) and what is said meanwhile is lost: with a call after a call,
//! the first words of the second were gone. A kept microphone is open when the next call comes, and
//! the sound that came in between (up to [`PREROLL_MS`], the newest) goes to that call first, so
//! speech that began before the call began is not cut. It is closed after [`MicKeeper::keep_ms`]
//! without a call, at once on a release, when it breaks, when the call that had it ends badly or
//! is cancelled, and when the last handle of its owner goes. While it is open the system shows its
//! microphone indicator.
//!
//! One thread per keeper reads the kept microphone (short reads, so a call that wants it waits
//! for one at most), and ends with the keeper (it holds a weak reference). The microphone is the
//! session's, shared by the recognizers that listen with it. std only.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak,
    },
    time::{Duration, Instant},
};

use super::{
    builtin::{Microphone, MicrophoneSource},
    recognizer::Outcome,
};

/// The most sound kept for the next call while nobody listens, ms.
pub const PREROLL_MS: u32 = 2_000;

/// The longest a microphone may be kept after a call, ms.
pub const MAX_KEEP_MS: u32 = 60_000;

/// The longest an armed microphone may wait without an arm or a call, ms.
pub const MAX_ARMED_IDLE_MS: u32 = 600_000;

/// How long a session keeps its microphone open after a call unless the app says otherwise, ms.
pub const DEFAULT_KEEP_MS: u32 = 5_000;

/// How long an armed microphone waits without an arm or a call unless the app says otherwise, ms.
pub const DEFAULT_ARMED_IDLE_MS: u32 = 60_000;

/// How long one read of the kept microphone waits for sound, ms: the longest a call that wants the
/// microphone can wait for the thread that reads it.
const PUMP_WAIT_MS: u32 = 10;

/// The least one turn of the thread takes: a microphone that does not block must not make it spin.
const PACE: Duration = Duration::from_millis(5);

/// How often an idle thread looks whether its keeper is still there.
const IDLE_CHECK: Duration = Duration::from_millis(250);

/// A microphone that waits between two calls.
struct Held {
    microphone: Box<dyn Microphone>,
    /// What came in since the last call, oldest first, at most [`PREROLL_MS`].
    preroll: Vec<f32>,
    /// When the last call let go of it, or the app armed it last.
    idle_since: Instant,
    /// The app armed it: it waits for the longer time of an armed microphone, not the short one of a call.
    armed: bool,
}

impl Held {
    fn append(&mut self, chunk: &[f32]) {
        self.preroll.extend_from_slice(chunk);
        let cap = (u64::from(self.microphone.rate()) * u64::from(PREROLL_MS) / 1000) as usize;
        if self.preroll.len() > cap {
            let surplus = self.preroll.len() - cap;
            self.preroll.drain(..surplus);
        }
    }
}

struct Shared {
    source: Mutex<Box<dyn MicrophoneSource>>,
    held: Mutex<Option<Held>>,
    changed: Condvar,
    keep_ms: AtomicU32,
    /// How long an armed microphone waits without an arm or a call, ms (0: an armed microphone
    /// waits like any other).
    armed_idle_ms: AtomicU32,
    /// The microphone stays open without a limit once it is open.
    always: AtomicBool,
    /// The app armed the microphone while a call had it: the call gives it back armed.
    arm_pending: AtomicBool,
    /// How many calls have the microphone.
    leased: AtomicU32,
    /// The thread that reads the kept microphone was started.
    started: AtomicBool,
    /// How many calls wait for the lock of the kept microphone: the thread that reads it lets them
    /// in (a lock is not fair, and a thread that takes it again at once starved a call for seconds).
    waiting: AtomicU32,
}

impl Shared {
    /// How long `held` may wait, `None` for no limit.
    fn limit_of(&self, held: &Held) -> Option<Duration> {
        if self.always.load(Ordering::SeqCst) {
            return None;
        }
        let armed_idle = self.armed_idle_ms.load(Ordering::SeqCst);
        let ms = if held.armed && armed_idle > 0 { armed_idle } else { self.keep_ms.load(Ordering::SeqCst) };
        Some(Duration::from_millis(u64::from(ms)))
    }

    /// Whether the standby keeps nothing: no time after a call, no armed time, not always.
    fn is_off(&self) -> bool {
        self.keep_ms.load(Ordering::SeqCst) == 0
            && self.armed_idle_ms.load(Ordering::SeqCst) == 0
            && !self.always.load(Ordering::SeqCst)
    }

    /// The kept microphone, for a call: it goes before the thread that reads it.
    fn held(&self) -> MutexGuard<'_, Option<Held>> {
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let guard = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        guard
    }

    /// The kept microphone, for the thread that reads it.
    fn held_by_thread(&self) -> MutexGuard<'_, Option<Held>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes what is held out (to be closed by the caller after letting go of the lock).
    fn take_held(&self) -> Option<Held> {
        self.held().take()
    }

    fn hold(self: &Arc<Self>, microphone: Box<dyn Microphone>, preroll: Vec<f32>, armed: bool) {
        *self.held() = Some(Held {
            microphone,
            preroll,
            idle_since: Instant::now(),
            armed,
        });
        if !self.started.swap(true, Ordering::SeqCst) {
            let weak = Arc::downgrade(self);
            let spawned = std::thread::Builder::new()
                .name("azul-speech-microphone".to_string())
                .spawn(move || keep_loop(&weak));
            if spawned.is_err() {
                // Nobody would read it: do not keep what cannot be kept.
                self.started.store(false, Ordering::SeqCst);
                drop(self.take_held());
                return;
            }
        }
        self.changed.notify_all();
    }
}

/// The thread of a keeper: reads the kept microphone into its pre-roll, closes it after its time
/// and when it breaks, and ends when the keeper is gone.
fn keep_loop(weak: &Weak<Shared>) {
    loop {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let mut held = shared.held_by_thread();
        let Some(kept) = held.as_mut() else {
            // Nothing to keep: wait for a call to give a microphone back (or for the keeper to go).
            let _ = shared.changed.wait_timeout(held, IDLE_CHECK).unwrap_or_else(PoisonError::into_inner);
            continue;
        };
        if shared.limit_of(kept).is_some_and(|limit| limit.is_zero() || kept.idle_since.elapsed() >= limit) {
            let old = held.take();
            drop(held);
            drop(old);
            continue;
        }
        let began = Instant::now();
        let mut chunk = Vec::new();
        match kept.microphone.read(&mut chunk, PUMP_WAIT_MS) {
            Ok(()) => kept.append(&chunk),
            Err(_) => {
                // The device went away: the next call opens another.
                let old = held.take();
                drop(held);
                drop(old);
                continue;
            }
        }
        drop(held);
        // Out of the lock for a moment, longer when a call is waiting for it: a thread that locks
        // again at once keeps it from the call that wants it.
        let mut rest = PACE.saturating_sub(began.elapsed());
        if shared.waiting.load(Ordering::SeqCst) > 0 {
            rest = rest.max(PACE);
        }
        drop(shared);
        if !rest.is_zero() {
            std::thread::sleep(rest);
        }
    }
}

/// The session's microphone: opens it for a call, and keeps it for the next one.
#[derive(Clone)]
pub struct MicKeeper {
    shared: Arc<Shared>,
}

impl MicKeeper {
    /// A keeper that opens microphones with `source` and keeps none (see [`Self::set_keep_ms`]).
    #[must_use]
    pub fn new(source: Box<dyn MicrophoneSource>) -> Self {
        Self {
            shared: Arc::new(Shared {
                source: Mutex::new(source),
                held: Mutex::new(None),
                changed: Condvar::new(),
                keep_ms: AtomicU32::new(0),
                armed_idle_ms: AtomicU32::new(0),
                always: AtomicBool::new(false),
                arm_pending: AtomicBool::new(false),
                leased: AtomicU32::new(0),
                started: AtomicBool::new(false),
                waiting: AtomicU32::new(0),
            }),
        }
    }

    /// A keeper whose source has no microphone: a scripted run, where nothing real is opened.
    #[must_use]
    pub fn without_microphone() -> Self {
        struct None;
        impl MicrophoneSource for None {
            fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
                Err((Outcome::Unavailable, "this session has no microphone".to_string()))
            }
        }
        Self::new(Box::new(None))
    }

    /// How long a microphone stays open after a call, ms: kept to [`MAX_KEEP_MS`]; zero closes it
    /// at the end of every call, and closes one that is kept now.
    pub fn set_keep_ms(&self, ms: u32) {
        let ms = ms.min(MAX_KEEP_MS);
        self.shared.keep_ms.store(ms, Ordering::SeqCst);
        if ms == 0 && self.shared.is_off() {
            self.release();
        }
        self.shared.changed.notify_all();
    }

    /// How long an armed microphone waits without an arm or a call before it is given up, ms (kept
    /// to ten minutes; 0: an armed microphone waits like any other). A forgotten disarm cannot
    /// leave the microphone on for ever.
    pub fn set_armed_idle_ms(&self, ms: u32) {
        self.shared.armed_idle_ms.store(ms.min(MAX_ARMED_IDLE_MS), Ordering::SeqCst);
        self.shared.changed.notify_all();
    }

    /// Whether the microphone stays open without a limit once it is open (an explicit opt-in: the
    /// system's microphone indicator stays on). Only a disarm, a reset or the end of the session
    /// closes it.
    pub fn set_always(&self, always: bool) {
        self.shared.always.store(always, Ordering::SeqCst);
        self.shared.changed.notify_all();
    }

    /// Opens the microphone now and keeps it until [`Self::disarm`] (or, when the standby has a
    /// time for an armed microphone, until that time passes without an arm or a call). Arming an
    /// open microphone opens nothing and starts the time again; arming while a call has it makes
    /// the call give it back armed.
    ///
    /// # Errors
    ///
    /// The source's refusal, or that the standby keeps nothing.
    pub fn arm(&self) -> Result<(), (Outcome, String)> {
        if self.shared.is_off() {
            return Err((
                Outcome::Unavailable,
                "the microphone standby is off: nothing is kept open (choose another standby to arm it)".to_string(),
            ));
        }
        {
            let mut held = self.shared.held();
            if let Some(kept) = held.as_mut() {
                kept.armed = true;
                kept.idle_since = Instant::now();
                return Ok(());
            }
        }
        if self.shared.leased.load(Ordering::SeqCst) > 0 {
            self.shared.arm_pending.store(true, Ordering::SeqCst);
            return Ok(());
        }
        let microphone = self.shared.source.lock().unwrap_or_else(PoisonError::into_inner).open()?;
        self.shared.hold(microphone, Vec::new(), true);
        Ok(())
    }

    /// Closes the microphone that waits, and forgets an arm that a call was going to hand on.
    pub fn disarm(&self) {
        self.shared.arm_pending.store(false, Ordering::SeqCst);
        self.release();
    }

    /// Whether a call has the microphone now.
    #[must_use]
    pub fn is_leased(&self) -> bool {
        self.shared.leased.load(Ordering::SeqCst) > 0
    }

    /// The time a microphone stays open after a call, ms.
    #[must_use]
    pub fn keep_ms(&self) -> u32 {
        self.shared.keep_ms.load(Ordering::SeqCst)
    }

    /// Closes the kept microphone now (none: nothing happens).
    pub fn release(&self) {
        drop(self.shared.take_held());
    }

    /// Whether a microphone is open and waiting for the next call.
    #[must_use]
    pub fn is_held(&self) -> bool {
        self.shared.held().is_some()
    }

    /// The microphone for one call: the kept one when there is one that is not out of time (with
    /// what came in since the last call), else a newly opened one.
    ///
    /// # Errors
    ///
    /// The source's refusal: `Denied` when the user said no, `Unavailable` when there is none.
    pub fn acquire(&self) -> Result<Lease, (Outcome, String)> {
        if let Some(held) = self.shared.take_held() {
            let in_time = self
                .shared
                .limit_of(&held)
                .map_or(true, |limit| !limit.is_zero() && held.idle_since.elapsed() < limit);
            if in_time {
                self.shared.leased.fetch_add(1, Ordering::SeqCst);
                return Ok(Lease {
                    microphone: Some(held.microphone),
                    preroll: held.preroll,
                    warm: true,
                    armed: held.armed,
                    shared: Arc::clone(&self.shared),
                });
            }
            // Out of time before the thread got to it: closed, and a new one is opened.
            drop(held);
        }
        let microphone = self
            .shared
            .source
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .open()?;
        self.shared.leased.fetch_add(1, Ordering::SeqCst);
        Ok(Lease {
            microphone: Some(microphone),
            preroll: Vec::new(),
            warm: false,
            armed: false,
            shared: Arc::clone(&self.shared),
        })
    }
}

/// A microphone for one call. [`Self::finish`] gives it back to be kept; a lease that is dropped
/// instead (the call returned early) closes it.
pub struct Lease {
    microphone: Option<Box<dyn Microphone>>,
    /// What came in while nobody listened, at the microphone's rate, oldest first (empty for a
    /// microphone that was just opened).
    pub preroll: Vec<f32>,
    /// The microphone was open before the call: its sound flows from the first read.
    pub warm: bool,
    /// The app had armed the microphone: it goes back armed.
    armed: bool,
    shared: Arc<Shared>,
}

impl Lease {
    /// The microphone.
    ///
    /// # Panics
    ///
    /// Never: the microphone is taken out only by [`Self::finish`], which consumes the lease.
    pub fn microphone(&mut self) -> &mut dyn Microphone {
        self.microphone.as_mut().expect("a lease holds its microphone until it is finished").as_mut()
    }

    /// Keeps of [`Self::preroll`] only the last `ms` (what an open microphone heard just before
    /// the call; the older is the room's from before the user meant to speak).
    pub fn trim_preroll(&mut self, ms: u32) {
        let keep = (u64::from(self.rate()) * u64::from(ms) / 1000) as usize;
        if self.preroll.len() > keep {
            let surplus = self.preroll.len() - keep;
            self.preroll.drain(..surplus);
        }
    }

    /// The rate of the microphone's sound, Hz.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.microphone.as_ref().map_or(0, |microphone| microphone.rate())
    }

    /// Ends the call's use of the microphone: kept for the next call when `keep` and the time is
    /// not zero, else closed.
    pub fn finish(mut self, keep: bool) {
        let microphone = self.microphone.take();
        let armed = self.armed || self.shared.arm_pending.swap(false, Ordering::SeqCst);
        if let Some(microphone) = microphone {
            let always = self.shared.always.load(Ordering::SeqCst);
            let kept_for_a_time = keep && self.shared.keep_ms.load(Ordering::SeqCst) > 0;
            let kept_armed = keep && armed && self.shared.armed_idle_ms.load(Ordering::SeqCst) > 0;
            if always || kept_for_a_time || kept_armed {
                self.shared.hold(microphone, Vec::new(), armed);
            }
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.shared.leased.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "keeper_test.rs"]
mod keeper_test;
