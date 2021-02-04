use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
use alloc::vec::Vec;
#[cfg(feature = "std")]
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
#[cfg(feature = "std")]
use alloc::sync::{Arc, Weak};

#[cfg(feature = "std")]
use std::thread::{self, JoinHandle};
#[cfg(feature = "std")]
use std::sync::mpsc::{Sender, Receiver};
#[cfg(feature = "std")]
use std::time::Instant as StdInstant;
#[cfg(feature = "std")]
use std::time::Duration as StdDuration;

use crate::{
    FastHashMap,
    callbacks::{
        TimerCallback, TimerCallbackInfo, RefAny,
        TimerCallbackReturn, TimerCallbackType, UpdateScreen,
        ThreadCallbackType, WriteBackCallback, WriteBackCallbackType,
        CallbackInfo, FocusTarget, ScrollPosition, DomNodeId
    },
    app_resources::AppResources,
    window::{FullWindowState, LogicalPosition, RawWindowHandle, WindowState, WindowCreateOptions},
    styled_dom::{DomId, AzNodeId},
    id_tree::NodeId,
    ui_solver::LayoutResult,
};
#[cfg(feature = "opengl")]
use crate::gl::GlContextPtr;
use azul_css::{OptionLayoutPoint, CssProperty};

/// Should a timer terminate or not - used to remove active timers
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TerminateTimer {
    /// Remove the timer from the list of active timers
    Terminate,
    /// Do nothing and let the timers continue to run
    Continue,
}

static MAX_TIMER_ID: AtomicUsize = AtomicUsize::new(0);

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerId { id: usize }

impl TimerId {
    /// Generates a new, unique `TimerId`.
    pub fn unique() -> Self {
        TimerId { id: MAX_TIMER_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

static MAX_THREAD_ID: AtomicUsize = AtomicUsize::new(0);

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ThreadId { id: usize }

impl ThreadId {
    /// Generates a new, unique `ThreadId`.
    pub fn unique() -> Self {
        ThreadId { id: MAX_THREAD_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Instant {
    System(AzInstantPtr),
    Tick(SystemTick),
}

impl Instant {
    /// Adds a duration to the instant, does nothing in undefined cases
    /// (i.e. trying to add a Duration::Tick to an Instant::System)
    pub fn add_optional_duration(&self, duration: Option<&Duration>) -> Self {
        match duration {
            Some(d) => match (self, d) {
                (Instant::System(i), Duration::System(d)) => {
                    #[cfg(feature = "std")] {
                        let s: StdInstant = i.into();
                        let d: StdDuration = d.into();
                        let new: AzInstantPtr = (s + d).into();
                        Instant::System(new)
                    }
                    #[cfg(not(feature = "std"))] {
                        unreachable!()
                    }
                },
                (Instant::Tick(s), Duration::Tick(d)) => {
                    Instant::Tick(SystemTick { tick_counter: s.tick_counter + d.tick_diff })
                },
                _ => { panic!("invalid: trying to add a duration {:?} to an instant {:?}", d, self); },
            },
            None => self.clone()
        }
    }

    /// Calculates the duration since an earlier point in time
    ///
    /// - Panics if the earlier Instant was created after the current Instant
    /// - Panics if the two enums do not have the same variant (tick / std)
    pub fn duration_since(&self, earlier: &Instant) -> Duration {
        match (earlier, self) {
            (Instant::System(prev), Instant::System(now)) => {
                #[cfg(feature = "std")] {
                    let prev_instant: StdInstant = prev.into();
                    let now_instant: StdInstant = now.into();
                    Duration::System((now_instant.duration_since(prev_instant)).into())
                }
                #[cfg(not(feature = "std"))] {
                    unreachable!() // cannot construct a SystemTime on no_std
                }
            },
            (Instant::Tick(SystemTick { tick_counter: prev }), Instant::Tick(SystemTick { tick_counter: now })) => {
                if prev > now {
                    panic!("illegal: subtraction 'Instant - Instant' would result in a negative duration")
                } else {
                    Duration::Tick(SystemTickDiff { tick_diff: now - prev })
                }
            },
            _ => panic!("illegal: trying to calculate a Duration from a SystemTime and a Tick instant"),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTick {
    pub tick_counter: u64,
}

impl SystemTick {
    pub const fn new(tick_counter: u64) -> Self { Self { tick_counter } }
}

#[repr(C)]
pub struct AzInstantPtr {
    pub ptr: *mut c_void, // ptr: *mut StdInstant
    pub clone_fn: extern "C" fn (*const c_void) -> AzInstantPtr,
    pub destructor: extern "C" fn(*mut c_void),
}

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

impl Eq for AzInstantPtr { }

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
    fn new(instant: StdInstant) -> Self { instant.into() }
    fn get(&self) -> StdInstant { let p = unsafe { &*(self.ptr as *const StdInstant) }; *p }
}

impl Clone for AzInstantPtr {
    fn clone(&self) -> Self {
        (self.clone_fn)(self.ptr)
    }
}

#[cfg(feature = "std")]
extern "C" fn std_instant_clone(ptr: *const c_void) -> AzInstantPtr {
    unsafe { &*(ptr as *mut StdInstant) }.clone().into()
}

#[cfg(feature = "std")]
extern "C" fn std_instant_drop(ptr: *mut c_void) {
    let _ = unsafe { Box::<StdInstant>::from_raw(ptr as *mut StdInstant) };
}

#[cfg(feature = "std")]
impl From<StdInstant> for AzInstantPtr {
    fn from(s: StdInstant) -> AzInstantPtr {
        Self {
            ptr: Box::into_raw(Box::new(s)) as *const c_void,
            destructor: std_instant_clone,
            destructor: std_instant_drop,
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
        (self.destructor)(self.ptr)
    }
}

// ----  LIBSTD implementation for AzInstantPtr END


#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Duration {
    System(SystemTimeDiff),
    Tick(SystemTickDiff),
}

impl Duration {

    #[allow(unused_variables)]
    pub fn greater_than(&self, other: &Self) -> bool {
        match (self, other) {
            // self > other
            (Duration::System(s), Duration::System(o)) => {
                #[cfg(feature = "std")] {
                    let s: StdDuration = s.into();
                    let o: StdDuration = o.into();
                    s > o
                }
                #[cfg(not(feature = "std"))] {
                    unreachable!()
                }
             },
            (Duration::Tick(s), Duration::Tick(o)) => {
                s.tick_diff > o.tick_diff
            },
            _ => { panic!("illegal: trying to compare a SystemDuration with a TickDuration"); },
        }
    }

    #[allow(unused_variables)]
    pub fn smaller_than(&self, other: &Self) -> bool {
        // self < other
        match (self, other) {
            // self > other
            (Duration::System(s), Duration::System(o)) => {
                #[cfg(feature = "std")] {
                    let s: StdDuration = s.into();
                    let o: StdDuration = o.into();
                    s < o
                }
                #[cfg(not(feature = "std"))] {
                    unreachable!()
                }
             },
            (Duration::Tick(s), Duration::Tick(o)) => {
                s.tick_diff < o.tick_diff
            },
            _ => { panic!("illegal: trying to compare a SystemDuration with a TickDuration"); },
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SystemTimeDiff {
    pub secs: u64,
    pub nanos: u32,
}

#[cfg(feature = "std")]
impl From<StdDuration> for SystemTimeDiff {
    fn from(d: StdDuration) -> Duration {
        Duration { secs: d.as_secs(), nanos: d.subsec_nanos() }
    }
}

#[cfg(feature = "std")]
impl From<SystemTimeDiff> for StdDuration {
    fn from(d: Duration) -> StdDuration {
        StdDuration::new(d.secs, d.nanos)
    }
}

const MILLIS_PER_SEC: u64 = 1_000;
const NANOS_PER_MILLI: u32 = 1_000_000;
const NANOS_PER_SEC: u32 = 1_000_000_000;

impl SystemTimeDiff {

    pub const fn from_secs(secs: u64) -> Self { SystemTimeDiff { secs, nanos: 0 } }
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
    #[cfg(feature = "std")]
    pub fn get(&self) -> StdDuration { (*self).into() }
}

impl_option!(Instant, OptionInstant, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(Duration, OptionDuration, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

/// A `Timer` is a function that is run on every frame.
///
/// There are often a lot of visual Threads such as animations or fetching the
/// next frame for a GIF or video, etc. - that need to run every frame or every X milliseconds,
/// but they aren't heavy enough to warrant creating a thread - otherwise the framework
/// would create too many threads, which leads to a lot of context switching and bad performance.
///
/// The callback of a `Timer` should be fast enough to run under 16ms,
/// otherwise running timers will block the main UI thread.
#[derive(Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    /// Data that is internal to the timer
    pub data: RefAny,
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
    pub fn new(mut data: RefAny, callback: TimerCallbackType, get_system_time_fn: GetSystemTimeFn) -> Self {
        Timer {
            data: data.clone_into_library_memory(),
            created: (get_system_time_fn)(),
            run_count: 0,
            last_run: OptionInstant::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: TimerCallback { cb: callback },
        }
    }

    /// Returns when the timer needs to run again
    pub fn instant_of_next_run(&self) -> Instant {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s,
            None => &self.created,
        };

        last_run.clone()
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

    /// Crate-internal: Invokes the timer if the timer should run. Otherwise returns `UpdateScreen::DoNothing`
    pub fn invoke(&mut self, data: &mut RefAny, callback_info: CallbackInfo, frame_start: Instant, get_system_time_fn: GetSystemTimeFn) -> TimerCallbackReturn {

        let instant_now = (get_system_time_fn)();

        if let OptionDuration::Some(interval) = self.interval {

            let last_run = match self.last_run.as_ref() {
                Some(s) => s.clone(),
                None => self.created.add_optional_duration(self.delay.as_ref()),
            };

            if instant_now.duration_since(&last_run).smaller_than(&interval) {
                return TimerCallbackReturn {
                    should_update: UpdateScreen::DoNothing,
                    should_terminate: TerminateTimer::Continue
                };
            }
        }

        let run_count = self.run_count;
        let timer_callback_info = TimerCallbackInfo {
            callback_info,
            frame_start,
            call_count: run_count,
        };
        let mut res = (self.callback.cb)(data, &mut self.data, timer_callback_info);

        // Check if the timers timeout is reached
        if let OptionDuration::Some(timeout) = self.timeout {
            if instant_now.duration_since(&self.created).greater_than(&timeout) {
                res = TimerCallbackReturn { should_update: UpdateScreen::DoNothing, should_terminate: TerminateTimer::Terminate };
            }
        }

        self.last_run = OptionInstant::Some(instant_now);
        self.run_count += 1;

        res
    }
}

/// Message that can be sent from the main thread to the Thread using the ThreadId.
///
/// The thread can ignore the event.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ThreadSendMsg {
    /// The thread should terminate at the nearest
    TerminateThread,
    /// Next frame tick
    Tick,
}

impl_option!(ThreadSendMsg, OptionThreadSendMsg, [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]);

// Message that is received from the running thread
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(UpdateScreen),
}

impl_option!(ThreadReceiveMsg, OptionThreadReceiveMsg, copy = false, clone = false, [Debug, PartialEq, PartialOrd, Eq, Ord, Hash]);

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadWriteBackMsg {
    // The data to write back into. Will be passed as the second argument to the thread
    pub data: RefAny,
    // The callback to call on this data.
    pub callback: WriteBackCallback,
}

impl ThreadWriteBackMsg {
    pub fn new(callback: WriteBackCallbackType, mut data: RefAny) -> Self {
        Self { data: data.clone_into_library_memory(), callback: WriteBackCallback { cb: callback } }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
#[repr(C)]
pub struct ThreadSender {
    pub ptr: *mut c_void, // *const Box<Sender<ThreadReceiveMsg>>
    pub send_fn: ThreadSendFn,
    pub destructor: ThreadSenderDestructorFn,
}

unsafe impl Send for ThreadSender { }

impl ThreadSender {
    // send data from the user thread to the main thread
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        (self.send_fn)(self.ptr, msg)
    }
}

impl Drop for ThreadSender {
    fn drop(&mut self) {
        (self.destructor)(self)
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
#[repr(C)]
pub struct ThreadReceiver {
    pub ptr: *mut c_void, // *mut Box<Receiver<ThreadSendMsg>>
    pub recv_fn: ThreadRecvFn,
    pub destructor: ThreadReceiverDestructorFn,
}

unsafe impl Send for ThreadReceiver { }

impl ThreadReceiver {
    // receive data from the main thread
    pub fn recv(&mut self) -> OptionThreadSendMsg {
        (self.recv_fn)(self.ptr)
    }
}

impl Drop for ThreadReceiver {
    fn drop(&mut self) {
        (self.destructor)(self)
    }
}

/// Config that is necessary so that threading + animations can compile on no_std
///
/// See the `default` implementations in this module for an example on how to
/// create a thread
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct ExternalSystemCallbacks {
    pub create_thread_fn: CreateThreadFn,
    pub get_system_time_fn: GetSystemTimeFn,
}

/// Function that creates a new `Thread` object
pub type CreateThreadFn = extern "C" fn(RefAny, RefAny, ThreadCallbackType) -> Thread;
/// Get the current system type, equivalent to `std::time::Instant::now()`, except it
/// also works on systems that don't have a clock (such as embedded timers)
pub type GetSystemTimeFn = extern "C" fn() -> Instant;

// function called to check if the thread has finished
pub type CheckThreadFinishedFn = extern "C" fn(/* dropcheck */ *const c_void) -> bool;
// function to send a message to the thread
pub type LibrarySendThreadMsgFn = extern "C" fn(/* Sender<ThreadSendMsg> */ *mut c_void, ThreadSendMsg) -> bool; // return true / false on success / failure
// function to receive a message from the thread
pub type LibraryReceiveThreadMsg = extern "C" fn(/* Receiver<ThreadReceiveMsg> */ *mut c_void) -> OptionThreadReceiveMsg;
// function that the RUNNING THREAD can call to receive messages from the main thread
pub type ThreadRecvFn = extern "C" fn(/* receiver.ptr */ *mut c_void) -> OptionThreadSendMsg;
// function that the RUNNING THREAD can call to send messages to the main thread
pub type ThreadSendFn = extern "C" fn(/* sender.ptr */*mut c_void, ThreadReceiveMsg) -> bool; // return false on error

// function called on Thread::drop()
pub type ThreadDestructorFn = extern "C" fn(/* thread handle */ *mut c_void, /* sender */ *mut c_void, /* receiver */ *mut c_void, /* dropcheck */ *const c_void);
// destructor of the ThreadReceiver
pub type ThreadReceiverDestructorFn = extern "C" fn(*mut ThreadReceiver);
// destructor of the ThreadSender
pub type ThreadSenderDestructorFn = extern "C" fn(*mut ThreadSender);

/// A `Thread` is a seperate thread that is owned by the framework.
///
/// In difference to a `Thread`, you don't have to `await()` the result of a `Thread`,
/// you can just hand the Thread to the framework (via `AppResources::add_Thread`) and
/// the framework will automatically update the UI when the Thread is finished.
/// This is useful to offload actions such as loading long files, etc. to a background thread.
///
/// Azul will join the thread automatically after it is finished (joining won't block the UI).
#[derive(Debug)]
#[repr(C)]
pub struct Thread {
    // Thread handle of the currently in-progress Thread
    pub thread_handle: *mut c_void, // *mut Option<JoinHandle<()>>,
    pub sender: *mut c_void, // *mut Sender<ThreadSendMsg>,
    pub receiver: *mut c_void, // *mut Receiver<ThreadReceiveMsg>,
    pub writeback_data: RefAny,
    pub dropcheck: *const c_void, // *const Weak<()>,
    pub check_thread_finished_fn: CheckThreadFinishedFn, //
    pub send_thread_msg_fn: LibrarySendThreadMsgFn,
    pub receive_thread_msg_fn: LibraryReceiveThreadMsg,
    pub thread_destructor_fn: ThreadDestructorFn,
}

#[cfg(feature = "std")]
pub extern "C" fn create_thread_libstd(mut thread_initialize_data: RefAny, mut writeback_data: RefAny, callback: ThreadCallbackType) -> Thread {

    let (sender_receiver, receiver_receiver) = core::sync::mpsc::channel::<ThreadReceiveMsg>();
    let sender_receiver = ThreadSender {
        ptr: Box::into_raw(Box::new(sender_receiver)) as *const c_void,
        send_fn: default_send_thread_msg_fn,
        destructor: thread_sender_drop,
    };

    let (sender_sender, receiver_sender) = core::sync::mpsc::channel::<ThreadSendMsg>();
    let receiver_sender = ThreadReceiver {
        ptr: Box::into_raw(Box::new(receiver_sender)) as *const c_void,
        recv_fn: default_receive_thread_msg_fn,
        destructor: thread_receiver_drop,
    };

    let thread_check = Arc::new(());
    let thread_weak = Arc::downgrade(&thread_check);

    let thread_handle = Some(thread::spawn(move || {
        let _ = thread_check;
        callback(thread_initialize_data.clone_into_library_memory(), sender_receiver, receiver_sender);
        // thread_check gets dropped here, signals that the thread has finished
    }));

    let thread: Box<Option<JoinHandle<()>>> = Box::new(thread_handle);
    let sender: Box<Sender<ThreadSendMsg>> = Box::new(sender_sender);
    let receiver: Box<Receiver<ThreadReceiveMsg>> = Box::new(receiver_receiver);
    let dropcheck: Box<Weak<()>> = Box::new(dropcheck);

    Self {
        thread_handle: Box::into_raw(thread) as *mut c_void,
        sender: Box::into_raw(receiver) as *mut c_void,
        receiver: Box::into_raw(sender) as *mut c_void,
        writeback_data,
        dropcheck: Box::into_raw(dropcheck) as *mut c_void,
        thread_destructor_fn: default_thread_destructor_fn,
        check_thread_finished_fn: default_check_thread_finished,
        send_thread_msg_fn: library_send_thread_msg_fn,
        receive_thread_msg_fn: library_receive_thread_msg_fn,
    }
}

impl Thread {
    /// Returns true if the Thread has been finished, false otherwise
    pub(crate) fn is_finished(&self) -> bool {
        (self.check_thread_finished_fn)(self.dropcheck)
    }

    pub(crate) fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        (self.send_thread_msg_fn)(self.sender, msg)
    }

    pub(crate) fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        (self.receive_thread_msg_fn)(self.receiver)
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        (self.thread_destructor_fn)(self.thread_handle, self.sender, self.receiver, self.dropcheck);
    }
}

#[cfg(feature = "std")]
extern "C" fn library_send_thread_msg_fn(ptr: *mut c_void, msg: ThreadSendMsg) -> bool {
    unsafe { &mut *(ptr as *mut Sender<ThreadSendMsg>) }.send(msg).is_ok()
}

#[cfg(feature = "std")]
extern "C" fn library_receive_thread_msg_fn(ptr: *mut c_void) -> OptionThreadReceiveMsg {
    unsafe { &mut *(ptr as *mut Receiver<ThreadReceiveMsg>) }.try_recv().ok()
}

#[cfg(feature = "std")]
extern "C" fn default_send_thread_msg_fn(sender: *mut ThreadSender, msg: ThreadReceiveMsg) -> bool {
    unsafe { &mut *(sender.ptr as *mut Sender<ThreadReceiveMsg>) }.send(msg).is_ok()
}

#[cfg(feature = "std")]
extern "C" fn default_receive_thread_msg_fn(receiver: *mut ThreadReceiver) -> OptionThreadSendMsg {
    unsafe { &mut *(receiver.ptr as *mut Receiver<ThreadSendMsg>) }.try_recv().ok()
}

#[cfg(feature = "std")]
extern "C" fn default_check_thread_finished(dropcheck: *mut c_void) -> bool {
    unsafe { &mut *dropcheck as *mut Weak<()> }.upgrade().is_none()
}

#[cfg(feature = "std")]
extern "C" fn default_thread_destructor_fn(thread: *mut c_void, sender: *mut c_void, receiver: *mut c_void, dropcheck: *mut c_void) {

    let thread = unsafe { Box::from_raw(thread as *mut Option<JoinHandle<()>>) };
    let sender = unsafe { Box::from_raw(sender as *mut Sender<ThreadSendMsg>) };
    let receiver = unsafe { Box::from_raw(receiver as *mut Receiver<ThreadReceiveMsg>) };
    let dropcheck = unsafe { Box::from_raw(dropcheck as *mut Weak<()>) };

    if let Some(thread_handle) = thread.take() {
        let _ = sender.send(ThreadSendMsg::TerminateThread);
        let _ = thread_handle.join(); // ignore the result, don't panic
    }
}

#[cfg(feature = "std")]
extern "C" fn thread_sender_drop(val: *mut ThreadSender) {
    let _ = unsafe { Box::from_raw(self.ptr as *mut Sender<ThreadReceiveMsg>) };
}

#[cfg(feature = "std")]
extern "C" fn thread_receiver_drop(val: *mut ThreadReceiver) {
    let _ = unsafe { Box::from_raw(self.ptr as *mut Receiver<ThreadSendMsg>) };
}

/// Run all currently registered timers
#[must_use = "the UpdateScreen result of running timers should not be ignored"]
#[cfg(feature = "opengl")]
pub fn run_all_timers<'a, 'b>(
    data: &mut RefAny,
    current_timers: &mut FastHashMap<TimerId, Timer>,
    frame_start: Instant,
    get_system_time_fn: GetSystemTimeFn,

    current_window_state: &FullWindowState,
    modifiable_window_state: &mut WindowState,
    gl_context: &GlContextPtr,
    resources : &mut AppResources,
    system_callbacks: ExternalSystemCallbacks,
    timers: &mut FastHashMap<TimerId, Timer>,
    threads: &mut FastHashMap<ThreadId, Thread>,
    new_windows: &mut Vec<WindowCreateOptions>,
    current_window_handle: &RawWindowHandle,
    layout_results: &'a mut Vec<LayoutResult>,
    stop_propagation: &mut bool,
    focus_target: &mut Option<FocusTarget>,
    current_scroll_states: &BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
    css_properties_changed_in_callbacks: &mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
    nodes_scrolled_in_callback: &mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
) -> UpdateScreen {

    let mut should_update_screen = UpdateScreen::DoNothing;
    let mut timers_to_terminate = Vec::new();

    for (key, timer) in current_timers.iter_mut() {

        let hit_dom_node = DomNodeId { dom: DomId::ROOT_ID, node: AzNodeId::from_crate_internal(None) };
        let cursor_relative_to_item = OptionLayoutPoint::None;
        let cursor_in_viewport = OptionLayoutPoint::None;

        let layout_result = &mut layout_results[hit_dom_node.dom.inner];
        let mut datasets = layout_result.styled_dom.node_data.split_into_callbacks_and_dataset();

        let callback_info = CallbackInfo::new(
            current_window_state,
            modifiable_window_state,
            gl_context,
            resources ,
            timers,
            threads,
            new_windows,
            current_window_handle,
            &layout_result.styled_dom.node_hierarchy,
            system_callbacks,
            &mut datasets.1,
            stop_propagation,
            focus_target,
            current_scroll_states,
            css_properties_changed_in_callbacks,
            nodes_scrolled_in_callback,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        let TimerCallbackReturn { should_update, should_terminate } = timer.invoke(data, callback_info, frame_start.clone(), get_system_time_fn);

        match should_update {
            UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                if should_update_screen == UpdateScreen::DoNothing {
                    should_update_screen = should_update;
                }
            },
            UpdateScreen::RegenerateStyledDomForAllWindows => {
                if should_update_screen == UpdateScreen::DoNothing || should_update_screen == UpdateScreen::RegenerateStyledDomForCurrentWindow  {
                    should_update_screen = should_update;
                }
            },
            UpdateScreen::DoNothing => { }
        }

        if should_terminate == TerminateTimer::Terminate {
            timers_to_terminate.push(key.clone());
        }
    }

    for key in timers_to_terminate {
        timers.remove(&key);
    }

    should_update_screen
}

/// Remove all Threads that have finished executing
#[must_use = "the UpdateScreen result of running Threads should not be ignored"]
#[cfg(feature = "opengl")]
pub fn clean_up_finished_threads<'a, 'b>(
    cleanup_threads: &mut FastHashMap<ThreadId, Thread>,

    current_window_state: &FullWindowState,
    modifiable_window_state: &mut WindowState,
    gl_context: &GlContextPtr,
    resources : &mut AppResources,
    system_callbacks: ExternalSystemCallbacks,
    timers: &mut FastHashMap<TimerId, Timer>,
    threads: &mut FastHashMap<ThreadId, Thread>,
    new_windows: &mut Vec<WindowCreateOptions>,
    current_window_handle: &RawWindowHandle,
    layout_results: &'a mut Vec<LayoutResult>,
    stop_propagation: &mut bool,
    focus_target: &mut Option<FocusTarget>,
    current_scroll_states: &BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
    css_properties_changed_in_callbacks: &mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
    nodes_scrolled_in_callback: &mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
) -> UpdateScreen {

    let mut update_screen = UpdateScreen::DoNothing;

    let hit_dom_node = DomNodeId { dom: DomId::ROOT_ID, node: AzNodeId::from_crate_internal(None) };
    let cursor_relative_to_item = OptionLayoutPoint::None;
    let cursor_in_viewport = OptionLayoutPoint::None;

    let layout_result = &mut layout_results[hit_dom_node.dom.inner];
    let mut datasets = layout_result.styled_dom.node_data.split_into_callbacks_and_dataset();
    let node_hierarchy = &layout_result.styled_dom.node_hierarchy;

    // originally this code used retain(), but retain() is not available on no_std
    let mut thread_ids_to_remove = Vec::new();

    for (thread_id, thread) in cleanup_threads.iter_mut() {

        let _ = thread.sender_send(ThreadSendMsg::Tick);

        let update = match thread.receiver_try_recv() {
            OptionThreadReceiveMsg::None => UpdateScreen::DoNothing,
            OptionThreadReceiveMsg::Some(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg { data, callback })) => {
                let callback_info = CallbackInfo::new(
                    current_window_state,
                    modifiable_window_state,
                    gl_context,
                    resources ,
                    timers,
                    threads,
                    new_windows,
                    current_window_handle,
                    node_hierarchy,
                    system_callbacks,
                    &mut datasets.1,
                    stop_propagation,
                    focus_target,
                    current_scroll_states,
                    css_properties_changed_in_callbacks,
                    nodes_scrolled_in_callback,
                    hit_dom_node,
                    cursor_relative_to_item,
                    cursor_in_viewport,
                );

                (callback.cb)(&mut thread.writeback_data, data, callback_info)
            },
            OptionThreadReceiveMsg::Some(ThreadReceiveMsg::Update(update_screen)) => update_screen,
        };

        match update {
            UpdateScreen::DoNothing => { },
            UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                if update_screen == UpdateScreen::DoNothing {
                    update_screen = UpdateScreen::RegenerateStyledDomForCurrentWindow;
                }
            },
            UpdateScreen::RegenerateStyledDomForAllWindows => {
                if update_screen == UpdateScreen::DoNothing || update_screen == UpdateScreen::RegenerateStyledDomForCurrentWindow {
                    update_screen = UpdateScreen::RegenerateStyledDomForAllWindows;
                }
            }
        }

        if thread.is_finished() {
            thread_ids_to_remove.push(*thread_id);
        }
    }

    for thread_id in thread_ids_to_remove {
        cleanup_threads.remove(&thread_id);
    }

    update_screen
}