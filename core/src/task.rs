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

/// Reserved timer ID for the caret / selection tween driver (~16ms).
///
/// Armed by the shared event dispatcher whenever a text tween is in flight; the
/// callback terminates itself the tick after the tween state goes idle.
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

impl_vec!(
    TimerId,
    TimerIdVec,
    TimerIdVecDestructor,
    TimerIdVecDestructorType,
    TimerIdVecSlice,
    OptionTimerId
);
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

impl_vec!(
    ThreadId,
    ThreadIdVec,
    ThreadIdVecDestructor,
    ThreadIdVecDestructorType,
    ThreadIdVecSlice,
    OptionThreadId
);
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

#[cfg(feature = "std")]
std::thread_local! {
    /// Injectable test-clock offset, in milliseconds, added to every
    /// `Instant::now()` **on this thread**.
    ///
    /// Driven by the E2E `tick_ms` op. Everything time-driven in the engine —
    /// scroll momentum, scrollbar fade, cursor blink, animations, timers —
    /// reads the clock through `Instant::now()` / `get_system_time_libstd()`,
    /// so advancing this offset moves all of them forward by exactly N ms
    /// WITHOUT sleeping. That is what makes "drive the animation to completion
    /// and assert it converges" deterministic instead of a `wait { ms }` race.
    ///
    /// Zero in production; only the debug-server `tick_ms` op ever writes it.
    ///
    /// # Why this is a thread-local and not a `static AtomicU64`
    ///
    /// It used to be process-global, which made the clock a shared mutable
    /// resource: every scenario that ticked had to run SERIALLY, or scenario
    /// A's `tick_ms` would shift scenario B's animations mid-frame. Since the
    /// corpus is dominated by idle/animation scenarios, that serialised
    /// essentially the whole suite.
    ///
    /// The read path is [`GetSystemTimeCallbackType`] — a bare
    /// `extern "C" fn() -> Instant` in the public C API — plus ~140 direct
    /// `Instant::now()` calls. Neither can carry a window, an app or a clock
    /// handle without either breaking the C ABI for every language binding or
    /// threading a time source through every call site including `no_std`
    /// ones. A thread-local is the narrowest scope a context-free C callback
    /// can read: it turns "the whole process" into "the thread that owns this
    /// scenario", which is exactly the ownership boundary the parallel E2E
    /// runner already establishes (one scenario runs start-to-finish on one
    /// worker thread). [`reset_test_clock`] makes that boundary explicit.
    static TEST_CLOCK_OFFSET_MS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// Advance the injectable test clock by `ms` (E2E `tick_ms`), returning the new
/// offset. Affects only the CURRENT thread — see [`TEST_CLOCK_OFFSET_MS`].
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
    /// answers `base + TEST_CLOCK_OFFSET_MS` and real time does not flow into it
    /// at all. See [`freeze_test_clock`].
    static TEST_CLOCK_BASE: core::cell::Cell<Option<StdInstant>> =
        const { core::cell::Cell::new(None) };
}

/// Freeze this thread's clock, so engine time advances ONLY when a scenario says
/// it does (`tick_ms` / `wait`) and never because wall time passed.
///
/// Offsetting alone is not enough. `Instant::now()` was
/// `StdInstant::now() + offset`, so the REAL component still flowed and every
/// time-driven behaviour rode on however long the machine happened to take:
/// elapsed = (exact virtual) + (whatever this build, under this load, spent
/// computing). The E2E suite runs 8 scenarios per core, so that second term is
/// both large and variable, and an assertion on a blinking caret's phase would
/// flip between runs on a loaded runner while passing every time in isolation.
///
/// Frozen, engine time becomes a pure function of the ops a scenario executed —
/// identical on a debug build, a release build and a saturated CI box. That is
/// also what makes an off-by-one in animation timing *observable*: advance
/// exactly one interval and the frame either flipped or it did not, with no
/// jitter to hide behind.
///
/// This deliberately does NOT touch [`Instant::Tick`]. Interval constants are
/// built as `Duration::System` (e.g. the cursor blink in `text_edit`), and
/// `Duration::greater_than` compares only matching variants — handing the engine
/// `Tick` elapsed values against `System` intervals would mismatch and silently
/// answer "not yet" forever. Freezing keeps every existing comparison intact.
///
/// Idempotent: re-freezing an already-frozen clock keeps the original base, so
/// the offset stays the single source of elapsed time.
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

/// Put this thread's test clock back on real time.
///
/// Worker threads are REUSED across scenarios, so without this the next
/// scenario scheduled onto this thread would inherit the previous one's
/// accumulated offset — the same cross-contamination the process-global
/// offset had, just at thread granularity. The E2E runner calls this at the
/// start of every scenario. Clears the freeze as well, so a scenario cannot
/// leave the next one's clock stopped.
#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
pub fn reset_test_clock() {
    TEST_CLOCK_OFFSET_MS.with(|c| c.set(0));
    TEST_CLOCK_BASE.with(|c| c.set(None));
}

/// Monotonic frame counter, and the ONLY clock a wasm build has.
///
/// `std::time::Instant::now()` PANICS on wasm32-unknown-unknown, and
/// `#[cfg(feature = "std")]` does not exclude wasm here: azul-core is built with
/// `default = ["std"]` for the web target, so every `std` path is compiled in.
///
/// Answering `Tick(0)` forever would stop the panic and freeze every animation
/// instead — a silent stall, which is worse than a loud crash. So the web build
/// gets a real monotonic source: the browser drives redraw, each produced DOM
/// patch is one frame, and one frame is exactly what a `t` (tick) duration
/// counts. `AzStartup_buildPatch` calls [`advance_system_tick`] once per patch.
#[cfg(feature = "std")]
static SYSTEM_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Advance the frame counter by one. Called once per produced frame by backends
/// that have no wall clock. Cheap enough to call unconditionally.
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

/// `std::time::Instant::now()` shifted by the injectable test-clock offset, or —
/// when the clock is frozen — built from the frozen base so real time cannot
/// leak in.
///
/// NOT COMPILED on wasm32, where `std::time::Instant::now()` panics.
///
/// `web_lift` is deliberately NOT included here. That backend compiles natively
/// and is lifted to wasm afterwards, so `target_arch` reads `x86_64` — but the
/// lift walks the LLVM graph and auto-inserts calls out to JS for things like
/// time, so it supplies its own clock and does not want this arm disabled.
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
    /// Returns the current system time.
    ///
    /// On systems with std, this uses `std::time::Instant::now()`.
    /// On `no_std` systems, this returns a zero tick.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[must_use]
    pub fn now() -> Self {
        std_now_with_test_offset().into()
    }

    /// Returns the current time on wasm32, which has no clock to read.
    ///
    /// `std::time::Instant::now()` panics on wasm32-unknown-unknown, and
    /// `#[cfg(feature = "std")]` does not exclude wasm here — azul-core is built
    /// with `default = ["std"]` for the web target, so the std path is compiled
    /// in and would trap on the first frame.
    ///
    /// This deliberately does NOT answer a constant `Tick(0)`. That stops the
    /// panic and freezes every animation instead, which is a silent stall — the
    /// worse failure of the two. The browser drives redraw and each produced DOM
    /// patch is one frame, so the frame counter IS the clock, and a frame is
    /// exactly what a `t` (tick) duration counts. Elapsed values come out as
    /// `Tick` and convert against `System` intervals through
    /// `Duration::as_nanos`, so `60t` compares equal to one second.
    #[cfg(all(feature = "std", target_arch = "wasm32"))]
    #[must_use]
    pub fn now() -> Self {
        Instant::Tick(SystemTick::new(system_tick_now()))
    }

    /// Returns the current system time (no_std fallback).
    #[cfg(not(feature = "std"))]
    pub fn now() -> Self {
        Instant::Tick(SystemTick::new(0))
    }

    /// Returns a number from 0.0 to 1.0 indicating the current
    /// linear interpolation value between (start, end)
    #[must_use]
    pub fn linear_interpolate(&self, mut start: Self, mut end: Self) -> f32 {
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

    /// Adds a duration to the instant.
    ///
    /// The duration's UNIT need not match the instant's: a `Tick` duration added
    /// to a `System` instant is converted at [`TICKS_PER_SECOND`], and a `System`
    /// duration added to a `Tick` instant is converted to whole ticks.
    ///
    /// # Why the mismatch is converted rather than dropped
    ///
    /// This used to return `self` unchanged for a unit mismatch, which turned a
    /// `Duration::Tick` interval on a wall-clock timer into a schedule point of
    /// `last_run + 0` — `Timer::instant_of_next_run` is literally
    /// `last_run + delay + interval`, so the timer reported itself permanently
    /// overdue and `LayoutWindow::time_until_next_timer_ms` answered `Some(0)`
    /// for it, i.e. "block for zero milliseconds" to any loop that consults it.
    ///
    /// `System + System` still overflow-panics on an absurd duration (that is
    /// `StdInstant`'s own behaviour, characterised in the tests); the tick arms
    /// saturate.
    #[must_use]
    pub fn add_optional_duration(&self, duration: Option<&Duration>) -> Self {
        duration.map_or_else(
            || self.clone(),
            |d| match (self, d) {
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
                (Self::System(_), Duration::Tick(_)) => self.add_optional_duration(Some(
                    &Duration::System(SystemTimeDiff::from_nanos_u128(d.as_nanos())),
                )),
                // Tick instant + System duration: convert to WHOLE ticks. A
                // sub-frame duration therefore advances nothing, which is the
                // truthful answer on a clock whose resolution is one frame.
                (Self::Tick(s), Duration::System(_)) => Self::Tick(SystemTick {
                    tick_counter: s.tick_counter.saturating_add(d.as_ticks()),
                }),
            },
        )
    }

    /// Converts to `std::time::Instant` (panics if Tick variant).
    #[cfg(feature = "std")]
    #[must_use]
    pub fn into_std_instant(self) -> StdInstant {
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
    #[must_use]
    pub fn duration_since(&self, earlier: &Self) -> Duration {
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
    #[must_use]
    pub const fn new(tick_counter: u64) -> Self {
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
            // SAFETY: `run_destructor` is set false above, so this arm runs at
            // most once per InstantPtr value; the `Box` inside was never moved
            // out, so it is live and owned here and safe to drop exactly once.
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

/// Nominal engine tick (frame) rate — the single exchange rate between
/// [`Duration::Tick`] (frames) and [`Duration::System`] (wall time).
///
/// Re-exported from `azul-css` so the CSS `t` unit and the engine's `Duration`
/// arithmetic cannot drift apart. See [`azul_css::props::basic::time::TICKS_PER_SECOND`].
pub use azul_css::props::basic::time::TICKS_PER_SECOND;

impl Duration {
    /// This duration on ONE canonical scale, in nanoseconds — the common ground
    /// on which a `Tick` span and a `System` span can be compared.
    ///
    /// `u128` because a `System` duration holds up to `u64::MAX` *seconds*
    /// (~1.8e28 ns), which does not fit `u64`. The tick conversion multiplies
    /// before it divides so whole seconds stay exact: `60t` is `1_000_000_000`ns,
    /// not `60 * 16_666_666 = 999_999_960`ns.
    ///
    /// Note this also normalises a DENORMALISED `SystemTimeDiff` (`nanos` past
    /// `1e9`) the same way `std::time::Duration::new` would — except it cannot
    /// panic on overflow while doing it.
    // `as u128` rather than `u128::from`: this is a `const fn` and `From` is not
    // const. Every one of these widenings is lossless.
    #[allow(clippy::cast_lossless)]
    #[must_use]
    pub const fn as_nanos(&self) -> u128 {
        match self {
            Self::System(s) => (s.secs as u128) * (NANOS_PER_SEC as u128) + (s.nanos as u128),
            Self::Tick(t) => {
                (t.tick_diff as u128) * (NANOS_PER_SEC as u128) / (TICKS_PER_SECOND as u128)
            }
        }
    }

    /// A wall-clock duration of `ms` whole milliseconds.
    #[must_use]
    pub const fn from_millis(ms: u64) -> Self {
        Self::System(SystemTimeDiff::from_millis(ms))
    }

    /// A duration of `ticks` engine frames — the clockless unit, and what the
    /// CSS `t` unit becomes.
    #[must_use]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self::Tick(SystemTickDiff { tick_diff: ticks })
    }

    /// This duration in whole ticks (frames), truncating toward zero.
    ///
    /// A sub-frame span is **zero** ticks, not one: "how many whole frames fit",
    /// never "round up so that something happens".
    // `as` casts: `const fn`, so `From`/`TryFrom` are unavailable. The widenings
    // are lossless and the u128 -> u64 narrowing is range-checked immediately
    // above it.
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

    /// This duration in whole milliseconds, truncating toward zero and
    /// saturating at `u64::MAX` rather than wrapping.
    // `as` casts: `const fn`, so `From`/`TryFrom` are unavailable. The u128 ->
    // u64 narrowing is range-checked immediately above it.
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
    #[must_use]
    pub fn max() -> Self {
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
    ///
    /// Same-unit division goes through the unit's own `div` so its exact
    /// floating-point result is unchanged. Cross-unit division falls back to the
    /// canonical nanosecond scale rather than returning `0.0` — a `0.0` ratio
    /// here means "animation is at 0% progress", which is a frozen animation, not
    /// an error anyone would notice.
    // the f64 ratio is intentionally narrowed to the f32 return type; the value
    // is a duration ratio, far inside f32's range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    #[must_use]
    pub fn div(&self, other: &Self) -> f32 {
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
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        if self.smaller_than(&other) {
            self
        } else {
            other
        }
    }

    /// Returns true if self > other.
    ///
    /// Compares on the canonical nanosecond scale ([`Self::as_nanos`]), so a
    /// `Tick` span and a `System` span compare TRUTHFULLY against each other.
    ///
    /// # Why this is not "mismatched kinds saturate to false"
    ///
    /// It used to be. That made a unit mismatch invisible and permanent instead
    /// of loud: the engine's interval constants are `Duration::System` (the
    /// cursor blink, the scrollbar fade, the tooltip delay), so the moment a
    /// clock produced `Tick` elapsed values every one of those comparisons
    /// answered "not yet" — forever. Nothing panicked, nothing logged, the UI
    /// simply stopped animating. That is precisely the failure a clockless unit
    /// is supposed to make *catchable*, so the comparison has to be total.
    ///
    /// Three behaviours changed, all in the safe direction:
    ///
    /// 1. Cross-unit comparisons now answer, instead of always `false`.
    /// 2. On `no_std` the `System`/`System` arm used to be hardcoded `false`
    ///    (there was no `StdDuration` to defer to); it now compares properly.
    /// 3. A denormalised `SystemTimeDiff` whose `secs + nanos/1e9` overflows
    ///    `u64` used to panic inside `StdDuration::new`; `u128` nanoseconds
    ///    cannot overflow.
    #[must_use]
    pub const fn greater_than(&self, other: &Self) -> bool {
        self.as_nanos() > other.as_nanos()
    }

    /// Returns true if self < other.
    ///
    /// Canonical-scale comparison; see [`Self::greater_than`] for why this is
    /// unit-aware rather than saturating to `false` on a unit mismatch.
    #[must_use]
    pub const fn smaller_than(&self, other: &Self) -> bool {
        self.as_nanos() < other.as_nanos()
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
    #[must_use]
    pub fn div(&self, other: &Self) -> f64 {
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
    #[must_use]
    pub fn div(&self, other: &Self) -> f64 {
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
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }
    /// Creates a duration from milliseconds.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self {
            secs: millis / MILLIS_PER_SEC,
            nanos: ((millis % MILLIS_PER_SEC) as u32) * NANOS_PER_MILLI,
        }
    }
    /// Creates a duration from nanoseconds.
    // const fn (no const TryFrom); `nanos % NANOS_PER_SEC` is always < 10^9, which
    // fits u32, so the narrowing cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self {
            secs: nanos / (NANOS_PER_SEC as u64),
            nanos: (nanos % (NANOS_PER_SEC as u64)) as u32,
        }
    }

    /// Creates a duration from a `u128` nanosecond count, saturating at the
    /// largest representable duration instead of wrapping.
    ///
    /// Needed because [`Duration::as_nanos`] is `u128`: a tick count near
    /// `u64::MAX` converts to ~3e26 ns, far past what `u64` nanoseconds hold.
    // `nanos % NANOS_PER_SEC` is always < 10^9 and `secs` is range-checked above,
    // so neither narrowing cast can truncate. `as` widenings rather than `From`
    // because this is a `const fn`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_lossless)]
    #[must_use]
    pub const fn from_nanos_u128(nanos: u128) -> Self {
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
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
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
    #[must_use]
    pub const fn millis(&self) -> u64 {
        self.secs
            .saturating_mul(MILLIS_PER_SEC)
            .saturating_add((self.nanos / NANOS_PER_MILLI) as u64)
    }

    /// Converts to `std::time::Duration`.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn get(&self) -> StdDuration {
        (*self).into()
    }
}

/// Bridge from the CSS-level duration to the engine-level one, preserving the
/// unit.
///
/// This is the join that makes a CSS `5t` mean five FRAMES all the way down to
/// the timer: `ms`/`s` become `Duration::System`, `t` becomes `Duration::Tick`.
/// Collapsing ticks to milliseconds here would put the wall clock back in the
/// path and make "advance exactly 5 ticks, assert the 5th frame flipped"
/// untestable again.
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
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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
    #[must_use]
    pub fn new(t: ThreadReceiverInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
            ctx: OptionRefAny::None,
        }
    }

    /// Get the FFI context (e.g., Python callable)
    #[must_use]
    pub fn get_ctx(&self) -> OptionRefAny {
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
        (std::ptr::from_ref(self.ptr.as_ref()) as usize)
            .cmp(&(std::ptr::from_ref(other.ptr.as_ref()) as usize))
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
#[must_use]
pub extern "C" fn get_system_time_libstd() -> Instant {
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
