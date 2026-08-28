//! Timer and thread management for asynchronous operations. This module provides: -
//! `TimerId` / `ThreadId`: Unique identifiers for timers and background threads -
//! `Instant` / `Duration`: Cross-platform time types (works on no_std with tick
//! counters) - `ThreadReceiver`: Channel for receiving messages from the main thread
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
// ============================================================================ User
// timers start at 0x0100 to avoid conflicts with system timers. These constants
// define well-known timer IDs for internal framework use.

/// Timer ID for cursor blinking in contenteditable elements (~530ms interval)
pub const CURSOR_BLINK_TIMER_ID: TimerId = TimerId { id: 0x0001 };
/// Timer ID for scroll momentum/inertia animation
pub const SCROLL_MOMENTUM_TIMER_ID: TimerId = TimerId { id: 0x0002 };
/// Timer ID for auto-scroll during drag operations near edges
pub const DRAG_AUTOSCROLL_TIMER_ID: TimerId = TimerId { id: 0x0003 };
/// Timer ID for tooltip show delay. Started by the platform event loop when the
/// hover target changes to a node that advertises a tooltip source (`aria-label` /
/// `alt` / `title`); fires once after `SystemStyle::input_metrics.hover_time_ms`
/// (`SPI_GETMOUSEHOVERTIME` on Windows, default 400ms) and emits a `ShowTooltip`
/// `CallbackChange`.
pub const TOOLTIP_DELAY_TIMER_ID: TimerId = TimerId { id: 0x0004 };
/// Timer ID for the single-threaded capability pump (MWA-A1). Armed by
/// `sync_capability_pump_timer` whenever a capability source needs polling or
/// draining while the app is otherwise idle (gamepad listeners, sensor listeners, an
/// active geolocation subscription).
pub const CAPABILITY_PUMP_TIMER_ID: TimerId = TimerId { id: 0x0005 };
/// Timer ID for the one-shot long-press wake-up (MWA-B12). Armed on every
/// `MouseDown` for the long-press threshold: a motionless press generates no further
/// events, so no pass would ever evaluate `detect_long_press` - this timer wakes the
/// loop exactly once at the threshold, `invoke_expired_timers` runs an event pass,
/// and the detection fires (or doesn't - moved/released holds are no-ops).
pub const LONG_PRESS_TIMER_ID: TimerId = TimerId { id: 0x0006 };

/// Reserved timer ID for the caret / selection tween driver (~16ms). Armed by the
/// shared event dispatcher whenever a text tween is in flight; the callback
/// terminates itself the tick after the tween state goes idle.
pub const CARET_TWEEN_TIMER_ID: TimerId = TimerId { id: 0x0007 };

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

// Thread IDs 0-4 are reserved for internal framework use. User threads start at
// RESERVED_THREAD_ID_COUNT.
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

/// A point in time, either from the system clock or a tick counter. Use
/// `Instant::System` on platforms with std, `Instant::Tick` on `embedded/no_std`.
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

#[cfg(feature = "std")]
std::thread_local! {
    /// Injectable test-clock offset, in milliseconds, added to every
    /// `Instant::now()` **on this thread**. Driven by the E2E `tick_ms` op.
    static TEST_CLOCK_OFFSET_MS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// Advance the injectable test clock by `ms` (E2E `tick_ms`), returning the new
/// offset. Affects only the CURRENT thread - see [`TEST_CLOCK_OFFSET_MS`].
#[cfg(feature = "std")]
#[must_use]
pub fn advance_test_clock_ms(ms: u64) -> u64 {
    TEST_CLOCK_OFFSET_MS.with(|c| {
        let next = c.get().saturating_add(ms);
        c.set(next);
        next
    })
}

/// The current test-clock offset in ms (0 unless `tick_ms` was used on this
/// thread).
#[cfg(feature = "std")]
#[must_use]
pub fn test_clock_offset_ms() -> u64 {
    TEST_CLOCK_OFFSET_MS.with(core::cell::Cell::get)
}

#[cfg(feature = "std")]
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
std::thread_local! {
    /// When set, this thread's clock is FROZEN at this instant: `Instant::now()`
    /// answers `base + TEST_CLOCK_OFFSET_MS` and real time does not flow into it at
    /// all. See [`freeze_test_clock`].
    static TEST_CLOCK_BASE: core::cell::Cell<Option<StdInstant>> =
        const { core::cell::Cell::new(None) };
}

/// Freeze this thread's clock, so engine time advances ONLY when a scenario says it
/// does (`tick_ms` / `wait`) and never because wall time passed. Offsetting alone is
/// not enough.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub fn freeze_test_clock() {
    TEST_CLOCK_BASE.with(|c| {
        if c.get().is_none() {
            c.set(Some(StdInstant::now()));
        }
    });
}

/// Whether this thread's clock is frozen (see [`freeze_test_clock`]).
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[must_use]
pub fn test_clock_is_frozen() -> bool {
    TEST_CLOCK_BASE.with(core::cell::Cell::get).is_some()
}

/// Put this thread's test clock back on real time. Worker threads are REUSED across
/// scenarios, so without this the next scenario scheduled onto this thread would
/// inherit the previous one's accumulated offset - the same cross-contamination the
/// process-global offset had, just at thread granularity.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub fn reset_test_clock() {
    TEST_CLOCK_OFFSET_MS.with(|c| c.set(0));
    TEST_CLOCK_BASE.with(|c| c.set(None));
}

/// Monotonic frame counter, and the ONLY clock a wasm build has.
/// `std::time::Instant::now()` PANICS on wasm32-unknown-unknown, and `#[cfg(feature
/// = "std")]` does not exclude wasm here: azul-core is built with `default =
/// ["std"]` for the web target, so every `std` path is compiled in.
#[cfg(feature = "std")]
static SYSTEM_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Advance the frame counter by one. Called once per produced frame by backends
/// that have no wall clock.
#[cfg(feature = "std")]
pub fn advance_system_tick() {
    SYSTEM_TICK.fetch_add(1, Ordering::Relaxed);
}

/// The current frame counter.
#[cfg(feature = "std")]
#[must_use]
pub fn system_tick_now() -> u64 {
    SYSTEM_TICK.load(Ordering::Relaxed)
}

/// `std::time::Instant::now()` shifted by the injectable test-clock offset, or -
/// when the clock is frozen - built from the frozen base so real time cannot leak
/// in. NOT COMPILED on wasm32, where `std::time::Instant::now()` panics.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
fn std_now_with_test_offset() -> StdInstant {
    let offset = test_clock_offset_ms();
    if let Some(base) = TEST_CLOCK_BASE.with(core::cell::Cell::get) {
        return base + core::time::Duration::from_millis(offset);
    }
    if offset == 0 {
        StdInstant::now()
    } else {
        StdInstant::now() + core::time::Duration::from_millis(offset)
    }
}

impl Instant {
    /// Returns the current system time. On systems with std, this uses
    /// `std::time::Instant::now()`.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[must_use] pub fn now() -> Self {
        std_now_with_test_offset().into()
    }

    /// Returns the current time on wasm32, which has no clock to read.
    /// `std::time::Instant::now()` panics on wasm32-unknown-unknown, and
    /// `#[cfg(feature = "std")]` does not exclude wasm here - azul-core is built
    /// with `default = ["std"]` for the web target, so the std path is compiled in
    /// and would trap on the first frame.
    #[cfg(all(feature = "std", target_arch = "wasm32"))]
    #[must_use] pub fn now() -> Self {
        Instant::Tick(SystemTick::new(system_tick_now()))
    }

    /// Returns the current system time (no_std fallback).
    #[cfg(not(feature = "std"))]
    pub fn now() -> Self {
        Instant::Tick(SystemTick::new(0))
    }

    /// Returns a number from 0.0 to 1.0 indicating the current linear interpolation
    /// value between (start, end)
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

        // Zero-length interval: `duration_current / duration_total` would be `0/0 =
        // NaN`. Treat a collapsed interval as fully elapsed (1.0) rather than
        // propagating NaN into animation progress.
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

    /// Adds a duration to the instant. The duration's UNIT need not match the
    /// instant's: a `Tick` duration added to a `System` instant is converted at
    /// [`TICKS_PER_SECOND`], and a `System` duration added to a `Tick` instant is
    /// converted to whole ticks.
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
                // System instant + Tick duration: convert the frame count to wall
                // time. Routed through the same `System + System` arm so the
                // overflow behaviour is identical for both units.
                (Self::System(_), Duration::Tick(_)) => {
                    self.add_optional_duration(Some(&Duration::System(
                        SystemTimeDiff::from_nanos_u128(d.as_nanos()),
                    )))
                }
                // Tick instant + System duration: convert to WHOLE ticks. A
                // sub-frame duration therefore advances nothing, which is the
                // truthful answer on a clock whose resolution is one frame.
                (Self::Tick(s), Duration::System(_)) => Self::Tick(SystemTick {
                    tick_counter: s.tick_counter.saturating_add(d.as_ticks()),
                }),
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

    /// Calculates the duration since an earlier point in time. Saturates to a zero
    /// duration in the degenerate cases (earlier is actually *later* than `self`, or
    /// the two instants are of mismatched kinds) instead of panicking - this runs on
    /// the hot event-loop path and must not crash.
    #[must_use] pub fn duration_since(&self, earlier: &Self) -> Duration {
        match (earlier, self) {
            (Self::System(prev), Self::System(now)) => {
                #[cfg(feature = "std")]
                {
                    let prev_instant: StdInstant = prev.clone().into();
                    let now_instant: StdInstant = now.clone().into();
                    // `saturating_duration_since` yields 0 if `prev` is later than
                    // `now` (monotonic-clock skew / reordered instants).
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

/// Tick-based timestamp for systems without a real-time clock. Used on embedded
/// systems where time is measured in frame ticks or cycles.
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
/// Allows crossing FFI boundaries while maintaining proper memory management.
#[repr(C)]
pub struct InstantPtr {
    /// `ManuallyDrop` so the owned `Box` is freed ONLY when `run_destructor` is
    /// still set (see `Drop`). The codegen FFI wrappers (`AzTimerCallbackInfo` etc.)
    /// embed this by value AND have their own `Drop` that `drop_in_place`s the real
    /// type first; Rust's drop glue would then drop this `ptr` field a SECOND time
    /// on the same bytes.
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
            // A second drop on the same bytes (the codegen wrapper's field-drop
            // after its own `_delete` already ran the real drop) sees
            // run_destructor=false and skips this -> no double-free.
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

/// A span of time, either from the system clock or as tick difference. Mirrors
/// `Instant` variants - System durations work with System instants, Tick durations
/// work with Tick instants.
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

/// Nominal engine tick (frame) rate - the single exchange rate between
/// [`Duration::Tick`] (frames) and [`Duration::System`] (wall time). Re-exported
/// from `azul-css` so the CSS `t` unit and the engine's `Duration` arithmetic cannot
/// drift apart.
pub use azul_css::props::basic::time::TICKS_PER_SECOND;

impl Duration {
    /// This duration on ONE canonical scale, in nanoseconds - the common ground on
    /// which a `Tick` span and a `System` span can be compared. `u128` because a
    /// `System` duration holds up to `u64::MAX` *seconds* (~1.8e28 ns), which does
    /// not fit `u64`.
    #[allow(clippy::cast_lossless)]
    #[must_use]
    pub const fn as_nanos(&self) -> u128 {
        match self {
            Self::System(s) => (s.secs as u128) * (NANOS_PER_SEC as u128) + (s.nanos as u128),
            Self::Tick(t) => (t.tick_diff as u128) * (NANOS_PER_SEC as u128) / (TICKS_PER_SECOND as u128),
        }
    }

    /// A wall-clock duration of `ms` whole milliseconds.
    #[must_use]
    pub const fn from_millis(ms: u64) -> Self {
        Self::System(SystemTimeDiff::from_millis(ms))
    }

    /// A duration of `ticks` engine frames - the clockless unit, and what the CSS
    /// `t` unit becomes.
    #[must_use]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self::Tick(SystemTickDiff { tick_diff: ticks })
    }

    /// This duration in whole ticks (frames), truncating toward zero. A sub-frame
    /// span is **zero** ticks, not one: "how many whole frames fit", never "round up
    /// so that something happens".
    #[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
    #[must_use]
    pub const fn as_ticks(&self) -> u64 {
        match self {
            Self::Tick(t) => t.tick_diff,
            Self::System(_) => {
                let ticks = self.as_nanos() * (TICKS_PER_SECOND as u128) / (NANOS_PER_SEC as u128);
                if ticks > u64::MAX as u128 {
                    u64::MAX
                } else {
                    ticks as u64
                }
            }
        }
    }

    /// This duration in whole milliseconds, truncating toward zero and saturating
    /// at `u64::MAX` rather than wrapping. `as` casts: `const fn`, so
    /// `From`/`TryFrom` are unavailable.
    #[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
    #[must_use]
    pub const fn as_millis_u64(&self) -> u64 {
        let ms = self.as_nanos() / (NANOS_PER_MILLI as u128);
        if ms > u64::MAX as u128 {
            u64::MAX
        } else {
            ms as u64
        }
    }

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

    /// Divides this duration by another, returning the ratio as f32. Same-unit
    /// division goes through the unit's own `div` so its exact floating-point result
    /// is unchanged.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    #[must_use] pub fn div(&self, other: &Self) -> f32 {
        use self::Duration::{System, Tick};
        match (self, other) {
            (System(s), System(s2)) => s.div(s2) as f32,
            (Tick(t), Tick(t2)) => t.div(t2) as f32,
            // u128 -> f64 loses precision only past 2^53 ns (~104 days), and the
            // result is a ratio that is then narrowed to f32 anyway.
            _ => (self.as_nanos() as f64 / other.as_nanos() as f64) as f32,
        }
    }

    /// Returns the smaller of two durations.
    #[must_use] pub const fn min(self, other: Self) -> Self {
        if self.smaller_than(&other) {
            self
        } else {
            other
        }
    }

    /// Returns true if self > other. Compares on the canonical nanosecond scale
    /// ([`Self::as_nanos`]), so a `Tick` span and a `System` span compare TRUTHFULLY
    /// against each other.
    #[must_use] pub const fn greater_than(&self, other: &Self) -> bool {
        self.as_nanos() > other.as_nanos()
    }

    /// Returns true if self < other. Canonical-scale comparison; see
    /// [`Self::greater_than`] for why this is unit-aware rather than saturating to
    /// `false` on a unit mismatch.
    #[must_use] pub const fn smaller_than(&self, other: &Self) -> bool {
        self.as_nanos() < other.as_nanos()
    }
}

/// Represents a difference in ticks for systems that don't support timing
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTickDiff {
    pub tick_diff: u64,
}

impl SystemTickDiff {
    /// Divide duration A by duration B. Returns `Inf` or `NaN` if `other` is zero.
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
    /// Divide duration A by duration B. Returns `Inf` or `NaN` if `other` is zero.
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
    /// Creates a duration from nanoseconds. const fn (no const TryFrom); `nanos %
    /// NANOS_PER_SEC` is always < 10^9, which fits u32, so the narrowing cast cannot
    /// truncate.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use] pub const fn from_nanos(nanos: u64) -> Self {
        Self {
            secs: nanos / (NANOS_PER_SEC as u64),
            nanos: (nanos % (NANOS_PER_SEC as u64)) as u32,
        }
    }

    /// Creates a duration from a `u128` nanosecond count, saturating at the largest
    /// representable duration instead of wrapping. Needed because
    /// [`Duration::as_nanos`] is `u128`: a tick count near `u64::MAX` converts to
    /// ~3e26 ns, far past what `u64` nanoseconds hold.
    #[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
    #[must_use] pub const fn from_nanos_u128(nanos: u128) -> Self {
        let secs = nanos / (NANOS_PER_SEC as u128);
        if secs > u64::MAX as u128 {
            Self {
                secs: u64::MAX,
                nanos: NANOS_PER_SEC - 1,
            }
        } else {
            Self {
                secs: secs as u64,
                nanos: (nanos % (NANOS_PER_SEC as u128)) as u32,
            }
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

    /// Returns the total duration in milliseconds. Saturates at `u64::MAX` instead
    /// of overflow-panicking for enormous `secs` values (`secs * 1000` overflows
    /// around ~1.8e16 seconds).
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

/// Bridge from the CSS-level duration to the engine-level one, preserving the unit.
/// This is the join that makes a CSS `5t` mean five FRAMES all the way down to the
/// timer: `ms`/`s` become `Duration::System`, `t` becomes `Duration::Tick`.
impl From<azul_css::props::basic::time::CssDuration> for Duration {
    fn from(d: azul_css::props::basic::time::CssDuration) -> Self {
        use azul_css::props::basic::time::CssDurationUnit;
        match d.unit {
            CssDurationUnit::Milliseconds => Self::from_millis(u64::from(d.inner)),
            CssDurationUnit::Ticks => Self::from_ticks(u64::from(d.inner)),
        }
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
/// Message that can be sent from the main thread to the Thread using the
/// `ThreadId`. The thread can ignore the event.
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

/// Channel endpoint for receiving messages from the main thread in a background
/// thread. Thread-safe wrapper around the receiver end of a message channel.
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

/// Get the current system type, equivalent to `std::time::Instant::now()`, except
/// it also works on systems that don't have a clock (such as embedded timers)
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;
#[repr(C)]
pub struct GetSystemTimeCallback {
    pub cb: GetSystemTimeCallbackType,
}
impl_callback_simple!(GetSystemTimeCallback);

/// Default implementation that gets the current system time. On WASM targets
/// `std::time::Instant::now()` panics, so we fall back to a zero-tick instant
/// instead.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[must_use] pub extern "C" fn get_system_time_libstd() -> Instant {
    // Honours the injectable E2E test clock (see TEST_CLOCK_OFFSET_MS).
    std_now_with_test_offset().into()
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

    /// The property the parallel E2E runner depends on: a `tick_ms` on one thread
    /// must not shift any other thread's clock. This is what allows a scenario that
    /// ticks to run in parallel with every other scenario instead of being
    /// serialised behind a process-global offset.
    #[test]
    #[cfg(feature = "std")]
    fn test_clock_offset_is_per_thread_not_process_global() {
        reset_test_clock();
        assert_eq!(test_clock_offset_ms(), 0);

        let (tx, rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let other = std::thread::spawn(move || {
            // Observed AFTER the main thread has advanced its own clock by 5 s.
            go_rx.recv().expect("handshake");
            let seen_after_main_ticked = test_clock_offset_ms();
            let _ = advance_test_clock_ms(7);
            tx.send((seen_after_main_ticked, test_clock_offset_ms()))
                .expect("send");
        });

        assert_eq!(advance_test_clock_ms(5_000), 5_000);
        go_tx.send(()).expect("handshake");
        let (other_before, other_after) = rx.recv().expect("recv");
        other.join().expect("join");

        assert_eq!(
            other_before, 0,
            "a tick on the main thread leaked into another thread's clock"
        );
        assert_eq!(other_after, 7, "the other thread must own its own offset");
        assert_eq!(
            test_clock_offset_ms(),
            5_000,
            "another thread's tick leaked into the main thread's clock"
        );

        // And a reset really is a reset (worker threads are reused).
        reset_test_clock();
        assert_eq!(test_clock_offset_ms(), 0);
    }

    /// A frozen clock must advance ONLY by what a scenario asks for. Offsetting
    /// alone left `Instant::now()` as `StdInstant::now() + offset`, so real time
    /// still flowed: elapsed time was what the scenario asked for PLUS whatever the
    /// machine happened to spend.
    #[test]
    #[cfg(feature = "std")]
    fn a_frozen_clock_advances_only_by_what_the_scenario_asks_for() {
        reset_test_clock();
        assert!(!test_clock_is_frozen());

        freeze_test_clock();
        assert!(test_clock_is_frozen());

        let t0 = Instant::now();
        // Burn REAL time. A frozen clock must not notice.
        std::thread::sleep(core::time::Duration::from_millis(25));
        let t1 = Instant::now();
        assert_eq!(
            t1.duration_since(&t0),
            Duration::System(SystemTimeDiff { secs: 0, nanos: 0 }),
            "real time leaked into a frozen clock",
        );

        // Only an explicit advance moves it, and by exactly that much.
        let _ = advance_test_clock_ms(500);
        let t2 = Instant::now();
        assert_eq!(
            t2.duration_since(&t0),
            Duration::System(SystemTimeDiff { secs: 0, nanos: 500_000_000 }),
            "a 500 ms tick must read back as exactly 500 ms",
        );

        // Freezing is idempotent: it must not re-base and lose the offset.
        freeze_test_clock();
        assert_eq!(
            Instant::now().duration_since(&t0),
            Duration::System(SystemTimeDiff { secs: 0, nanos: 500_000_000 }),
            "re-freezing re-based the clock and discarded elapsed virtual time",
        );

        // A reset must unfreeze, or the next scenario on this reused worker thread
        // would start with time stopped.
        reset_test_clock();
        assert!(!test_clock_is_frozen());
        let r0 = Instant::now();
        std::thread::sleep(core::time::Duration::from_millis(15));
        assert!(
            Instant::now().duration_since(&r0)
                > Duration::System(SystemTimeDiff { secs: 0, nanos: 0 }),
            "reset_test_clock left the clock frozen",
        );
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

    /// Cross-unit comparison must answer TRUTHFULLY, not saturate to `false`. The
    /// saturating version made a unit mismatch a permanent silent "not yet": every
    /// interval constant in the engine is a `Duration::System`, so a Tick elapsed
    /// value compared against one never expired and the UI simply stopped animating,
    /// with nothing to catch.
    #[test]
    fn duration_compare_is_unit_aware_across_ticks_and_wall_clock() {
        // 5 ticks at 60Hz is ~83ms, i.e. LESS than one second.
        let five_ticks = tick_dur(5);
        let one_second = sys_dur(1, 0);
        assert!(five_ticks.smaller_than(&one_second));
        assert!(!five_ticks.greater_than(&one_second));
        assert!(one_second.greater_than(&five_ticks));
        assert!(!one_second.smaller_than(&five_ticks));

        // 120 ticks is two seconds, i.e. MORE than one second.
        assert!(tick_dur(120).greater_than(&one_second));
        assert!(one_second.smaller_than(&tick_dur(120)));

        // Exactly 60 ticks IS one second: neither greater nor smaller.
        assert!(!tick_dur(60).greater_than(&one_second));
        assert!(!tick_dur(60).smaller_than(&one_second));
        assert!(!one_second.greater_than(&tick_dur(60)));
        assert!(!one_second.smaller_than(&tick_dur(60)));
    }

    /// `Duration::max()` is `System`, and it must still dominate every tick count -
    /// including `u64::MAX` ticks, which is a bigger *number* but a smaller span.
    #[test]
    fn duration_compare_across_units_at_the_extremes() {
        assert!(Duration::max().greater_than(&tick_dur(u64::MAX)));
        assert!(tick_dur(u64::MAX).smaller_than(&Duration::max()));
        assert!(!tick_dur(0).greater_than(&sys_dur(0, 0)));
        assert!(!sys_dur(0, 0).greater_than(&tick_dur(0)));
        // One nanosecond beats zero ticks.
        assert!(sys_dur(0, 1).greater_than(&tick_dur(0)));
    }

    #[test]
    fn add_optional_duration_converts_across_units() {
        let inst = tick(100);
        // A System duration on a Tick instant advances by WHOLE ticks: 1s = 60.
        assert_eq!(inst.add_optional_duration(Some(&sys_dur(1, 0))), tick(160));
        // Sub-frame durations advance nothing - one frame is the resolution.
        assert_eq!(inst.add_optional_duration(Some(&sys_dur(0, 1))), tick(100));
        // Matching kinds add and saturate.
        assert_eq!(inst.add_optional_duration(Some(&tick_dur(5))), tick(105));
        // Saturating add: near-max tick doesn't overflow-panic.
        let big = tick(u64::MAX);
        assert_eq!(big.add_optional_duration(Some(&tick_dur(10))), tick(u64::MAX));
        // ...and the cross-unit arm saturates too.
        assert_eq!(big.add_optional_duration(Some(&Duration::max())), tick(u64::MAX));
    }

    #[test]
    fn millis_saturates_on_overflow() {
        let huge = SystemTimeDiff { secs: u64::MAX, nanos: 0 };
        assert_eq!(huge.millis(), u64::MAX);
        let normal = SystemTimeDiff { secs: 2, nanos: 500_000_000 };
        assert_eq!(normal.millis(), 2500);
    }

    /// Cross-unit division goes through the canonical scale. Returning `0.0` (the
    /// old behaviour) reads downstream as "this animation is at 0% progress", i.e.
    #[test]
    fn duration_div_is_unit_aware() {
        // 30 ticks is half a second.
        assert!((tick_dur(30).div(&sys_dur(1, 0)) - 0.5).abs() < 1e-6);
        // ...and one second is two lots of 30 ticks.
        assert!((sys_dur(1, 0).div(&tick_dur(30)) - 2.0).abs() < 1e-6);
        // Matching Tick kinds divide normally.
        assert!((tick_dur(5).div(&tick_dur(10)) - 0.5).abs() < 1e-6);
        // Matching System kinds divide normally.
        assert!((sys_dur(1, 0).div(&sys_dur(2, 0)) - 0.5).abs() < 1e-6);
    }

    // Exercises the `unsafe` pointer work in `std_instant_clone` (`&*ptr`) and the
    // `ManuallyDrop::drop` guard in `InstantPtr::drop`: build an InstantPtr, clone
    // it (goes through the FFI clone callback + raw-ptr deref), then let both drop.
    // Under Miri this asserts the clone/drop path is UB-free and the owned `Box` is
    // freed exactly once per value (no double-free).
    #[cfg(feature = "std")]
    #[test]
    fn instant_ptr_clone_and_drop_no_ub() {
        let base = StdInstant::now();
        let a: InstantPtr = base.into();
        let b = a.clone();
        // The clone must observe the same underlying instant.
        assert_eq!(a, b);
        // Both `a` and `b` own independent Boxes; dropping both must not
        // double-free (each has run_destructor == true).
        drop(a);
        drop(b);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on ratios / interpolation results
mod autotest_generated {
    use super::*;

    // ---- helpers -----------------------------------------------------------

    fn tick(n: u64) -> Instant {
        Instant::Tick(SystemTick::new(n))
    }
    fn tick_dur(n: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: n })
    }
    fn sys_dur(secs: u64, nanos: u32) -> Duration {
        Duration::System(SystemTimeDiff { secs, nanos })
    }

    // ========================================================================
    // TimerId::unique / ThreadId::unique (monotonic, never hits reserved IDs)
    // ========================================================================

    #[test]
    fn timer_id_unique_is_strictly_increasing_and_above_reserved_range() {
        let a = TimerId::unique();
        let b = TimerId::unique();
        assert_ne!(a, b);
        assert!(b.id > a.id, "unique() must strictly increase: {a:?} -> {b:?}");
        // User IDs must never land inside the reserved system-timer block.
        for id in [a, b] {
            assert!(
                id.id >= USER_TIMER_ID_START,
                "unique() handed out a reserved system ID: {id:?}"
            );
            assert_ne!(id, CURSOR_BLINK_TIMER_ID);
            assert_ne!(id, SCROLL_MOMENTUM_TIMER_ID);
            assert_ne!(id, DRAG_AUTOSCROLL_TIMER_ID);
            assert_ne!(id, TOOLTIP_DELAY_TIMER_ID);
            assert_ne!(id, CAPABILITY_PUMP_TIMER_ID);
            assert_ne!(id, LONG_PRESS_TIMER_ID);
        }
    }

    #[test]
    fn thread_id_unique_is_strictly_increasing_and_above_reserved_range() {
        let a = ThreadId::unique();
        let b = ThreadId::unique();
        assert_ne!(a, b);
        assert!(b.id > a.id);
        assert!(a.id >= RESERVED_THREAD_ID_COUNT);
    }

    // The counters are `AtomicUsize` + `fetch_add`, so concurrent callers must
    // never be handed the same ID. 8 threads x 64 IDs => 512 distinct values.
    #[cfg(feature = "std")]
    #[test]
    fn unique_ids_do_not_collide_across_threads() {
        use alloc::collections::BTreeSet;

        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    let mut out = Vec::new();
                    for _ in 0..64 {
                        out.push((TimerId::unique().id, ThreadId::unique().id));
                    }
                    out
                })
            })
            .collect();

        let mut timer_ids = BTreeSet::new();
        let mut thread_ids = BTreeSet::new();
        for h in handles {
            for (t, th) in h.join().expect("worker thread panicked") {
                assert!(timer_ids.insert(t), "duplicate TimerId handed out: {t}");
                assert!(thread_ids.insert(th), "duplicate ThreadId handed out: {th}");
            }
        }
        assert_eq!(timer_ids.len(), 8 * 64);
        assert_eq!(thread_ids.len(), 8 * 64);
    }

    // ========================================================================
    // Instant::now / get_system_time_libstd
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn instant_now_is_system_and_monotonic() {
        let a = Instant::now();
        let b = Instant::now();
        assert!(matches!(a, Instant::System(_)));
        assert!(a <= b, "Instant::now() went backwards");
        // A later instant is never "before" an earlier one.
        assert_eq!(a.duration_since(&b), sys_dur(0, 0));
    }

    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[test]
    fn get_system_time_libstd_is_monotonic_system_instant() {
        let a = get_system_time_libstd();
        let b = get_system_time_libstd();
        assert!(matches!(a, Instant::System(_)));
        assert!(matches!(b, Instant::System(_)));
        assert!(a <= b);
    }

    #[cfg(any(not(feature = "std"), target_arch = "wasm32"))]
    #[test]
    fn get_system_time_libstd_wasm_fallback_is_zero_tick() {
        // On WASM / no_std `StdInstant::now()` would panic, so the fallback must
        // hand back a tick instant instead of exploding.
        assert_eq!(get_system_time_libstd(), tick(0));
    }

    // ========================================================================
    // Instant::linear_interpolate (must never return NaN / escape [0.0, 1.0])
    // ========================================================================

    #[test]
    fn linear_interpolate_clamps_outside_the_interval() {
        // before start -> 0.0, after end -> 1.0 (never negative / >1).
        assert_eq!(tick(0).linear_interpolate(tick(10), tick(20)), 0.0);
        assert_eq!(tick(999).linear_interpolate(tick(10), tick(20)), 1.0);
        // exactly on the boundaries
        assert_eq!(tick(10).linear_interpolate(tick(10), tick(20)), 0.0);
        assert_eq!(tick(20).linear_interpolate(tick(10), tick(20)), 1.0);
    }

    #[test]
    fn linear_interpolate_reversed_interval_is_normalized() {
        // `end < start` is swapped internally, so the ratio is the same as the
        // correctly-ordered call rather than a garbage / negative value.
        let forwards = tick(5).linear_interpolate(tick(0), tick(10));
        let backwards = tick(5).linear_interpolate(tick(10), tick(0));
        assert_eq!(forwards, backwards);
        assert!((backwards - 0.5).abs() < 1e-6);
    }

    #[test]
    fn linear_interpolate_saturating_extremes_stay_in_range() {
        // Full u64 span: the tick diff hits u64::MAX and the f64->f32 narrowing
        // must not produce inf/NaN.
        let v = tick(u64::MAX / 2).linear_interpolate(tick(0), tick(u64::MAX));
        assert!(v.is_finite(), "interpolation over the full u64 span went non-finite");
        assert!((0.0..=1.0).contains(&v));
        assert!((v - 0.5).abs() < 1e-3, "expected ~0.5, got {v}");

        // Degenerate zero-length interval at the extremes -> 1.0, not 0/0 = NaN.
        let z = tick(u64::MAX).linear_interpolate(tick(u64::MAX), tick(u64::MAX));
        assert_eq!(z, 1.0);
        let z0 = tick(0).linear_interpolate(tick(0), tick(0));
        assert_eq!(z0, 1.0);
    }

    #[cfg(feature = "std")]
    #[test]
    fn linear_interpolate_mismatched_kinds_never_nan() {
        // Every mismatched (System / Tick) permutation feeds a 0/0 division
        // internally; the guard must turn that into a finite value in [0, 1].
        let sys = Instant::now();
        let cases = [
            (tick(5), sys.clone(), tick(10)),
            (sys.clone(), tick(0), tick(10)),
            (tick(5), tick(0), sys.clone()),
            (sys.clone(), sys.clone(), tick(10)),
            (tick(5), sys.clone(), sys.clone()),
        ];
        for (this, start, end) in cases {
            let v = this.linear_interpolate(start, end);
            assert!(v.is_finite(), "mismatched-kind interpolation returned {v}");
            assert!(
                (0.0..=1.0).contains(&v),
                "mismatched-kind interpolation escaped [0,1]: {v}"
            );
        }
    }

    // ========================================================================
    // Instant::add_optional_duration
    // ========================================================================

    #[test]
    fn add_optional_duration_none_is_identity() {
        let t = tick(42);
        assert_eq!(t.add_optional_duration(None), t);
        assert_eq!(tick(u64::MAX).add_optional_duration(None), tick(u64::MAX));
    }

    #[test]
    fn add_optional_duration_tick_saturates_at_u64_max() {
        // saturating_add: u64::MAX-1 + huge must clamp, not wrap or panic.
        let near_max = tick(u64::MAX - 1);
        assert_eq!(
            near_max.add_optional_duration(Some(&tick_dur(u64::MAX))),
            tick(u64::MAX)
        );
        assert_eq!(tick(0).add_optional_duration(Some(&tick_dur(0))), tick(0));
    }

    #[cfg(feature = "std")]
    #[test]
    fn add_optional_duration_system_advances_by_the_duration() {
        let base = Instant::now();
        let later = base.add_optional_duration(Some(&Duration::System(SystemTimeDiff::from_secs(1))));
        assert!(later > base);
        let delta = later.duration_since(&base);
        assert_eq!(delta, sys_dur(1, 0));
        // ... and the reverse span saturates to zero rather than going negative.
        assert_eq!(base.duration_since(&later), sys_dur(0, 0));
    }

    /// A `Tick` interval on a wall-clock instant has to ADVANCE that instant, not
    /// leave it alone. `Timer::instant_of_next_run` is exactly `last_run + delay +
    /// interval`; when this returned `self`, a tick-unit timer's next run was always
    /// "now" - permanently overdue, and `time_until_next_timer_ms` answered
    /// `Some(0)` for it.
    #[cfg(feature = "std")]
    #[test]
    fn add_optional_duration_converts_between_units_in_both_directions() {
        let sys = Instant::now();
        // System instant + Tick duration: 60 ticks is exactly one second.
        let later = sys.add_optional_duration(Some(&tick_dur(60)));
        assert!(later > sys, "a tick interval must advance a wall-clock instant");
        assert_eq!(later.duration_since(&sys), sys_dur(1, 0));

        // 0 ticks is genuinely no time at all.
        assert_eq!(sys.add_optional_duration(Some(&tick_dur(0))), sys);

        // Tick instant + System duration: 3s is 180 whole frames.
        assert_eq!(tick(7).add_optional_duration(Some(&sys_dur(3, 0))), tick(187));
    }

    // A `System` instant plus an enormous `System` duration overflows the platform
    // clock representation: `StdInstant + StdDuration` panics with "overflow when
    // adding duration to instant". Unlike the mismatched-kind case (documented to
    // saturate), this arm has no guard -- characterized here so a future saturating
    // fix flips this test loudly.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[test]
    #[should_panic(expected = "overflow")]
    fn add_optional_duration_system_overflow_panics_today() {
        let base = Instant::now();
        let _ = base.add_optional_duration(Some(&Duration::max()));
    }

    // ========================================================================
    // Instant::duration_since / into_std_instant
    // ========================================================================

    #[test]
    fn duration_since_tick_saturates_and_is_exact() {
        assert_eq!(tick(10).duration_since(&tick(4)), tick_dur(6));
        // self == earlier -> zero span
        assert_eq!(tick(10).duration_since(&tick(10)), tick_dur(0));
        // earlier is later -> saturate to zero, no underflow panic
        assert_eq!(tick(0).duration_since(&tick(u64::MAX)), tick_dur(0));
        // full-range span does not overflow
        assert_eq!(tick(u64::MAX).duration_since(&tick(0)), tick_dur(u64::MAX));
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_since_mismatched_kinds_is_zero_tick_both_directions() {
        let sys = Instant::now();
        assert_eq!(sys.duration_since(&tick(5)), tick_dur(0));
        assert_eq!(tick(5).duration_since(&sys), tick_dur(0));
    }

    #[cfg(feature = "std")]
    #[test]
    fn into_std_instant_round_trips_a_system_instant() {
        let base = StdInstant::now();
        let wrapped: Instant = base.into();
        assert_eq!(wrapped.into_std_instant(), base);
    }

    #[cfg(feature = "std")]
    #[test]
    #[should_panic(expected = "internal error: entered unreachable code")]
    fn into_std_instant_on_tick_variant_panics() {
        // Documented: `into_std_instant` is `unreachable!()` for Tick instants.
        let _ = tick(1).into_std_instant();
    }

    // ========================================================================
    // SystemTick::new
    // ========================================================================

    #[test]
    fn system_tick_new_stores_the_counter_verbatim() {
        for n in [0_u64, 1, 0x0100, u64::MAX / 2, u64::MAX] {
            assert_eq!(SystemTick::new(n).tick_counter, n);
        }
        // Ordering follows the counter (used by Instant's derived Ord).
        assert!(SystemTick::new(0) < SystemTick::new(u64::MAX));
        assert_eq!(SystemTick::new(7), SystemTick::new(7));
    }

    // ========================================================================
    // InstantPtr: get / std_instant_clone / std_instant_drop
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn instant_ptr_get_returns_the_wrapped_instant() {
        let base = StdInstant::now();
        let p: InstantPtr = base.into();
        assert_eq!(p.get(), base);
        // `get` is a copy, not a move: repeated reads stay stable.
        assert_eq!(p.get(), p.get());
        assert!(p.run_destructor);
        // Debug must not panic and must not be empty.
        assert!(!alloc::format!("{p:?}").is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn std_instant_clone_deep_copies_and_arms_the_destructor() {
        let base = StdInstant::now();
        let a: InstantPtr = base.into();
        let cloned = std_instant_clone(core::ptr::from_ref(&a));
        assert_eq!(cloned.get(), base);
        // The clone owns its OWN box (freeing both must not double-free).
        assert!(!core::ptr::eq(&**a.ptr, &**cloned.ptr));
        assert!(cloned.run_destructor, "clone handed back a disarmed destructor");
        drop(cloned);
        // The source survives its clone being dropped.
        assert_eq!(a.get(), base);
    }

    #[cfg(feature = "std")]
    #[test]
    fn std_instant_drop_is_a_noop_even_for_null() {
        // The libstd destructor callback is deliberately empty: the Box is freed by
        // `InstantPtr::drop` under the `run_destructor` guard. Calling it with a
        // null pointer must therefore be harmless.
        std_instant_drop(core::ptr::null_mut());

        let mut p: InstantPtr = StdInstant::now().into();
        let before = p.get();
        std_instant_drop(core::ptr::from_mut(&mut p));
        // Value is untouched and still owned afterwards.
        assert_eq!(p.get(), before);
        assert!(p.run_destructor);
    }

    // ========================================================================
    // Duration::fmt (Display)
    // ========================================================================

    #[test]
    fn duration_display_tick_edge_values() {
        assert_eq!(alloc::format!("{}", tick_dur(0)), "0 ticks");
        assert_eq!(alloc::format!("{}", tick_dur(1)), "1 ticks");
        assert_eq!(
            alloc::format!("{}", tick_dur(u64::MAX)),
            "18446744073709551615 ticks"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_display_system_edge_values_do_not_panic() {
        // zero, sub-second, denormalized nanos and the absolute maximum all have to
        // format without panicking and without producing an empty string.
        for d in [
            sys_dur(0, 0),
            sys_dur(1, 500_000_000),
            sys_dur(0, u32::MAX),
            sys_dur(u64::MAX, NANOS_PER_SEC - 1),
            Duration::max(),
        ] {
            let s = alloc::format!("{d}");
            assert!(!s.is_empty());
            assert!(!s.ends_with("ticks"), "System duration formatted as ticks: {s}");
        }
    }

    // ========================================================================
    // Duration::max / div / min / greater_than / smaller_than
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn duration_max_is_the_upper_bound() {
        let m = Duration::max();
        assert_eq!(m, sys_dur(u64::MAX, NANOS_PER_SEC - 1));
        // Nothing of the same kind is greater than it...
        assert!(m.greater_than(&sys_dur(u64::MAX, NANOS_PER_SEC - 2)));
        assert!(m.greater_than(&sys_dur(0, 0)));
        // ... and it is not greater/smaller than itself.
        assert!(!m.greater_than(&m));
        assert!(!m.smaller_than(&m));
        // Converting the maximum back to std must not overflow-panic.
        let Duration::System(inner) = m else {
            panic!("Duration::max() is not a System duration under std")
        };
        assert_eq!(inner.get(), StdDuration::new(u64::MAX, NANOS_PER_SEC - 1));
    }

    #[test]
    fn duration_div_by_zero_yields_inf_or_nan_not_a_panic() {
        // 0/0 -> NaN, x/0 -> +inf. Neither may panic.
        assert!(tick_dur(0).div(&tick_dur(0)).is_nan());
        let inf = tick_dur(5).div(&tick_dur(0));
        assert!(inf.is_infinite() && inf.is_sign_positive());

        assert!(sys_dur(0, 0).div(&sys_dur(0, 0)).is_nan());
        let sinf = sys_dur(1, 0).div(&sys_dur(0, 0));
        assert!(sinf.is_infinite() && sinf.is_sign_positive());
    }

    #[test]
    fn duration_div_extremes_stay_finite_in_f32() {
        // u64::MAX / 1 ~= 1.8e19, comfortably inside f32 range: the f64 -> f32
        // narrowing must not produce inf.
        let r = tick_dur(u64::MAX).div(&tick_dur(1));
        assert!(r.is_finite(), "u64::MAX tick ratio overflowed f32: {r}");
        assert!(r > 1e19);
        // Identity ratios are exactly 1.0 for both kinds.
        assert_eq!(tick_dur(u64::MAX).div(&tick_dur(u64::MAX)), 1.0);
        assert_eq!(sys_dur(3, 0).div(&sys_dur(2, 0)), 1.5);
    }

    /// Cross-unit division converts instead of collapsing to `0.0`. 10 ticks is one
    /// sixth of a second, so the two ratios are reciprocals of each other - which is
    /// the property `0.0` both ways could never satisfy.
    #[test]
    fn duration_div_across_kinds_converts_both_ways() {
        assert!((sys_dur(1, 0).div(&tick_dur(10)) - 6.0).abs() < 1e-5);
        assert!((tick_dur(10).div(&sys_dur(1, 0)) - (1.0 / 6.0)).abs() < 1e-5);
    }

    #[test]
    fn duration_min_picks_the_smaller_of_the_same_kind() {
        assert_eq!(tick_dur(5).min(tick_dur(10)), tick_dur(5));
        assert_eq!(tick_dur(10).min(tick_dur(5)), tick_dur(5));
        assert_eq!(tick_dur(7).min(tick_dur(7)), tick_dur(7));
        assert_eq!(tick_dur(0).min(tick_dur(u64::MAX)), tick_dur(0));
        // Comparison no longer needs std: it is u128 nanosecond arithmetic.
        assert_eq!(sys_dur(1, 0).min(sys_dur(1, 1)), sys_dur(1, 0));
    }

    /// `min` is built on `smaller_than`, so it picks the genuinely shorter span
    /// across units and is COMMUTATIVE. It used to just return `other` whenever the
    /// units differed - so `a.min(b)` and `b.min(a)` disagreed, and the answer
    /// depended on argument order rather than on the durations.
    #[test]
    fn duration_min_across_kinds_picks_the_genuinely_shorter_span() {
        // 5 ticks is ~83ms, so it is shorter than a second either way round.
        assert_eq!(tick_dur(5).min(sys_dur(1, 0)), tick_dur(5));
        assert_eq!(sys_dur(1, 0).min(tick_dur(5)), tick_dur(5));
        // ...and 120 ticks is 2s, so the second wins either way round.
        assert_eq!(tick_dur(120).min(sys_dur(1, 0)), sys_dur(1, 0));
        assert_eq!(sys_dur(1, 0).min(tick_dur(120)), sys_dur(1, 0));
    }

    #[test]
    fn duration_comparison_is_a_strict_total_order_within_a_kind() {
        let mut pairs = alloc::vec![(tick_dur(0), tick_dur(u64::MAX)), (tick_dur(1), tick_dur(2))];
        // System ordering no longer needs std: the comparison is u128 nanosecond
        // arithmetic. It used to defer to `StdDuration`, so on no_std it answered
        // `false` for every System pair - a total order that ordered nothing.
        pairs.extend_from_slice(&[
            (sys_dur(0, 0), sys_dur(u64::MAX, 0)),
            (sys_dur(1, 999_999_999), sys_dur(2, 0)),
        ]);

        for (a, b) in pairs {
            assert!(a.smaller_than(&b));
            assert!(b.greater_than(&a));
            assert!(!a.greater_than(&b));
            assert!(!b.smaller_than(&a));
        }
        // Equal values: neither greater nor smaller (holds for both kinds).
        let eq = tick_dur(4);
        assert!(!eq.greater_than(&eq));
        assert!(!eq.smaller_than(&eq));
        let eq_sys = sys_dur(4, 2);
        assert!(!eq_sys.greater_than(&eq_sys));
        assert!(!eq_sys.smaller_than(&eq_sys));
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_comparison_normalizes_denormalized_nanos() {
        // nanos == u32::MAX (> 1e9) is denormalized; the std conversion carries it
        // into secs, so {0, u32::MAX} == 4.294967295s > 4s.
        let denorm = sys_dur(0, u32::MAX);
        assert!(denorm.greater_than(&sys_dur(4, 0)));
        assert!(denorm.smaller_than(&sys_dur(5, 0)));
    }

    // ========================================================================
    // SystemTickDiff::div / SystemTimeDiff::div + as_secs_f64
    // ========================================================================

    #[test]
    fn system_tick_diff_div_edge_cases() {
        let zero = SystemTickDiff { tick_diff: 0 };
        let one = SystemTickDiff { tick_diff: 1 };
        let max = SystemTickDiff { tick_diff: u64::MAX };

        assert!(zero.div(&zero).is_nan());
        assert!(one.div(&zero).is_infinite());
        assert_eq!(zero.div(&one), 0.0);
        assert_eq!(max.div(&max), 1.0);
        assert!(max.div(&one).is_finite());
        assert_eq!(SystemTickDiff { tick_diff: 5 }.div(&SystemTickDiff { tick_diff: 10 }), 0.5);
    }

    #[test]
    fn system_time_diff_as_secs_f64_is_exact_for_representable_values() {
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 0 }.as_secs_f64(), 0.0);
        assert_eq!(SystemTimeDiff { secs: 1, nanos: 500_000_000 }.as_secs_f64(), 1.5);
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 500_000_000 }.as_secs_f64(), 0.5);
        // Extremes stay finite (u64::MAX secs ~= 1.8e19, well inside f64).
        let huge = SystemTimeDiff { secs: u64::MAX, nanos: NANOS_PER_SEC - 1 };
        assert!(huge.as_secs_f64().is_finite());
        assert!(huge.as_secs_f64() > 1e19);
        // Monotone in secs.
        assert!(
            SystemTimeDiff::from_secs(2).as_secs_f64() > SystemTimeDiff::from_secs(1).as_secs_f64()
        );
    }

    #[test]
    fn system_time_diff_div_edge_cases() {
        let zero = SystemTimeDiff { secs: 0, nanos: 0 };
        let one = SystemTimeDiff::from_secs(1);
        let half = SystemTimeDiff { secs: 0, nanos: 500_000_000 };

        assert!(zero.div(&zero).is_nan());
        assert!(one.div(&zero).is_infinite());
        assert_eq!(zero.div(&one), 0.0);
        assert_eq!(one.div(&one), 1.0);
        assert_eq!(one.div(&half), 2.0);
        let max = SystemTimeDiff { secs: u64::MAX, nanos: NANOS_PER_SEC - 1 };
        assert_eq!(max.div(&max), 1.0);
        assert!(max.div(&one).is_finite());
    }

    // ========================================================================
    // SystemTimeDiff constructors: from_secs / from_millis / from_nanos
    // ========================================================================

    #[test]
    fn from_secs_invariants() {
        for s in [0_u64, 1, 1_000, u64::MAX] {
            let d = SystemTimeDiff::from_secs(s);
            assert_eq!(d.secs, s);
            assert_eq!(d.nanos, 0, "from_secs must leave nanos at zero");
        }
    }

    #[test]
    fn from_millis_normalizes_and_keeps_nanos_in_range() {
        assert_eq!(SystemTimeDiff::from_millis(0), SystemTimeDiff { secs: 0, nanos: 0 });
        assert_eq!(
            SystemTimeDiff::from_millis(999),
            SystemTimeDiff { secs: 0, nanos: 999_000_000 }
        );
        assert_eq!(SystemTimeDiff::from_millis(1_000), SystemTimeDiff { secs: 1, nanos: 0 });
        assert_eq!(
            SystemTimeDiff::from_millis(1_500),
            SystemTimeDiff { secs: 1, nanos: 500_000_000 }
        );
        // u64::MAX millis must not overflow the u32 nanos field.
        let max = SystemTimeDiff::from_millis(u64::MAX);
        assert!(max.nanos < NANOS_PER_SEC, "from_millis produced denormalized nanos");
        assert_eq!(max.secs, u64::MAX / MILLIS_PER_SEC);
    }

    #[test]
    fn from_nanos_normalizes_and_keeps_nanos_in_range() {
        assert_eq!(SystemTimeDiff::from_nanos(0), SystemTimeDiff { secs: 0, nanos: 0 });
        assert_eq!(
            SystemTimeDiff::from_nanos(999_999_999),
            SystemTimeDiff { secs: 0, nanos: 999_999_999 }
        );
        assert_eq!(
            SystemTimeDiff::from_nanos(1_000_000_000),
            SystemTimeDiff { secs: 1, nanos: 0 }
        );
        for n in [0_u64, 1, 999_999_999, 1_000_000_001, u64::MAX] {
            let d = SystemTimeDiff::from_nanos(n);
            assert!(d.nanos < NANOS_PER_SEC, "from_nanos({n}) produced denormalized nanos");
            // Lossless round-trip: secs * 1e9 + nanos == n (checked in u128).
            let back =
                u128::from(d.secs) * u128::from(NANOS_PER_SEC) + u128::from(d.nanos);
            assert_eq!(back, u128::from(n), "from_nanos({n}) lost information");
        }
    }

    // ========================================================================
    // Round-trip: from_millis <-> millis
    // ========================================================================

    #[test]
    fn millis_round_trips_through_from_millis() {
        // Exact for every whole-millisecond value, INCLUDING u64::MAX (where `secs
        // * 1000 + 615` lands exactly on u64::MAX without saturating).
        for m in [0_u64, 1, 999, 1_000, 1_500, 86_400_000, u64::MAX] {
            assert_eq!(
                SystemTimeDiff::from_millis(m).millis(),
                m,
                "from_millis({m}).millis() is not lossless"
            );
        }
    }

    #[test]
    fn millis_truncates_and_saturates_instead_of_panicking() {
        // Sub-millisecond nanos truncate towards zero.
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 999_999 }.millis(), 0);
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 999_999_999 }.millis(), 999);
        // secs * 1000 overflows u64 -> saturate at u64::MAX, no panic.
        assert_eq!(SystemTimeDiff { secs: u64::MAX, nanos: 0 }.millis(), u64::MAX);
        assert_eq!(
            SystemTimeDiff { secs: u64::MAX, nanos: NANOS_PER_SEC - 1 }.millis(),
            u64::MAX
        );
        assert_eq!(SystemTimeDiff::from_secs(u64::MAX / 1_000).millis(), (u64::MAX / 1_000) * 1_000);
    }

    // ========================================================================
    // SystemTimeDiff::checked_add
    // ========================================================================

    #[test]
    fn checked_add_carries_nanos_into_secs() {
        let a = SystemTimeDiff { secs: 0, nanos: 999_999_999 };
        let sum = a.checked_add(a).expect("0.999s + 0.999s must not overflow");
        assert_eq!(sum, SystemTimeDiff { secs: 1, nanos: 999_999_998 });
        // Exactly one second of nanos carries cleanly.
        let b = SystemTimeDiff { secs: 1, nanos: 500_000_000 };
        assert_eq!(
            b.checked_add(b),
            Some(SystemTimeDiff { secs: 3, nanos: 0 })
        );
    }

    #[test]
    fn checked_add_returns_none_on_overflow_instead_of_panicking() {
        let max_secs = SystemTimeDiff { secs: u64::MAX, nanos: 0 };
        // secs overflow
        assert_eq!(max_secs.checked_add(SystemTimeDiff::from_secs(1)), None);
        // secs at max, nanos still fit -> Some
        assert_eq!(
            max_secs.checked_add(SystemTimeDiff { secs: 0, nanos: NANOS_PER_SEC - 1 }),
            Some(SystemTimeDiff { secs: u64::MAX, nanos: NANOS_PER_SEC - 1 })
        );
        // overflow that only happens because of the nanos CARRY
        let brim = SystemTimeDiff { secs: u64::MAX, nanos: NANOS_PER_SEC - 1 };
        assert_eq!(brim.checked_add(SystemTimeDiff { secs: 0, nanos: 1 }), None);
    }

    #[test]
    fn checked_add_identity_and_commutativity() {
        let zero = SystemTimeDiff { secs: 0, nanos: 0 };
        for d in [
            SystemTimeDiff::from_secs(0),
            SystemTimeDiff::from_millis(1_500),
            SystemTimeDiff::from_nanos(u64::MAX),
            SystemTimeDiff { secs: u64::MAX, nanos: 0 },
        ] {
            assert_eq!(d.checked_add(zero), Some(d));
            assert_eq!(zero.checked_add(d), Some(d));
            // a + b == b + a for well-formed operands
            let other = SystemTimeDiff::from_millis(750);
            assert_eq!(d.checked_add(other), other.checked_add(d));
        }
    }

    // ========================================================================
    // SystemTimeDiff::get (std::time::Duration conversion round-trip)
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn system_time_diff_get_round_trips_std_duration() {
        for std_d in [
            StdDuration::ZERO,
            StdDuration::from_millis(1_500),
            StdDuration::from_nanos(1),
            StdDuration::new(u64::MAX, NANOS_PER_SEC - 1),
        ] {
            let mid: SystemTimeDiff = std_d.into();
            assert_eq!(mid.get(), std_d, "StdDuration -> SystemTimeDiff -> StdDuration lost data");
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn system_time_diff_get_on_edge_values_does_not_panic() {
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 0 }.get(), StdDuration::ZERO);
        // secs at max with zero nanos: no carry, so no overflow in Duration::new.
        assert_eq!(
            SystemTimeDiff::from_secs(u64::MAX).get(),
            StdDuration::new(u64::MAX, 0)
        );
        // Denormalized nanos (>= 1e9) are carried by Duration::new, not rejected.
        assert_eq!(
            SystemTimeDiff { secs: 0, nanos: u32::MAX }.get(),
            StdDuration::new(0, u32::MAX)
        );
    }

    // ========================================================================
    // ThreadReceiver: new / get_ctx / recv / clone
    // ========================================================================

    #[cfg(feature = "std")]
    extern "C" fn test_thread_recv(ptr: *const c_void) -> OptionThreadSendMsg {
        // Mirrors the real callback: `ThreadReceiver::recv` hands over a pointer to
        // the boxed `Receiver<ThreadSendMsg>` inside `ThreadReceiverInner`.
        let receiver = unsafe { &*(ptr.cast::<Receiver<ThreadSendMsg>>()) };
        receiver.try_recv().ok().into()
    }

    #[cfg(feature = "std")]
    const extern "C" fn test_thread_recv_destructor(_: *mut ThreadReceiverInner) {}

    #[cfg(feature = "std")]
    fn test_receiver() -> (Sender<ThreadSendMsg>, ThreadReceiver) {
        let (tx, rx) = std::sync::mpsc::channel::<ThreadSendMsg>();
        let inner = ThreadReceiverInner {
            ptr: Box::new(rx),
            recv_fn: ThreadRecvCallback { cb: test_thread_recv },
            destructor: ThreadReceiverDestructorCallback {
                cb: test_thread_recv_destructor,
            },
        };
        (tx, ThreadReceiver::new(inner))
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_new_arms_destructor_and_has_no_ctx() {
        let (_tx, r) = test_receiver();
        assert!(r.run_destructor, "ThreadReceiver::new left the destructor disarmed");
        assert!(r.get_ctx().is_none(), "a fresh receiver must have no FFI context");
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_recv_on_empty_and_disconnected_channel_is_none() {
        let (tx, mut r) = test_receiver();
        // Empty channel -> None (must not block / panic).
        assert!(r.recv().is_none());
        // Disconnected channel -> still None, not a panic.
        drop(tx);
        assert!(r.recv().is_none());
        assert!(r.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_recv_delivers_messages_in_order() {
        let (tx, mut r) = test_receiver();
        tx.send(ThreadSendMsg::Tick).unwrap();
        tx.send(ThreadSendMsg::Custom(RefAny::new(42_u32))).unwrap();
        tx.send(ThreadSendMsg::TerminateThread).unwrap();

        assert_eq!(r.recv(), OptionThreadSendMsg::Some(ThreadSendMsg::Tick));
        assert!(matches!(
            r.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::Custom(_))
        ));
        assert_eq!(
            r.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        );
        // Drained.
        assert!(r.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_clone_shares_the_same_channel() {
        let (tx, mut a) = test_receiver();
        let mut b = a.clone();
        assert!(b.run_destructor);

        tx.send(ThreadSendMsg::Tick).unwrap();
        // The clone shares the Arc<Mutex<..>>: whichever half receives first
        // consumes the message; the other must see an empty channel, not a duplicate
        // and not a deadlock.
        assert_eq!(a.recv(), OptionThreadSendMsg::Some(ThreadSendMsg::Tick));
        assert!(b.recv().is_none());

        tx.send(ThreadSendMsg::TerminateThread).unwrap();
        assert_eq!(
            b.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        );
        assert!(a.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_get_ctx_clones_rather_than_takes() {
        let (_tx, mut r) = test_receiver();
        r.ctx = OptionRefAny::Some(RefAny::new(7_u64));
        // Repeated reads must all succeed -- `get_ctx` clones the RefAny (refcount
        // bump); a take/move would leave the second call empty.
        assert!(r.get_ctx().is_some());
        assert!(r.get_ctx().is_some());
        let held = r.get_ctx();
        drop(r);
        // The cloned handle outlives the receiver it came from.
        assert!(held.is_some());
    }
}
