//! The queues between the Java UI thread and the loop thread (`android_main`).
//!
//! # The rule
//!
//! NativeActivity runs `android_main` on a thread of its own; every callback of the Java side (the
//! `InputConnection`, the insets listener, the gesture detectors, the selection toolbar, the
//! accessibility provider) arrives on the UI thread. The shell handed those callbacks the address of the
//! `AndroidWindow` on `android_main`'s stack and they formed `&mut AndroidWindow` from it while the loop
//! thread held its own: two live `&mut` to one window (undefined behaviour) and real races (the IME
//! replacing the preedit `String` while the layout read it; `getTextBeforeCursor` walking the layout
//! window while a regeneration freed what it walked). After `android_main` returned the address dangled.
//!
//! Now the Java side holds the address of a [`UiQueues`] (leaked, so it never dangles; closed when the
//! loop ends): everything a UI callback does is `push` an event, set the theme, or read the mirror the
//! loop published. The loop drains the events at the top of each iteration and applies them with the
//! window it owns. `UiQueues` is `Send + Sync` by construction (locks and atomics), and holds nothing
//! of the window.
//!
//! # Reads
//!
//! `InputConnection.getTextBeforeCursor` and its siblings must answer on the UI thread, now. They read the
//! [`ImeMirror`] the loop publishes after it has applied events. An IME asks again right after a
//! `commitText`, so a read made after a push waits (a few milliseconds, bounded) for the loop to publish
//! a mirror that has seen that event, then answers with what it has: a loop that is busy or in the
//! background makes the answer late-stale, never a deadlock.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
        Condvar, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use super::ime::ImeMirror;

/// A gesture the Android detectors recognised, in the Java side's units (PHYSICAL pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RawGesture {
    /// `GestureDetector.onDoubleTap`.
    DoubleTap,
    /// `GestureDetector.onLongPress` at a point.
    LongPress { x: f32, y: f32, duration_ms: i64 },
    /// `GestureDetector.onFling`: 0 up, 1 down, 2 left, 3 right (`GestureDirection`).
    Swipe { direction: i32 },
    /// `ScaleGestureDetector`: the scale is CUMULATIVE since the pinch began.
    Pinch {
        scale: f32,
        center_x: f32,
        center_y: f32,
        initial_distance: f32,
        current_distance: f32,
        duration_ms: i64,
        began: bool,
        ended: bool,
    },
    /// The two-finger rotation: the turn since the last update, radians, clockwise positive.
    Rotation { angle: f32, center_x: f32, center_y: f32, duration_ms: i64 },
}

/// What the UI thread tells the loop, in the order it happened.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// `InputConnection.commitText`.
    Commit(String),
    /// `InputConnection.setComposingText`: Android's own cursor convention (`> 0` counts from the end
    /// of the composing text, `<= 0` from its start).
    SetComposing { text: String, cursor: i32 },
    /// `InputConnection.finishComposingText`.
    FinishComposing,
    /// `InputConnection.deleteSurroundingText`, in UTF-16 units.
    DeleteSurrounding { before: i32, after: i32 },
    /// The floating toolbar: 0 cut, 1 copy, 2 paste, 3 select all.
    SelectionAction(i32),
    /// `View.onPointerCaptureChange`.
    PointerCapture(bool),
    /// `View.onCapturedPointerEvent`: a delta of a captured mouse or touchpad.
    RawMotion { dx: f32, dy: f32, device: i32 },
    /// `View.onApplyWindowInsets`, PHYSICAL pixels (the keyboard apart from the bars).
    Insets { top: i32, bottom: i32, left: i32, right: i32, ime: i32 },
    /// A recognised gesture.
    Gesture(RawGesture),
    /// `Activity.onTrimMemory(level)` (the `TRIM_MEMORY_*` of `ComponentCallbacks2`).
    TrimMemory(i32),
}

/// The inbox and the mirror. Everything is behind a lock or an atomic.
pub struct UiQueues {
    /// Set when `android_main` has ended: every entry point then answers its default and queues nothing.
    closed: AtomicBool,
    /// The device's night mode as the activity last reported it: 0 none pending, 1 light, 2 dark.
    theme: AtomicI32,
    /// The soft keyboard the engine asked for since the loop last raised it: -1 nothing, 0 hide, 1 show.
    keyboard: AtomicI32,
    /// Events pushed by the UI thread, oldest first.
    events: Mutex<Vec<UiEvent>>,
    /// The number of events ever pushed.
    pushed: AtomicU64,
    /// The mirror the loop published and the number of pushed events it had applied by then.
    mirror: Mutex<Published>,
    /// Signalled when a mirror was published.
    published: Condvar,
}

/// What the loop last published for the IME.
struct Published {
    /// Events pushed before this mirror was made had been applied.
    applied: u64,
    mirror: ImeMirror,
}

impl Default for UiQueues {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for UiQueues {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UiQueues")
            .field("closed", &self.is_closed())
            .field("pushed", &self.pushed())
            .field("published_through", &self.published_through())
            .finish_non_exhaustive()
    }
}

/// A poisoned lock still holds consistent queues (nothing in here panics while holding one): take it.
fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl UiQueues {
    /// How long a read of the mirror waits for the loop to have seen an event that was pushed before
    /// it. An IME asks within microseconds of a commit; the loop's tick is 16 ms.
    pub const READ_WAIT: Duration = Duration::from_millis(40);

    /// Empty queues, open.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
            theme: AtomicI32::new(0),
            keyboard: AtomicI32::new(-1),
            events: Mutex::new(Vec::new()),
            pushed: AtomicU64::new(0),
            mirror: Mutex::new(Published {
                applied: 0,
                mirror: ImeMirror {
                    active: false,
                    text: String::new(),
                    caret: 0,
                    selected: None,
                    selection_rect: None,
                    hints: 0,
                    composing: false,
                },
            }),
            published: Condvar::new(),
        }
    }

    /// `android_main` ended: nothing is queued from now on, reads answer their defaults, and a reader
    /// that waits is let go.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        locked(&self.events).clear();
        self.published.notify_all();
    }

    /// Whether `android_main` has ended.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    // ---- UI thread -> loop ---------------------------------------------------------------------

    /// Queue an event for the loop. Dropped when the loop has ended.
    pub fn push(&self, event: UiEvent) {
        if self.is_closed() {
            return;
        }
        let mut events = locked(&self.events);
        events.push(event);
        self.pushed.fetch_add(1, Ordering::AcqRel);
    }

    /// The device's night mode changed: `1` light, `2` dark (anything else is ignored: the device
    /// expressed no preference).
    pub fn set_theme(&self, night_mode: i32) {
        if !self.is_closed() && matches!(night_mode, 1 | 2) {
            self.theme.store(night_mode, Ordering::Release);
        }
    }

    // ---- loop side -----------------------------------------------------------------------------

    /// Take every queued event, oldest first, and the number of events ever pushed up to and including
    /// them (counted under the same lock as the push, so the two always agree). Publish a mirror made
    /// after applying them with that number.
    pub fn drain(&self) -> (Vec<UiEvent>, u64) {
        let mut events = locked(&self.events);
        let pushed = self.pushed.load(Ordering::Acquire);
        (core::mem::take(&mut *events), pushed)
    }

    /// Whether the UI thread has queued anything the loop has not taken.
    #[must_use]
    pub fn has_events(&self) -> bool {
        !locked(&self.events).is_empty()
    }

    /// Take the pending night mode: `0` when none, else `1` light or `2` dark.
    pub fn take_theme(&self) -> i32 {
        self.theme.swap(0, Ordering::AcqRel)
    }

    /// The engine asks for the soft keyboard (`true`) or for it to go (`false`) while it handles an event.
    /// Only recorded, the last word wins: the loop raises it AFTER it has published the mirror of the focused
    /// text (see [`Self::take_keyboard_request`]).
    pub fn request_keyboard(&self, visible: bool) {
        self.keyboard.store(i32::from(visible), Ordering::Release);
    }

    /// Take the keyboard request of this iteration of the loop, once. The loop publishes the mirror first and
    /// raises the keyboard then: the IME asks the mirror which kind of field has the focus
    /// (`onCreateInputConnection`) within milliseconds of `showSoftInput`, and a keyboard raised from inside
    /// the event handling reached the UI thread before the mirror said which field that was.
    pub fn take_keyboard_request(&self) -> Option<bool> {
        match self.keyboard.swap(-1, Ordering::AcqRel) {
            -1 => None,
            wanted => Some(wanted != 0),
        }
    }

    /// The number of events pushed so far.
    #[must_use]
    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Acquire)
    }

    /// The `applied` number of the mirror last published (what [`Self::drain`] returned for it).
    #[must_use]
    pub fn published_through(&self) -> u64 {
        locked(&self.mirror).applied
    }

    /// Publish the mirror the IME reads. `applied` is the count [`Self::drain`] returned for the events
    /// the mirror reflects.
    pub fn publish(&self, applied: u64, mirror: ImeMirror) {
        let mut published = locked(&self.mirror);
        published.applied = applied;
        published.mirror = mirror;
        drop(published);
        self.published.notify_all();
    }

    // ---- UI thread reads -----------------------------------------------------------------------

    /// The mirror, once the loop has seen every event pushed before this call (it waits at most
    /// [`Self::READ_WAIT`] for that), else what was last published. Empty after `close`.
    #[must_use]
    pub fn read_mirror(&self) -> ImeMirror {
        self.read_mirror_within(Self::READ_WAIT)
    }

    /// [`Self::read_mirror`] with an explicit bound.
    #[must_use]
    pub fn read_mirror_within(&self, wait: Duration) -> ImeMirror {
        if self.is_closed() {
            return ImeMirror::default();
        }
        let need = self.pushed();
        let deadline = Instant::now() + wait;
        let mut published = locked(&self.mirror);
        while published.applied < need && !self.is_closed() {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            published = self
                .published
                .wait_timeout(published, deadline - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        if self.is_closed() {
            return ImeMirror::default();
        }
        published.mirror.clone()
    }
}

#[cfg(test)]
#[path = "ui_bridge_test.rs"]
mod tests;
