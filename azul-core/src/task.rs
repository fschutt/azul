use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::{
    ffi::c_void,
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(feature = "std")]
use std::sync::mpsc::{Receiver, Sender};
#[cfg(feature = "std")]
use std::thread::{self, JoinHandle};
#[cfg(feature = "std")]
use std::time::Duration as StdDuration;
#[cfg(feature = "std")]
use std::time::Instant as StdInstant;

use std::sync::Mutex;

use crate::gl::OptionGlContextPtr;
use crate::{
    app_resources::{ImageCache, ImageMask, ImageRef},
    callbacks::{
        CallbackInfo, DomNodeId, FocusTarget, OptionDomNodeId, RefAny, ScrollPosition,
        ThreadCallback, TimerCallback, TimerCallbackInfo, TimerCallbackReturn, TimerCallbackType,
        Update, WriteBackCallback, WriteBackCallbackType,
    },
    id_tree::NodeId,
    styled_dom::{DomId, NodeHierarchyItemId},
    ui_solver::LayoutResult,
    window::{
        FullWindowState, LogicalPosition, OptionLogicalPosition, RawWindowHandle,
        WindowCreateOptions, WindowState,
    },
    FastBTreeSet, FastHashMap,
};
use azul_css::{AzString, CssProperty};
use rust_fontconfig::FcFontCache;

/// Should a timer terminate or not - used to remove active timers
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TerminateTimer {
    /// Remove the timer from the list of active timers
    Terminate,
    /// Do nothing and let the timers continue to run
    Continue,
}

static MAX_TIMER_ID: AtomicUsize = AtomicUsize::new(5);

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerId {
    pub id: usize,
}

impl TimerId {
    /// Generates a new, unique `TimerId`.
    pub fn unique() -> Self {
        TimerId {
            id: MAX_TIMER_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

impl_option!(
    TimerId,
    OptionTimerId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

static MAX_THREAD_ID: AtomicUsize = AtomicUsize::new(5);

/// ID for uniquely identifying a timer
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

impl ThreadId {
    /// Generates a new, unique `ThreadId`.
    pub fn unique() -> Self {
        ThreadId {
            id: MAX_THREAD_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Instant {
    System(AzInstantPtr),
    Tick(SystemTick),
}

#[cfg(feature = "std")]
impl From<StdInstant> for Instant {
    fn from(s: StdInstant) -> Instant {
        Instant::System(s.into())
    }
}

impl Instant {
    /// Returns a number from 0.0 to 1.0 indicating the current
    /// linear interpolation value between (start, end)
    pub fn linear_interpolate(&self, mut start: Self, mut end: Self) -> f32 {
        use std::mem;

        if end < start {
            mem::swap(&mut start, &mut end);
        }

        if *self < start {
            return 0.0;
        }
        if *self > end {
            return 1.0;
        }

        let duration_total = end.duration_since(&start);
        let duration_current = self.duration_since(&start);

        duration_current.div(&duration_total).max(0.0).min(1.0)
    }

    /// Adds a duration to the instant, does nothing in undefined cases
    /// (i.e. trying to add a Duration::Tick to an Instant::System)
    pub fn add_optional_duration(&self, duration: Option<&Duration>) -> Self {
        match duration {
            Some(d) => match (self, d) {
                (Instant::System(i), Duration::System(d)) => {
                    #[cfg(feature = "std")]
                    {
                        let s: StdInstant = i.clone().into();
                        let d: StdDuration = d.clone().into();
                        let new: AzInstantPtr = (s + d).into();
                        Instant::System(new)
                    }
                    #[cfg(not(feature = "std"))]
                    {
                        unreachable!()
                    }
                }
                (Instant::Tick(s), Duration::Tick(d)) => Instant::Tick(SystemTick {
                    tick_counter: s.tick_counter + d.tick_diff,
                }),
                _ => {
                    panic!(
                        "invalid: trying to add a duration {:?} to an instant {:?}",
                        d, self
                    );
                }
            },
            None => self.clone(),
        }
    }

    #[cfg(feature = "std")]
    pub fn into_std_instant(self) -> StdInstant {
        match self {
            Instant::System(s) => s.into(),
            Instant::Tick(_) => unreachable!(),
        }
    }

    /// Calculates the duration since an earlier point in time
    ///
    /// - Panics if the earlier Instant was created after the current Instant
    /// - Panics if the two enums do not have the same variant (tick / std)
    pub fn duration_since(&self, earlier: &Instant) -> Duration {
        match (earlier, self) {
            (Instant::System(prev), Instant::System(now)) => {
                #[cfg(feature = "std")]
                {
                    let prev_instant: StdInstant = prev.clone().into();
                    let now_instant: StdInstant = now.clone().into();
                    Duration::System((now_instant.duration_since(prev_instant)).into())
                }
                #[cfg(not(feature = "std"))]
                {
                    unreachable!() // cannot construct a SystemTime on no_std
                }
            }
            (
                Instant::Tick(SystemTick { tick_counter: prev }),
                Instant::Tick(SystemTick { tick_counter: now }),
            ) => {
                if prev > now {
                    panic!("illegal: subtraction 'Instant - Instant' would result in a negative duration")
                } else {
                    Duration::Tick(SystemTickDiff {
                        tick_diff: now - prev,
                    })
                }
            }
            _ => panic!(
                "illegal: trying to calculate a Duration from a SystemTime and a Tick instant"
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTick {
    pub tick_counter: u64,
}

impl SystemTick {
    pub const fn new(tick_counter: u64) -> Self {
        Self { tick_counter }
    }
}

#[repr(C)]
pub struct AzInstantPtr {
    #[cfg(feature = "std")]
    pub ptr: Box<StdInstant>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub clone_fn: InstantPtrCloneCallback,
    pub destructor: InstantPtrDestructorCallback,
    pub run_destructor: bool,
}

pub type InstantPtrCloneCallbackType = extern "C" fn(*const AzInstantPtr) -> AzInstantPtr;
#[repr(C)]
pub struct InstantPtrCloneCallback {
    pub cb: InstantPtrCloneCallbackType,
}
impl_callback!(InstantPtrCloneCallback);

pub type InstantPtrDestructorCallbackType = extern "C" fn(*mut AzInstantPtr);
#[repr(C)]
pub struct InstantPtrDestructorCallback {
    pub cb: InstantPtrDestructorCallbackType,
}
impl_callback!(InstantPtrDestructorCallback);

// ----  LIBSTD implementation for AzInstantPtr BEGIN
#[cfg(feature = "std")]
impl core::fmt::Debug for AzInstantPtr {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{:?}", self.get())
    }
}

#[cfg(not(feature = "std"))]
impl core::fmt::Debug for AzInstantPtr {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{:?}", self.ptr as usize)
    }
}

#[cfg(feature = "std")]
impl core::hash::Hash for AzInstantPtr {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

#[cfg(not(feature = "std"))]
impl core::hash::Hash for AzInstantPtr {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.ptr as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for AzInstantPtr {
    fn eq(&self, other: &AzInstantPtr) -> bool {
        self.get() == other.get()
    }
}

#[cfg(not(feature = "std"))]
impl PartialEq for AzInstantPtr {
    fn eq(&self, other: &AzInstantPtr) -> bool {
        (self.ptr as usize).eq(&(other.ptr as usize))
    }
}

impl Eq for AzInstantPtr {}

#[cfg(feature = "std")]
impl PartialOrd for AzInstantPtr {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.get()).cmp(&(other.get())))
    }
}

#[cfg(not(feature = "std"))]
impl PartialOrd for AzInstantPtr {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.ptr as usize).cmp(&(other.ptr as usize)))
    }
}

#[cfg(feature = "std")]
impl Ord for AzInstantPtr {
    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
        (self.get()).cmp(&(other.get()))
    }
}

#[cfg(not(feature = "std"))]
impl Ord for AzInstantPtr {
    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
        (self.ptr as usize).cmp(&(other.ptr as usize))
    }
}

#[cfg(feature = "std")]
impl AzInstantPtr {
    fn get(&self) -> StdInstant {
        *(self.ptr).clone()
    }
}

impl Clone for AzInstantPtr {
    fn clone(&self) -> Self {
        (self.clone_fn.cb)(self)
    }
}

#[cfg(feature = "std")]
extern "C" fn std_instant_clone(ptr: *const AzInstantPtr) -> AzInstantPtr {
    let az_instant_ptr = unsafe { &*ptr };
    AzInstantPtr {
        ptr: az_instant_ptr.ptr.clone(),
        clone_fn: az_instant_ptr.clone_fn.clone(),
        destructor: az_instant_ptr.destructor.clone(),
        run_destructor: true,
    }
}

#[cfg(feature = "std")]
impl From<StdInstant> for AzInstantPtr {
    fn from(s: StdInstant) -> AzInstantPtr {
        Self {
            ptr: Box::new(s),
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
impl From<AzInstantPtr> for StdInstant {
    fn from(s: AzInstantPtr) -> StdInstant {
        s.get()
    }
}

impl Drop for AzInstantPtr {
    fn drop(&mut self) {
        self.run_destructor = false;
        (self.destructor.cb)(self);
    }
}

#[cfg(feature = "std")]
extern "C" fn std_instant_drop(_: *mut AzInstantPtr) {}

// ----  LIBSTD implementation for AzInstantPtr END

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Duration {
    System(SystemTimeDiff),
    Tick(SystemTickDiff),
}

impl core::fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Duration::System(s) => {
                let s: StdDuration = s.clone().into();
                write!(f, "{:?}", s)
            }
            #[cfg(not(feature = "std"))]
            Duration::System(s) => write!(f, "({}s, {}ns)", s.secs, s.nanos),
            Duration::Tick(tick) => write!(f, "{} ticks", tick.tick_diff),
        }
    }
}

#[cfg(feature = "std")]
impl From<StdDuration> for Duration {
    fn from(s: StdDuration) -> Self {
        Duration::System(s.into())
    }
}

impl Duration {
    pub fn max() -> Self {
        #[cfg(feature = "std")]
        {
            Duration::System(StdDuration::new(core::u64::MAX, NANOS_PER_SEC - 1).into())
        }
        #[cfg(not(feature = "std"))]
        {
            Duration::Tick(SystemTickDiff {
                tick_diff: u64::MAX,
            })
        }
    }

    pub fn div(&self, other: &Self) -> f32 {
        use self::Duration::*;
        match (self, other) {
            (System(s), System(s2)) => s.div(s2) as f32,
            (Tick(t), Tick(t2)) => t.div(t2) as f32,
            _ => 0.0,
        }
    }

    pub fn min(self, other: Self) -> Self {
        if self.smaller_than(&other) {
            self
        } else {
            other
        }
    }

    #[allow(unused_variables)]
    pub fn greater_than(&self, other: &Self) -> bool {
        match (self, other) {
            // self > other
            (Duration::System(s), Duration::System(o)) => {
                #[cfg(feature = "std")]
                {
                    let s: StdDuration = s.clone().into();
                    let o: StdDuration = o.clone().into();
                    s > o
                }
                #[cfg(not(feature = "std"))]
                {
                    unreachable!()
                }
            }
            (Duration::Tick(s), Duration::Tick(o)) => s.tick_diff > o.tick_diff,
            _ => {
                panic!("illegal: trying to compare a SystemDuration with a TickDuration");
            }
        }
    }

    #[allow(unused_variables)]
    pub fn smaller_than(&self, other: &Self) -> bool {
        // self < other
        match (self, other) {
            // self > other
            (Duration::System(s), Duration::System(o)) => {
                #[cfg(feature = "std")]
                {
                    let s: StdDuration = s.clone().into();
                    let o: StdDuration = o.clone().into();
                    s < o
                }
                #[cfg(not(feature = "std"))]
                {
                    unreachable!()
                }
            }
            (Duration::Tick(s), Duration::Tick(o)) => s.tick_diff < o.tick_diff,
            _ => {
                panic!("illegal: trying to compare a SystemDuration with a TickDuration");
            }
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
    /// Divide duration A by duration B
    pub fn div(&self, other: &Self) -> f64 {
        self.tick_diff as f64 / other.tick_diff as f64
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTimeDiff {
    pub secs: u64,
    pub nanos: u32,
}

impl SystemTimeDiff {
    /// Divide duration A by duration B
    pub fn div(&self, other: &Self) -> f64 {
        self.as_secs_f64() / other.as_secs_f64()
    }
    fn as_secs_f64(&self) -> f64 {
        (self.secs as f64) + ((self.nanos as f64) / (NANOS_PER_SEC as f64))
    }
}

#[cfg(feature = "std")]
impl From<StdDuration> for SystemTimeDiff {
    fn from(d: StdDuration) -> SystemTimeDiff {
        SystemTimeDiff {
            secs: d.as_secs(),
            nanos: d.subsec_nanos(),
        }
    }
}

#[cfg(feature = "std")]
impl From<SystemTimeDiff> for StdDuration {
    fn from(d: SystemTimeDiff) -> StdDuration {
        StdDuration::new(d.secs, d.nanos)
    }
}

const MILLIS_PER_SEC: u64 = 1_000;
const NANOS_PER_MILLI: u32 = 1_000_000;
const NANOS_PER_SEC: u32 = 1_000_000_000;

impl SystemTimeDiff {
    pub const fn from_secs(secs: u64) -> Self {
        SystemTimeDiff { secs, nanos: 0 }
    }
    pub const fn from_millis(millis: u64) -> Self {
        SystemTimeDiff {
            secs: millis / MILLIS_PER_SEC,
            nanos: ((millis % MILLIS_PER_SEC) as u32) * NANOS_PER_MILLI,
        }
    }
    pub const fn from_nanos(nanos: u64) -> Self {
        SystemTimeDiff {
            secs: nanos / (NANOS_PER_SEC as u64),
            nanos: (nanos % (NANOS_PER_SEC as u64)) as u32,
        }
    }
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
            Some(SystemTimeDiff { secs, nanos })
        } else {
            None
        }
    }

    pub fn millis(&self) -> u64 {
        (self.secs * MILLIS_PER_SEC) + (self.nanos / NANOS_PER_MILLI) as u64
    }

    #[cfg(feature = "std")]
    pub fn get(&self) -> StdDuration {
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

/// A `Timer` is a function that is run on every frame.
///
/// There are often a lot of visual Threads such as animations or fetching the
/// next frame for a GIF or video, etc. - that need to run every frame or every X milliseconds,
/// but they aren't heavy enough to warrant creating a thread - otherwise the framework
/// would create too many threads, which leads to a lot of context switching and bad performance.
///
/// The callback of a `Timer` should be fast enough to run under 16ms,
/// otherwise running timers will block the main UI thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    /// Data that is internal to the timer
    pub data: RefAny,
    /// Optional node that the timer is attached to - timers attached to a DOM node
    /// will be automatically stopped when the UI is recreated.
    pub node_id: OptionDomNodeId,
    /// Stores when the timer was created (usually acquired by `Instant::now()`)
    pub created: Instant,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: OptionInstant,
    /// How many times the callback was run
    pub run_count: usize,
    /// If the timer shouldn't start instantly, but rather be delayed by a certain timeframe
    pub delay: OptionDuration,
    /// How frequently the timer should run, i.e. set this to `Some(Duration::from_millis(16))`
    /// to run the timer every 16ms. If this value is set to `None`, (the default), the timer
    /// will execute the timer as-fast-as-possible (i.e. at a faster framerate
    /// than the framework itself) - which might be  performance intensive.
    pub interval: OptionDuration,
    /// When to stop the timer (for example, you can stop the
    /// execution after 5s using `Some(Duration::from_secs(5))`).
    pub timeout: OptionDuration,
    /// Callback to be called for this timer
    pub callback: TimerCallback,
}

impl Timer {
    /// Create a new timer
    pub fn new(
        data: RefAny,
        callback: TimerCallbackType,
        get_system_time_fn: GetSystemTimeCallback,
    ) -> Self {
        Timer {
            data,
            node_id: None.into(),
            created: (get_system_time_fn.cb)(),
            run_count: 0,
            last_run: OptionInstant::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: TimerCallback { cb: callback },
        }
    }

    pub fn tick_millis(&self) -> u64 {
        match self.interval.as_ref() {
            Some(Duration::System(s)) => s.millis(),
            Some(Duration::Tick(s)) => s.tick_diff,
            None => 10, // ms
        }
    }

    /// Returns true ONCE on the LAST invocation of the timer
    /// This is useful if you want to run some animation and then
    /// when the timer finishes (i.e. all animations finish),
    /// rebuild the UI / DOM (so that the user does not notice any dropped frames).
    pub fn is_about_to_finish(&self, instant_now: &Instant) -> bool {
        let mut finish = false;
        if let OptionDuration::Some(timeout) = self.timeout {
            finish = instant_now
                .duration_since(&self.created)
                .greater_than(&timeout);
        }
        finish
    }

    /// Returns when the timer needs to run again
    pub fn instant_of_next_run(&self) -> Instant {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s,
            None => &self.created,
        };

        last_run
            .clone()
            .add_optional_duration(self.delay.as_ref())
            .add_optional_duration(self.interval.as_ref())
    }

    /// Delays the timer to not start immediately but rather
    /// start after a certain time frame has elapsed.
    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    /// Converts the timer into a timer, running the function only
    /// if the given `Duration` has elapsed since the last run
    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    /// Converts the timer into a countdown, by giving it a maximum duration
    /// (counted from the creation of the Timer, not the first use).
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }

    /// Crate-internal: Invokes the timer if the timer should run. Otherwise returns `Update::DoNothing`
    pub fn invoke(
        &mut self,
        callback_info: CallbackInfo,
        frame_start: Instant,
        get_system_time_fn: GetSystemTimeCallback,
    ) -> TimerCallbackReturn {
        let instant_now = (get_system_time_fn.cb)();

        if let OptionDuration::Some(interval) = self.interval {
            let last_run = match self.last_run.as_ref() {
                Some(s) => s.clone(),
                None => self.created.add_optional_duration(self.delay.as_ref()),
            };

            if instant_now
                .duration_since(&last_run)
                .smaller_than(&interval)
            {
                return TimerCallbackReturn {
                    should_update: Update::DoNothing,
                    should_terminate: TerminateTimer::Continue,
                };
            }
        }

        let run_count = self.run_count;
        let is_about_to_finish = self.is_about_to_finish(&instant_now);
        let mut timer_callback_info = TimerCallbackInfo {
            callback_info,
            node_id: self.node_id,
            frame_start,
            call_count: run_count,
            is_about_to_finish,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        };
        let mut res = (self.callback.cb)(&mut self.data, &mut timer_callback_info);

        // Check if the timers timeout is reached
        if is_about_to_finish {
            res.should_terminate = TerminateTimer::Terminate;
        }

        self.last_run = OptionInstant::Some(instant_now);
        self.run_count += 1;

        res
    }
}

/// Message that can be sent from the main thread to the Thread using the ThreadId.
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

// Message that is received from the running thread
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(Update),
}

impl_option!(
    ThreadReceiveMsg,
    OptionThreadReceiveMsg,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadWriteBackMsg {
    // The data to write back into. Will be passed as the second argument to the thread
    pub data: RefAny,
    // The callback to call on this data.
    pub callback: WriteBackCallback,
}

impl ThreadWriteBackMsg {
    pub fn new(callback: WriteBackCallbackType, data: RefAny) -> Self {
        Self {
            data,
            callback: WriteBackCallback { cb: callback },
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct ThreadSender {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadSenderInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

impl Clone for ThreadSender {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for ThreadSender {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl ThreadSender {
    pub fn new(t: ThreadSenderInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
        }
    }

    // send data from the user thread to the main thread
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        let ts = match self.ptr.lock().ok() {
            Some(s) => s,
            None => return false,
        };
        (ts.send_fn.cb)(ts.ptr.as_ref() as *const _ as *const c_void, msg)
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct ThreadReceiver {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadReceiverInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

impl Clone for ThreadReceiver {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for ThreadReceiver {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl ThreadReceiver {
    pub fn new(t: ThreadReceiverInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
        }
    }

    // receive data from the main thread
    pub fn recv(&mut self) -> OptionThreadSendMsg {
        let ts = match self.ptr.lock().ok() {
            Some(s) => s,
            None => return None.into(),
        };
        (ts.recv_fn.cb)(ts.ptr.as_ref() as *const _ as *const c_void)
    }
}

#[derive(Debug)]
#[cfg_attr(not(feature = "std"), derive(PartialEq, PartialOrd, Eq, Ord))]
#[repr(C)]
pub struct ThreadSenderInner {
    #[cfg(feature = "std")]
    pub ptr: Box<Sender<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub send_fn: ThreadSendCallback,
    pub destructor: ThreadSenderDestructorCallback,
}

#[cfg(not(feature = "std"))]
unsafe impl Send for ThreadSenderInner {}

#[cfg(feature = "std")]
impl core::hash::Hash for ThreadSenderInner {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.ptr.as_ref() as *const _ as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadSenderInner {
    fn eq(&self, other: &Self) -> bool {
        (self.ptr.as_ref() as *const _ as usize) == (other.ptr.as_ref() as *const _ as usize)
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadSenderInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadSenderInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (self.ptr.as_ref() as *const _ as usize)
                .cmp(&(other.ptr.as_ref() as *const _ as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadSenderInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.ptr.as_ref() as *const _ as usize).cmp(&(other.ptr.as_ref() as *const _ as usize))
    }
}

impl Drop for ThreadSenderInner {
    fn drop(&mut self) {
        (self.destructor.cb)(self);
    }
}

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
        (self.ptr.as_ref() as *const _ as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadReceiverInner {
    fn eq(&self, other: &Self) -> bool {
        (self.ptr.as_ref() as *const _ as usize) == (other.ptr.as_ref() as *const _ as usize)
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadReceiverInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadReceiverInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (self.ptr.as_ref() as *const _ as usize)
                .cmp(&(other.ptr.as_ref() as *const _ as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadReceiverInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.ptr.as_ref() as *const _ as usize).cmp(&(other.ptr.as_ref() as *const _ as usize))
    }
}

impl Drop for ThreadReceiverInner {
    fn drop(&mut self) {
        (self.destructor.cb)(self);
    }
}

/// Config that is necessary so that threading + animations can compile on no_std
///
/// See the `default` implementations in this module for an example on how to
/// create a thread
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ExternalSystemCallbacks {
    pub create_thread_fn: CreateThreadCallback,
    pub get_system_time_fn: GetSystemTimeCallback,
}

#[cfg(feature = "std")]
impl ExternalSystemCallbacks {
    pub fn rust_internal() -> Self {
        Self {
            create_thread_fn: CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: GetSystemTimeCallback {
                cb: get_system_time_libstd,
            },
        }
    }
}

/// Function that creates a new `Thread` object
pub type CreateThreadCallbackType = extern "C" fn(RefAny, RefAny, ThreadCallback) -> Thread;
#[repr(C)]
pub struct CreateThreadCallback {
    pub cb: CreateThreadCallbackType,
}
impl_callback!(CreateThreadCallback);

/// Get the current system type, equivalent to `std::time::Instant::now()`, except it
/// also works on systems that don't have a clock (such as embedded timers)
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;
#[repr(C)]
pub struct GetSystemTimeCallback {
    pub cb: GetSystemTimeCallbackType,
}
impl_callback!(GetSystemTimeCallback);

// function called to check if the thread has finished
pub type CheckThreadFinishedCallbackType =
    extern "C" fn(/* dropcheck */ *const c_void) -> bool;
#[repr(C)]
pub struct CheckThreadFinishedCallback {
    pub cb: CheckThreadFinishedCallbackType,
}
impl_callback!(CheckThreadFinishedCallback);

// function to send a message to the thread
pub type LibrarySendThreadMsgCallbackType =
    extern "C" fn(/* Sender<ThreadSendMsg> */ *const c_void, ThreadSendMsg) -> bool; // return true / false on success / failure
#[repr(C)]
pub struct LibrarySendThreadMsgCallback {
    pub cb: LibrarySendThreadMsgCallbackType,
}
impl_callback!(LibrarySendThreadMsgCallback);

// function to receive a message from the thread
pub type LibraryReceiveThreadMsgCallbackType =
    extern "C" fn(/* Receiver<ThreadReceiveMsg> */ *const c_void) -> OptionThreadReceiveMsg;
#[repr(C)]
pub struct LibraryReceiveThreadMsgCallback {
    pub cb: LibraryReceiveThreadMsgCallbackType,
}
impl_callback!(LibraryReceiveThreadMsgCallback);

// function that the RUNNING THREAD can call to receive messages from the main thread
pub type ThreadRecvCallbackType =
    extern "C" fn(/* receiver.ptr */ *const c_void) -> OptionThreadSendMsg;
#[repr(C)]
pub struct ThreadRecvCallback {
    pub cb: ThreadRecvCallbackType,
}
impl_callback!(ThreadRecvCallback);

// function that the RUNNING THREAD can call to send messages to the main thread
pub type ThreadSendCallbackType =
    extern "C" fn(/* sender.ptr */ *const c_void, ThreadReceiveMsg) -> bool; // return false on error
#[repr(C)]
pub struct ThreadSendCallback {
    pub cb: ThreadSendCallbackType,
}
impl_callback!(ThreadSendCallback);

// function called on Thread::drop()
pub type ThreadDestructorCallbackType = extern "C" fn(*mut ThreadInner);
#[repr(C)]
pub struct ThreadDestructorCallback {
    pub cb: ThreadDestructorCallbackType,
}
impl_callback!(ThreadDestructorCallback);

// destructor of the ThreadReceiver
pub type ThreadReceiverDestructorCallbackType = extern "C" fn(*mut ThreadReceiverInner);
#[repr(C)]
pub struct ThreadReceiverDestructorCallback {
    pub cb: ThreadReceiverDestructorCallbackType,
}
impl_callback!(ThreadReceiverDestructorCallback);

// destructor of the ThreadSender
pub type ThreadSenderDestructorCallbackType = extern "C" fn(*mut ThreadSenderInner);
#[repr(C)]
pub struct ThreadSenderDestructorCallback {
    pub cb: ThreadSenderDestructorCallbackType,
}
impl_callback!(ThreadSenderDestructorCallback);

/// Wrapper around Thread because Thread needs to be clone-able for Python
#[derive(Debug)]
#[repr(C)]
pub struct Thread {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

impl Clone for Thread {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl Thread {
    pub fn new(ti: ThreadInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(ti))),
            run_destructor: true,
        }
    }
}

impl ThreadInner {
    /// Returns true if the Thread has been finished, false otherwise
    pub(crate) fn is_finished(&self) -> bool {
        (self.check_thread_finished_fn.cb)(self.dropcheck.as_ref() as *const _ as *const c_void)
    }

    pub(crate) fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        (self.send_thread_msg_fn.cb)(self.sender.as_ref() as *const _ as *const c_void, msg)
    }

    pub(crate) fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        (self.receive_thread_msg_fn.cb)(self.receiver.as_ref() as *const _ as *const c_void)
    }
}

/// A `Thread` is a seperate thread that is owned by the framework.
///
/// In difference to a `Thread`, you don't have to `await()` the result of a `Thread`,
/// you can just hand the Thread to the framework (via `RendererResources::add_Thread`) and
/// the framework will automatically update the UI when the Thread is finished.
/// This is useful to offload actions such as loading long files, etc. to a background thread.
///
/// Azul will join the thread automatically after it is finished (joining won't block the UI).
#[derive(Debug)]
#[repr(C)]
pub struct ThreadInner {
    // Thread handle of the currently in-progress Thread
    #[cfg(feature = "std")]
    pub thread_handle: Box<Option<JoinHandle<()>>>,
    #[cfg(not(feature = "std"))]
    pub thread_handle: *const c_void,

    #[cfg(feature = "std")]
    pub sender: Box<Sender<ThreadSendMsg>>,
    #[cfg(not(feature = "std"))]
    pub sender: *const c_void,

    #[cfg(feature = "std")]
    pub receiver: Box<Receiver<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub receiver: *const c_void,

    #[cfg(feature = "std")]
    pub dropcheck: Box<Weak<()>>,
    #[cfg(not(feature = "std"))]
    pub dropcheck: *const c_void,

    pub writeback_data: RefAny,
    pub check_thread_finished_fn: CheckThreadFinishedCallback,
    pub send_thread_msg_fn: LibrarySendThreadMsgCallback,
    pub receive_thread_msg_fn: LibraryReceiveThreadMsgCallback,
    pub thread_destructor_fn: ThreadDestructorCallback,
}

#[cfg(feature = "std")]
pub extern "C" fn get_system_time_libstd() -> Instant {
    StdInstant::now().into()
}

#[cfg(feature = "std")]
pub extern "C" fn create_thread_libstd(
    thread_initialize_data: RefAny,
    writeback_data: RefAny,
    callback: ThreadCallback,
) -> Thread {
    let (sender_receiver, receiver_receiver) = std::sync::mpsc::channel::<ThreadReceiveMsg>();
    let sender_receiver = ThreadSender::new(ThreadSenderInner {
        ptr: Box::new(sender_receiver),
        send_fn: ThreadSendCallback {
            cb: default_send_thread_msg_fn,
        },
        destructor: ThreadSenderDestructorCallback {
            cb: thread_sender_drop,
        },
    });

    let (sender_sender, receiver_sender) = std::sync::mpsc::channel::<ThreadSendMsg>();
    let receiver_sender = ThreadReceiver::new(ThreadReceiverInner {
        ptr: Box::new(receiver_sender),
        recv_fn: ThreadRecvCallback {
            cb: default_receive_thread_msg_fn,
        },
        destructor: ThreadReceiverDestructorCallback {
            cb: thread_receiver_drop,
        },
    });

    let thread_check = Arc::new(());
    let dropcheck = Arc::downgrade(&thread_check);

    let thread_handle = Some(thread::spawn(move || {
        let _ = thread_check;
        (callback.cb)(thread_initialize_data, sender_receiver, receiver_sender);
        // thread_check gets dropped here, signals that the thread has finished
    }));

    let thread_handle: Box<Option<JoinHandle<()>>> = Box::new(thread_handle);
    let sender: Box<Sender<ThreadSendMsg>> = Box::new(sender_sender);
    let receiver: Box<Receiver<ThreadReceiveMsg>> = Box::new(receiver_receiver);
    let dropcheck: Box<Weak<()>> = Box::new(dropcheck);

    Thread::new(ThreadInner {
        thread_handle,
        sender,
        receiver,
        writeback_data,
        dropcheck,
        thread_destructor_fn: ThreadDestructorCallback {
            cb: default_thread_destructor_fn,
        },
        check_thread_finished_fn: CheckThreadFinishedCallback {
            cb: default_check_thread_finished,
        },
        send_thread_msg_fn: LibrarySendThreadMsgCallback {
            cb: library_send_thread_msg_fn,
        },
        receive_thread_msg_fn: LibraryReceiveThreadMsgCallback {
            cb: library_receive_thread_msg_fn,
        },
    })
}

impl Drop for ThreadInner {
    fn drop(&mut self) {
        (self.thread_destructor_fn.cb)(self);
    }
}

#[cfg(feature = "std")]
extern "C" fn default_thread_destructor_fn(thread: *mut ThreadInner) {
    let thread = unsafe { &mut *thread };

    if let Some(thread_handle) = thread.thread_handle.take() {
        let _ = thread.sender.send(ThreadSendMsg::TerminateThread);
        let _ = thread_handle.join(); // ignore the result, don't panic
    }
}

#[cfg(feature = "std")]
extern "C" fn library_send_thread_msg_fn(sender: *const c_void, msg: ThreadSendMsg) -> bool {
    unsafe { &*(sender as *const Sender<ThreadSendMsg>) }
        .send(msg)
        .is_ok()
}

#[cfg(feature = "std")]
extern "C" fn library_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadReceiveMsg {
    unsafe { &*(receiver as *const Receiver<ThreadReceiveMsg>) }
        .try_recv()
        .ok()
        .into()
}

#[cfg(feature = "std")]
extern "C" fn default_send_thread_msg_fn(sender: *const c_void, msg: ThreadReceiveMsg) -> bool {
    unsafe { &*(sender as *const Sender<ThreadReceiveMsg>) }
        .send(msg)
        .is_ok()
}

#[cfg(feature = "std")]
extern "C" fn default_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadSendMsg {
    unsafe { &*(receiver as *const Receiver<ThreadSendMsg>) }
        .try_recv()
        .ok()
        .into()
}

#[cfg(feature = "std")]
extern "C" fn default_check_thread_finished(dropcheck: *const c_void) -> bool {
    unsafe { &*(dropcheck as *const Weak<()>) }
        .upgrade()
        .is_none()
}

#[cfg(feature = "std")]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(feature = "std")]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}
