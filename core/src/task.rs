//! Timer and thread management for asynchronous operations.
//!
//! This module provides:
//! - `TimerId` / `ThreadId`: Unique identifiers for timers and background threads
//! - `Instant` / `Duration`: Cross-platform time types (works on no_std with tick counters)
//! - `ThreadReceiver`: Channel for receiving messages from the main thread
//! - Callback types for thread communication and system time queries

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    boxed::Box,
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    ffi::c_void,
    fmt,
    mem::ManuallyDrop,
    sync::atomic::{AtomicUsize, Ordering},
};
#[cfg(feature = "std")]
use std::sync::mpsc::{Receiver, Sender};
#[cfg(feature = "std")]
use std::sync::Mutex;
#[cfg(feature = "std")]
use std::thread::{self, JoinHandle};
#[cfg(feature = "std")]
use std::time::Duration as StdDuration;
#[cfg(feature = "std")]
use std::time::Instant as StdInstant;

use azul_css::{props::property::CssProperty, AzString};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{FocusTarget, TimerCallbackReturn, Update},
    dom::{DomId, DomNodeId, OptionDomNodeId},
    geom::{LogicalPosition, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    id::NodeId,
    refany::{OptionRefAny, RefAny},
    resources::{ImageCache, ImageMask, ImageRef},
    styled_dom::NodeHierarchyItemId,
    window::RawWindowHandle,
    FastBTreeSet, OrderedMap,
};

/// Should a timer terminate or not - used to remove active timers
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TerminateTimer {
    /// Remove the timer from the list of active timers
    Terminate,
    /// Do nothing and let the timers continue to run
    Continue,
}

// ============================================================================
// Reserved System Timer IDs (0x0000 - 0x00FF)
// ============================================================================
// User timers start at 0x0100 to avoid conflicts with system timers.
// These constants define well-known timer IDs for internal framework use.

/// Timer ID for cursor blinking in contenteditable elements (~530ms interval)
pub const CURSOR_BLINK_TIMER_ID: TimerId = TimerId { id: 0x0001 };
/// Timer ID for scroll momentum/inertia animation
pub const SCROLL_MOMENTUM_TIMER_ID: TimerId = TimerId { id: 0x0002 };
/// Timer ID for auto-scroll during drag operations near edges
pub const DRAG_AUTOSCROLL_TIMER_ID: TimerId = TimerId { id: 0x0003 };
/// Timer ID for tooltip show delay.
///
/// Started by the platform event loop when the hover target changes to a node
/// that advertises a tooltip source (`aria-label` / `alt` / `title`); fires
/// once after `SystemStyle::input_metrics.hover_time_ms` (`SPI_GETMOUSEHOVERTIME`
/// on Windows, default 400ms) and emits a `ShowTooltip` `CallbackChange`. The
/// timer is torn down on hover loss, which also emits `HideTooltip`.
///
/// Double-click detection used to live on a neighbouring reserved ID but is
/// now handled entirely by `GestureManager::detect_double_click`, so no
/// equivalent `DOUBLE_CLICK_TIMER_ID` exists.
pub const TOOLTIP_DELAY_TIMER_ID: TimerId = TimerId { id: 0x0004 };
/// Timer ID for the single-threaded capability pump (MWA-A1).
///
/// Armed by `sync_capability_pump_timer` whenever a capability source needs
/// polling or draining while the app is otherwise idle (gamepad listeners,
/// sensor listeners, an active geolocation subscription). Each tick wakes the
/// blocked platform loop; `invoke_expired_timers` then runs an event pass,
/// whose top-of-pass pump drains the async capability channels. There is NO
/// pump thread by design — a recurring shell timer is the only wake
/// mechanism, so the identical code path works on WASM (no threads).
pub const CAPABILITY_PUMP_TIMER_ID: TimerId = TimerId { id: 0x0005 };
/// Timer ID for the one-shot long-press wake-up (MWA-B12).
///
/// Armed on every `MouseDown` for the long-press threshold: a motionless
/// press generates no further events, so no pass would ever evaluate
/// `detect_long_press` — this timer wakes the loop exactly once at the
/// threshold, `invoke_expired_timers` runs an event pass, and the
/// detection fires (or doesn't — moved/released holds are no-ops).
pub const LONG_PRESS_TIMER_ID: TimerId = TimerId { id: 0x0006 };

/// First available ID for user-defined timers
pub const USER_TIMER_ID_START: usize = 0x0100;

// User timers start at 0x0100 to avoid conflicts with reserved system timer IDs
static MAX_TIMER_ID: AtomicUsize = AtomicUsize::new(USER_TIMER_ID_START);

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerId {
    pub id: usize,
}

impl TimerId {
    /// Generates a new, unique `TimerId`.
    #[must_use]
    pub fn unique() -> Self {
        Self {
            id: MAX_TIMER_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

impl_option!(
    TimerId,
    OptionTimerId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(TimerId, TimerIdVec, TimerIdVecDestructor, TimerIdVecDestructorType, TimerIdVecSlice, OptionTimerId);
impl_vec_debug!(TimerId, TimerIdVec);
impl_vec_clone!(TimerId, TimerIdVec, TimerIdVecDestructor);
impl_vec_partialeq!(TimerId, TimerIdVec);
impl_vec_partialord!(TimerId, TimerIdVec);

// Thread IDs 0-4 are reserved for internal framework use.
// User threads start at RESERVED_THREAD_ID_COUNT.
const RESERVED_THREAD_ID_COUNT: usize = 5;
static MAX_THREAD_ID: AtomicUsize = AtomicUsize::new(RESERVED_THREAD_ID_COUNT);

/// ID for uniquely identifying a background thread
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ThreadId {
    id: usize,
}

impl_option!(
    ThreadId,
    OptionThreadId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(ThreadId, ThreadIdVec, ThreadIdVecDestructor, ThreadIdVecDestructorType, ThreadIdVecSlice, OptionThreadId);
impl_vec_debug!(ThreadId, ThreadIdVec);
impl_vec_clone!(ThreadId, ThreadIdVec, ThreadIdVecDestructor);
impl_vec_partialeq!(ThreadId, ThreadIdVec);
impl_vec_partialord!(ThreadId, ThreadIdVec);

impl ThreadId {
    /// Generates a new, unique `ThreadId`.
    #[must_use]
    pub fn unique() -> Self {
        Self {
            id: MAX_THREAD_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

/// A point in time, either from the system clock or a tick counter.
///
/// Use `Instant::System` on platforms with std, `Instant::Tick` on `embedded/no_std`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Instant {
    /// System time from `std::time::Instant` (requires "std" feature)
    System(InstantPtr),
    /// Tick-based time for embedded systems without a real-time clock
    Tick(SystemTick),
}

#[cfg(feature = "std")]
impl From<StdInstant> for Instant {
    fn from(s: StdInstant) -> Self {
        Self::System(s.into())
    }
}

impl Instant {
    /// Returns the current system time.
    /// 
    /// On systems with std, this uses `std::time::Instant::now()`.
    /// On `no_std` systems, this returns a zero tick.
    #[cfg(feature = "std")]
    #[must_use] pub fn now() -> Self {
        StdInstant::now().into()
    }

    /// Returns the current system time (no_std fallback).
    #[cfg(not(feature = "std"))]
    pub fn now() -> Self {
        Instant::Tick(SystemTick::new(0))
    }

    /// Returns a number from 0.0 to 1.0 indicating the current
    /// linear interpolation value between (start, end)
    #[must_use] pub fn linear_interpolate(&self, mut start: Self, mut end: Self) -> f32 {
        use core::mem;

        if end < start {
            mem::swap(&mut start, &mut end);
        }

        if *self < start {
            return 0.0;
        }
        if *self > end {
            return 1.0;
        }

        // Zero-length interval: `duration_current / duration_total` would be
        // `0/0 = NaN`. Treat a collapsed interval as fully elapsed (1.0) rather
        // than propagating NaN into animation progress.
        if start == end {
            return 1.0;
        }

        let duration_total = end.duration_since(&start);
        let duration_current = self.duration_since(&start);

        let ratio = duration_current.div(&duration_total);
        if ratio.is_nan() {
            return 1.0;
        }
        ratio.clamp(0.0, 1.0)
    }

    /// Adds a duration to the instant, does nothing in undefined cases
    /// (i.e. trying to add a `Duration::Tick` to an `Instant::System`).
    ///
    /// Mismatched kinds (`System` instant + `Tick` duration, or vice versa)
    /// saturate to `self` unchanged instead of panicking — a stray mismatch
    /// must never crash the event loop.
    #[must_use] pub fn add_optional_duration(&self, duration: Option<&Duration>) -> Self {
        duration.map_or_else(|| self.clone(), |d| match (self, d) {
                (Self::System(i), Duration::System(d)) => {
                    #[cfg(feature = "std")]
                    {
                        let s: StdInstant = i.clone().into();
                        let d: StdDuration = (*d).into();
                        let new: InstantPtr = (s + d).into();
                        Self::System(new)
                    }
                    #[cfg(not(feature = "std"))]
                    {
                        // A `System` instant cannot be constructed on no_std, so
                        // this arm is unreachable in practice; return self rather
                        // than aborting.
                        let _ = (i, d);
                        self.clone()
                    }
                }
                (Self::Tick(s), Duration::Tick(d)) => Self::Tick(SystemTick {
                    // Saturate so a runaway tick delta cannot overflow-panic.
                    tick_counter: s.tick_counter.saturating_add(d.tick_diff),
                }),
                // Mismatched kinds: undefined operation -> do nothing (saturate).
                _ => self.clone(),
            })
    }

    /// Converts to `std::time::Instant` (panics if Tick variant).
    #[cfg(feature = "std")]
    #[must_use] pub fn into_std_instant(self) -> StdInstant {
        match self {
            Self::System(s) => s.into(),
            Self::Tick(_) => unreachable!(),
        }
    }

    /// Calculates the duration since an earlier point in time.
    ///
    /// Saturates to a zero duration in the degenerate cases (earlier is actually
    /// *later* than `self`, or the two instants are of mismatched kinds) instead
    /// of panicking — this runs on the hot event-loop path and must not crash.
    #[must_use] pub fn duration_since(&self, earlier: &Self) -> Duration {
        match (earlier, self) {
            (Self::System(prev), Self::System(now)) => {
                #[cfg(feature = "std")]
                {
                    let prev_instant: StdInstant = prev.clone().into();
                    let now_instant: StdInstant = now.clone().into();
                    // `saturating_duration_since` yields 0 if `prev` is later
                    // than `now` (monotonic-clock skew / reordered instants).
                    Duration::System(now_instant.saturating_duration_since(prev_instant).into())
                }
                #[cfg(not(feature = "std"))]
                {
                    // Unreachable on no_std (no System instants); saturate to 0.
                    let _ = (prev, now);
                    Duration::Tick(SystemTickDiff { tick_diff: 0 })
                }
            }
            (
                Self::Tick(SystemTick { tick_counter: prev }),
                Self::Tick(SystemTick { tick_counter: now }),
            ) => Duration::Tick(SystemTickDiff {
                // Saturate: a "negative" span (prev > now) clamps to 0.
                tick_diff: now.saturating_sub(*prev),
            }),
            // Mismatched kinds: no meaningful span -> saturate to 0.
            _ => Duration::Tick(SystemTickDiff { tick_diff: 0 }),
        }
    }
}

/// Tick-based timestamp for systems without a real-time clock.
///
/// Used on embedded systems where time is measured in frame ticks or cycles.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTick {
    pub tick_counter: u64,
}

impl SystemTick {
    /// Creates a new tick timestamp from a counter value.
    #[must_use] pub const fn new(tick_counter: u64) -> Self {
        Self { tick_counter }
    }
}

/// FFI-safe wrapper around `std::time::Instant` with custom clone/drop callbacks.
///
/// Allows crossing FFI boundaries while maintaining proper memory management.
#[repr(C)]
pub struct InstantPtr {
    /// `ManuallyDrop` so the owned `Box` is freed ONLY when `run_destructor` is
    /// still set (see `Drop`). The codegen FFI wrappers (`AzTimerCallbackInfo`
    /// etc.) embed this by value AND have their own `Drop` that `drop_in_place`s
    /// the real type first; Rust's drop glue would then drop this `ptr` field a
    /// SECOND time on the same bytes. Gating the `Box` free on `run_destructor`
    /// (cleared by the first drop) makes that second drop a safe no-op. Layout is
    /// unchanged: `ManuallyDrop<Box<T>>` is one pointer, like the old `Box<T>`.
    #[cfg(feature = "std")]
    pub ptr: ManuallyDrop<Box<StdInstant>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub clone_fn: InstantPtrCloneCallback,
    pub destructor: InstantPtrDestructorCallback,
    pub run_destructor: bool,
}

pub type InstantPtrCloneCallbackType = extern "C" fn(*const InstantPtr) -> InstantPtr;
#[repr(C)]
pub struct InstantPtrCloneCallback {
    pub cb: InstantPtrCloneCallbackType,
}
impl_callback_simple!(InstantPtrCloneCallback);

pub type InstantPtrDestructorCallbackType = extern "C" fn(*mut InstantPtr);
#[repr(C)]
pub struct InstantPtrDestructorCallback {
    pub cb: InstantPtrDestructorCallbackType,
}
impl_callback_simple!(InstantPtrDestructorCallback);

// ----  LIBSTD implementation for InstantPtr BEGIN
#[cfg(feature = "std")]
impl fmt::Debug for InstantPtr {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "{:?}", self.get())
    }
}

#[cfg(not(feature = "std"))]
impl core::fmt::Debug for InstantPtr {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{:?}", self.ptr as usize)
    }
}

#[cfg(feature = "std")]
impl core::hash::Hash for InstantPtr {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

#[cfg(not(feature = "std"))]
impl core::hash::Hash for InstantPtr {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.ptr as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for InstantPtr {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

#[cfg(not(feature = "std"))]
impl PartialEq for InstantPtr {
    fn eq(&self, other: &InstantPtr) -> bool {
        (self.ptr as usize).eq(&(other.ptr as usize))
    }
}

impl Eq for InstantPtr {}

#[cfg(feature = "std")]
impl PartialOrd for InstantPtr {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.get()).cmp(&(other.get())))
    }
}

#[cfg(not(feature = "std"))]
impl PartialOrd for InstantPtr {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.ptr as usize).cmp(&(other.ptr as usize)))
    }
}

#[cfg(feature = "std")]
impl Ord for InstantPtr {
    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
        (self.get()).cmp(&(other.get()))
    }
}

#[cfg(not(feature = "std"))]
impl Ord for InstantPtr {
    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
        (self.ptr as usize).cmp(&(other.ptr as usize))
    }
}

#[cfg(feature = "std")]
impl InstantPtr {
    fn get(&self) -> StdInstant {
        (**self.ptr)
    }
}

impl Clone for InstantPtr {
    fn clone(&self) -> Self {
        (self.clone_fn.cb)(self)
    }
}

#[cfg(feature = "std")]
extern "C" fn std_instant_clone(ptr: *const InstantPtr) -> InstantPtr {
    let az_instant_ptr = unsafe { &*ptr };
    InstantPtr {
        ptr: ManuallyDrop::new((*az_instant_ptr.ptr).clone()),
        clone_fn: az_instant_ptr.clone_fn,
        destructor: az_instant_ptr.destructor,
        run_destructor: true,
    }
}

#[cfg(feature = "std")]
impl From<StdInstant> for InstantPtr {
    fn from(s: StdInstant) -> Self {
        Self {
            ptr: ManuallyDrop::new(Box::new(s)),
            clone_fn: InstantPtrCloneCallback {
                cb: std_instant_clone,
            },
            destructor: InstantPtrDestructorCallback {
                cb: std_instant_drop,
            },
            run_destructor: true,
        }
    }
}

#[cfg(feature = "std")]
impl From<InstantPtr> for StdInstant {
    fn from(s: InstantPtr) -> Self {
        s.get()
    }
}

impl Drop for InstantPtr {
    fn drop(&mut self) {
        if self.run_destructor {
            self.run_destructor = false;
            (self.destructor.cb)(self);
            // Free the owned Box exactly once, here under the run_destructor guard.
            // A second drop on the same bytes (the codegen wrapper's field-drop after
            // its own `_delete` already ran the real drop) sees run_destructor=false
            // and skips this -> no double-free. (non-std `ptr` is a raw POD pointer
            // freed by the destructor callback above, so nothing to drop here.)
            #[cfg(feature = "std")]
            unsafe {
                ManuallyDrop::drop(&mut self.ptr);
            }
        }
    }
}

#[cfg(feature = "std")]
const extern "C" fn std_instant_drop(_: *mut InstantPtr) {}

// ----  LIBSTD implementation for InstantPtr END

/// A span of time, either from the system clock or as tick difference.
///
/// Mirrors `Instant` variants - System durations work with System instants,
/// Tick durations work with Tick instants.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Duration {
    /// System duration from `std::time::Duration` (requires "std" feature)
    System(SystemTimeDiff),
    /// Tick-based duration for embedded systems
    Tick(SystemTickDiff),
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Self::System(s) => {
                let s: StdDuration = (*s).into();
                write!(f, "{s:?}")
            }
            #[cfg(not(feature = "std"))]
            Duration::System(s) => write!(f, "({}s, {}ns)", s.secs, s.nanos),
            Self::Tick(tick) => write!(f, "{} ticks", tick.tick_diff),
        }
    }
}

#[cfg(feature = "std")]
impl From<StdDuration> for Duration {
    fn from(s: StdDuration) -> Self {
        Self::System(s.into())
    }
}

impl Duration {
    /// Returns the maximum possible duration.
    #[must_use] pub fn max() -> Self {
        #[cfg(feature = "std")]
        {
            Self::System(StdDuration::new(core::u64::MAX, NANOS_PER_SEC - 1).into())
        }
        #[cfg(not(feature = "std"))]
        {
            Duration::Tick(SystemTickDiff {
                tick_diff: u64::MAX,
            })
        }
    }

    /// Divides this duration by another, returning the ratio as f32.
    // the f64 ratio is intentionally narrowed to the f32 return type; the value
    // is a duration ratio, far inside f32's range.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use] pub fn div(&self, other: &Self) -> f32 {
        use self::Duration::{System, Tick};
        match (self, other) {
            (System(s), System(s2)) => s.div(s2) as f32,
            (Tick(t), Tick(t2)) => t.div(t2) as f32,
            _ => 0.0,
        }
    }

    /// Returns the smaller of two durations.
    #[must_use] pub fn min(self, other: Self) -> Self {
        if self.smaller_than(&other) {
            self
        } else {
            other
        }
    }

    /// Returns true if self > other.
    ///
    /// Mismatched kinds (comparing a System duration with a Tick duration) are
    /// undefined and saturate to `false` instead of panicking.
    #[allow(unused_variables)]
    #[must_use] pub fn greater_than(&self, other: &Self) -> bool {
        match (self, other) {
            // self > other
            (Self::System(s), Self::System(o)) => {
                #[cfg(feature = "std")]
                {
                    let s: StdDuration = (*s).into();
                    let o: StdDuration = (*o).into();
                    s > o
                }
                #[cfg(not(feature = "std"))]
                {
                    false
                }
            }
            (Self::Tick(s), Self::Tick(o)) => s.tick_diff > o.tick_diff,
            _ => false,
        }
    }

    /// Returns true if self < other.
    ///
    /// Mismatched kinds (comparing a System duration with a Tick duration) are
    /// undefined and saturate to `false` instead of panicking.
    #[allow(unused_variables)]
    #[must_use] pub fn smaller_than(&self, other: &Self) -> bool {
        // self < other
        match (self, other) {
            // self > other
            (Self::System(s), Self::System(o)) => {
                #[cfg(feature = "std")]
                {
                    let s: StdDuration = (*s).into();
                    let o: StdDuration = (*o).into();
                    s < o
                }
                #[cfg(not(feature = "std"))]
                {
                    false
                }
            }
            (Self::Tick(s), Self::Tick(o)) => s.tick_diff < o.tick_diff,
            _ => false,
        }
    }
}

/// Represents a difference in ticks for systems that
/// don't support timing
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTickDiff {
    pub tick_diff: u64,
}

impl SystemTickDiff {
    /// Divide duration A by duration B.
    /// Returns `Inf` or `NaN` if `other` is zero.
    // tick counts -> f64 for the ratio; precision only degrades past 2^53 ticks.
    #[allow(clippy::cast_precision_loss)]
    #[must_use] pub fn div(&self, other: &Self) -> f64 {
        self.tick_diff as f64 / other.tick_diff as f64
    }
}

/// Duration represented as seconds + nanoseconds (mirrors `std::time::Duration`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTimeDiff {
    pub secs: u64,
    pub nanos: u32,
}

impl SystemTimeDiff {
    /// Divide duration A by duration B.
    /// Returns `Inf` or `NaN` if `other` is zero.
    #[must_use] pub fn div(&self, other: &Self) -> f64 {
        self.as_secs_f64() / other.as_secs_f64()
    }
    // secs (u64) -> f64 loses precision only past 2^53 seconds (~285M years).
    #[allow(clippy::cast_precision_loss)]
    fn as_secs_f64(&self) -> f64 {
        (self.secs as f64) + (f64::from(self.nanos) / f64::from(NANOS_PER_SEC))
    }
}

#[cfg(feature = "std")]
impl From<StdDuration> for SystemTimeDiff {
    fn from(d: StdDuration) -> Self {
        Self {
            secs: d.as_secs(),
            nanos: d.subsec_nanos(),
        }
    }
}

#[cfg(feature = "std")]
impl From<SystemTimeDiff> for StdDuration {
    fn from(d: SystemTimeDiff) -> Self {
        Self::new(d.secs, d.nanos)
    }
}

const MILLIS_PER_SEC: u64 = 1_000;
const NANOS_PER_MILLI: u32 = 1_000_000;
const NANOS_PER_SEC: u32 = 1_000_000_000;

impl SystemTimeDiff {
    /// Creates a duration from whole seconds.
    #[must_use] pub const fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }
    /// Creates a duration from milliseconds.
    #[must_use] pub const fn from_millis(millis: u64) -> Self {
        Self {
            secs: millis / MILLIS_PER_SEC,
            nanos: ((millis % MILLIS_PER_SEC) as u32) * NANOS_PER_MILLI,
        }
    }
    /// Creates a duration from nanoseconds.
    // const fn (no const TryFrom); `nanos % NANOS_PER_SEC` is always < 10^9, which
    // fits u32, so the narrowing cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use] pub const fn from_nanos(nanos: u64) -> Self {
        Self {
            secs: nanos / (NANOS_PER_SEC as u64),
            nanos: (nanos % (NANOS_PER_SEC as u64)) as u32,
        }
    }
    /// Adds two durations, returning None on overflow.
    #[must_use] pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        if let Some(mut secs) = self.secs.checked_add(rhs.secs) {
            let mut nanos = self.nanos + rhs.nanos;
            if nanos >= NANOS_PER_SEC {
                nanos -= NANOS_PER_SEC;
                if let Some(new_secs) = secs.checked_add(1) {
                    secs = new_secs;
                } else {
                    return None;
                }
            }
            Some(Self { secs, nanos })
        } else {
            None
        }
    }

    /// Returns the total duration in milliseconds.
    ///
    /// Saturates at `u64::MAX` instead of overflow-panicking for enormous
    /// `secs` values (`secs * 1000` overflows around ~1.8e16 seconds).
    #[must_use] pub const fn millis(&self) -> u64 {
        self.secs
            .saturating_mul(MILLIS_PER_SEC)
            .saturating_add((self.nanos / NANOS_PER_MILLI) as u64)
    }

    /// Converts to `std::time::Duration`.
    #[cfg(feature = "std")]
    #[must_use] pub fn get(&self) -> StdDuration {
        (*self).into()
    }
}

impl_option!(
    Instant,
    OptionInstant,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_option!(
    Duration,
    OptionDuration,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Message that can be sent from the main thread to the Thread using the `ThreadId`.
///
/// The thread can ignore the event.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadSendMsg {
    /// The thread should terminate at the nearest
    TerminateThread,
    /// Next frame tick
    Tick,
    /// Custom data
    Custom(RefAny),
}

impl_option!(
    ThreadSendMsg,
    OptionThreadSendMsg,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

/// Channel endpoint for receiving messages from the main thread in a background thread.
///
/// Thread-safe wrapper around the receiver end of a message channel.
#[derive(Debug)]
#[repr(C)]
pub struct ThreadReceiver {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadReceiverInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    pub ctx: OptionRefAny,
}

impl Clone for ThreadReceiver {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
            ctx: self.ctx.clone(),
        }
    }
}

impl Drop for ThreadReceiver {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl ThreadReceiver {
    /// Creates a new receiver (no-op on no_std).
    #[cfg(not(feature = "std"))]
    pub fn new(_t: ThreadReceiverInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
            ctx: OptionRefAny::None,
        }
    }

    /// Creates a new receiver wrapping the inner channel.
    #[cfg(feature = "std")]
    #[must_use] pub fn new(t: ThreadReceiverInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
            ctx: OptionRefAny::None,
        }
    }

    /// Get the FFI context (e.g., Python callable)
    #[must_use] pub fn get_ctx(&self) -> OptionRefAny {
        self.ctx.clone()
    }

    /// Receives a message (returns None on no_std).
    #[cfg(not(feature = "std"))]
    pub fn recv(&mut self) -> OptionThreadSendMsg {
        None.into()
    }

    /// Receives a message from the main thread, if available.
    #[cfg(feature = "std")]
    pub fn recv(&mut self) -> OptionThreadSendMsg {
        let Some(ts) = self.ptr.lock().ok() else {
            return None.into();
        };
        (ts.recv_fn.cb)(std::ptr::from_ref(ts.ptr.as_ref()) as *const c_void)
    }
}

/// Inner receiver state containing the actual channel and callbacks.
#[derive(Debug)]
#[cfg_attr(not(feature = "std"), derive(PartialEq, PartialOrd, Eq, Ord))]
#[repr(C)]
pub struct ThreadReceiverInner {
    #[cfg(feature = "std")]
    pub ptr: Box<Receiver<ThreadSendMsg>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub recv_fn: ThreadRecvCallback,
    pub destructor: ThreadReceiverDestructorCallback,
}

#[cfg(not(feature = "std"))]
unsafe impl Send for ThreadReceiverInner {}

#[cfg(feature = "std")]
impl core::hash::Hash for ThreadReceiverInner {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (std::ptr::from_ref(self.ptr.as_ref()) as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadReceiverInner {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.ptr.as_ref(), other.ptr.as_ref())
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadReceiverInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadReceiverInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (std::ptr::from_ref(self.ptr.as_ref()) as usize)
                .cmp(&(std::ptr::from_ref(other.ptr.as_ref()) as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadReceiverInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (std::ptr::from_ref(self.ptr.as_ref()) as usize).cmp(&(std::ptr::from_ref(other.ptr.as_ref()) as usize))
    }
}

impl Drop for ThreadReceiverInner {
    fn drop(&mut self) {
        (self.destructor.cb)(self);
    }
}

/// Get the current system type, equivalent to `std::time::Instant::now()`, except it
/// also works on systems that don't have a clock (such as embedded timers)
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;
#[repr(C)]
pub struct GetSystemTimeCallback {
    pub cb: GetSystemTimeCallbackType,
}
impl_callback_simple!(GetSystemTimeCallback);

/// Default implementation that gets the current system time.
///
/// On WASM targets `std::time::Instant::now()` panics, so we fall back to
/// a zero-tick instant instead.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[must_use] pub extern "C" fn get_system_time_libstd() -> Instant {
    StdInstant::now().into()
}

/// Fallback for WASM (where `Instant::now()` panics) and no-std targets.
#[cfg(any(not(feature = "std"), target_arch = "wasm32"))]
pub extern "C" fn get_system_time_libstd() -> Instant {
    Instant::Tick(SystemTick::new(0))
}

/// Callback to check if a thread has finished execution.
pub type CheckThreadFinishedCallbackType =
    extern "C" fn(/* dropcheck */ *const c_void) -> bool;
/// Wrapper for thread completion check callback.
#[repr(C)]
pub struct CheckThreadFinishedCallback {
    pub cb: CheckThreadFinishedCallbackType,
}
impl_callback_simple!(CheckThreadFinishedCallback);

/// Callback to send a message to a background thread.
pub type LibrarySendThreadMsgCallbackType =
    extern "C" fn(/* Sender<ThreadSendMsg> */ *const c_void, ThreadSendMsg) -> bool;
/// Wrapper for thread message send callback.
#[repr(C)]
pub struct LibrarySendThreadMsgCallback {
    pub cb: LibrarySendThreadMsgCallbackType,
}
impl_callback_simple!(LibrarySendThreadMsgCallback);

/// Callback for a running thread to receive messages from the main thread.
pub type ThreadRecvCallbackType =
    extern "C" fn(/* receiver.ptr */ *const c_void) -> OptionThreadSendMsg;
/// Wrapper for thread message receive callback.
#[repr(C)]
pub struct ThreadRecvCallback {
    pub cb: ThreadRecvCallbackType,
}
impl_callback_simple!(ThreadRecvCallback);

/// Callback to destroy a `ThreadReceiver`.
pub type ThreadReceiverDestructorCallbackType = extern "C" fn(*mut ThreadReceiverInner);
/// Wrapper for thread receiver destructor callback.
#[repr(C)]
pub struct ThreadReceiverDestructorCallback {
    pub cb: ThreadReceiverDestructorCallbackType,
}
impl_callback_simple!(ThreadReceiverDestructorCallback);

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on interpolation results
mod tests {
    use super::*;

    fn tick(n: u64) -> Instant {
        Instant::Tick(SystemTick::new(n))
    }
    fn tick_dur(n: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: n })
    }
    fn sys_dur(secs: u64, nanos: u32) -> Duration {
        Duration::System(SystemTimeDiff { secs, nanos })
    }

    #[test]
    fn linear_interpolate_zero_interval_is_one_not_nan() {
        let t = tick(5);
        let v = t.linear_interpolate(tick(5), tick(5));
        assert!(v.is_finite());
        assert_eq!(v, 1.0);
    }

    #[test]
    fn linear_interpolate_midpoint() {
        let v = tick(5).linear_interpolate(tick(0), tick(10));
        assert!((v - 0.5).abs() < 1e-6);
    }

    #[test]
    fn duration_since_saturates_on_negative() {
        // earlier is actually later -> saturate to zero, no panic.
        let d = tick(1).duration_since(&tick(10));
        assert_eq!(d, tick_dur(0));
    }

    #[test]
    fn duration_compare_mismatched_kinds_saturates() {
        // greater_than / smaller_than across kinds must not panic (saturate to false).
        let a = tick_dur(5);
        let b = sys_dur(1, 0);
        assert!(!a.greater_than(&b));
        assert!(!a.smaller_than(&b));
        assert!(!b.greater_than(&a));
        assert!(!b.smaller_than(&a));
    }

    #[test]
    fn add_optional_duration_mismatched_is_noop() {
        let inst = tick(100);
        // Adding a System duration to a Tick instant is undefined -> returns self.
        let out = inst.add_optional_duration(Some(&sys_dur(1, 0)));
        assert_eq!(out, tick(100));
        // Matching kinds add and saturate.
        let out2 = inst.add_optional_duration(Some(&tick_dur(5)));
        assert_eq!(out2, tick(105));
        // Saturating add: near-max tick doesn't overflow-panic.
        let big = tick(u64::MAX);
        let out3 = big.add_optional_duration(Some(&tick_dur(10)));
        assert_eq!(out3, tick(u64::MAX));
    }

    #[test]
    fn millis_saturates_on_overflow() {
        let huge = SystemTimeDiff { secs: u64::MAX, nanos: 0 };
        assert_eq!(huge.millis(), u64::MAX);
        let normal = SystemTimeDiff { secs: 2, nanos: 500_000_000 };
        assert_eq!(normal.millis(), 2500);
    }
}
