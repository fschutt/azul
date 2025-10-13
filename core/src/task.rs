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
    refany::RefAny,
    resources::{ImageCache, ImageMask, ImageRef},
    styled_dom::NodeHierarchyItemId,
    window::RawWindowHandle,
    FastBTreeSet, FastHashMap,
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
                    panic!(
                        "illegal: subtraction 'Instant - Instant' would result in a negative \
                         duration"
                    )
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
    #[cfg(not(feature = "std"))]
    pub fn new(t: ThreadReceiverInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
        }
    }

    #[cfg(feature = "std")]
    pub fn new(t: ThreadReceiverInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn recv(&mut self) -> OptionThreadSendMsg {
        None.into()
    }

    // receive data from the main thread
    #[cfg(feature = "std")]
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

/// Get the current system type, equivalent to `std::time::Instant::now()`, except it
/// also works on systems that don't have a clock (such as embedded timers)
pub type GetSystemTimeCallbackType = extern "C" fn() -> Instant;
#[repr(C)]
pub struct GetSystemTimeCallback {
    pub cb: GetSystemTimeCallbackType,
}
impl_callback!(GetSystemTimeCallback);

/// Default implementation that gets the current system time
#[cfg(feature = "std")]
pub extern "C" fn get_system_time_libstd() -> Instant {
    StdInstant::now().into()
}

/// Default implementation for systems without a clock
#[cfg(not(feature = "std"))]
pub extern "C" fn get_system_time_libstd() -> Instant {
    Instant::Tick(SystemTick::new(0))
}

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

// function that the RUNNING THREAD can call to receive messages from the main thread
pub type ThreadRecvCallbackType =
    extern "C" fn(/* receiver.ptr */ *const c_void) -> OptionThreadSendMsg;
#[repr(C)]
pub struct ThreadRecvCallback {
    pub cb: ThreadRecvCallbackType,
}
impl_callback!(ThreadRecvCallback);

// destructor of the ThreadReceiver
pub type ThreadReceiverDestructorCallbackType = extern "C" fn(*mut ThreadReceiverInner);
#[repr(C)]
pub struct ThreadReceiverDestructorCallback {
    pub cb: ThreadReceiverDestructorCallbackType,
}
impl_callback!(ThreadReceiverDestructorCallback);
