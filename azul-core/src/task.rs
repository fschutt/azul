use std::{
    sync::{Arc, Weak, atomic::{AtomicUsize, Ordering}, mpsc::{Sender, Receiver}},
    thread::{self, JoinHandle},
    ffi::c_void,
    collections::BTreeMap,
};
use std::time::Instant as StdInstant;
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
    gl::GlContextPtr,
    styled_dom::{DomId, AzNodeId, AzNodeVec},
    id_tree::NodeId,
};
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

#[repr(C)]
pub struct AzInstantPtr { /* ptr: *const StdInstant */ pub ptr: *const c_void }

impl std::fmt::Debug for AzInstantPtr {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.get())
    }
}

impl std::hash::Hash for AzInstantPtr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

impl PartialEq for AzInstantPtr {
    fn eq(&self, other: &AzInstantPtr) -> bool {
        self.get() == other.get()
    }
}

impl Eq for AzInstantPtr { }

impl PartialOrd for AzInstantPtr {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.get()).cmp(&(other.get())))
    }
}

impl Ord for AzInstantPtr {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        (self.get()).cmp(&(other.get()))
    }
}

impl AzInstantPtr {
    pub fn now() -> Self { StdInstant::now().into() }
    fn new(instant: StdInstant) -> Self { instant.into() }
    fn get(&self) -> StdInstant { let p = unsafe { &*(self.ptr as *const StdInstant) }; *p }
}

impl Clone for AzInstantPtr {
    fn clone(&self) -> Self {
        self.get().into()
    }
}

impl From<StdInstant> for AzInstantPtr {
    fn from(s: StdInstant) -> AzInstantPtr {
        Self { ptr: Box::into_raw(Box::new(s)) as *const c_void }
    }
}

impl From<AzInstantPtr> for StdInstant {
    fn from(s: AzInstantPtr) -> StdInstant {
        s.get()
    }
}

impl Drop for AzInstantPtr {
    fn drop(&mut self) {
        let _ = unsafe { Box::<StdInstant>::from_raw(self.ptr as *mut StdInstant) };
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AzDuration {
    pub secs: u64,
    pub nanos: u32,
}

impl From<StdDuration> for AzDuration {
    fn from(d: StdDuration) -> AzDuration {
        AzDuration { secs: d.as_secs(), nanos: d.subsec_nanos() }
    }
}

impl From<AzDuration> for StdDuration {
    fn from(d: AzDuration) -> StdDuration {
        StdDuration::new(d.secs, d.nanos)
    }
}

impl AzDuration {
    pub fn from_secs(secs: u64) -> Self { StdDuration::from_secs(secs).into() }
    pub fn from_millis(millis: u64) -> Self { StdDuration::from_millis(millis).into() }
    pub fn from_nanos(nanos: u64) -> Self { StdDuration::from_nanos(nanos).into() }
    pub fn get(&self) -> StdDuration { (*self).into() }
}

impl_option!(AzInstantPtr, OptionInstantPtr, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(AzDuration, OptionDuration, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

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
    pub created: AzInstantPtr,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: OptionInstantPtr,
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
    pub fn new(mut data: RefAny, callback: TimerCallbackType) -> Self {
        Timer {
            data: data.clone_into_library_memory(),
            created: AzInstantPtr::new(StdInstant::now()),
            run_count: 0,
            last_run: OptionInstantPtr::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: TimerCallback { cb: callback },
        }
    }

    /// Returns when the timer needs to run again
    pub fn instant_of_next_run(&self) -> StdInstant {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s,
            None => &self.created,
        };
        let last_run: StdInstant = last_run.clone().into();
        let delay = self.delay.as_ref().map(|i| i.clone().into()).unwrap_or(StdDuration::from_millis(0));
        let interval = self.interval.as_ref().map(|i| i.clone().into()).unwrap_or(StdDuration::from_millis(16));
        last_run + delay + interval
    }

    /// Delays the timer to not start immediately but rather
    /// start after a certain time frame has elapsed.
    #[inline]
    pub fn with_delay(mut self, delay: AzDuration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    /// Converts the timer into a timer, running the function only
    /// if the given `Duration` has elapsed since the last run
    #[inline]
    pub fn with_interval(mut self, interval: AzDuration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    /// Converts the timer into a countdown, by giving it a maximum duration
    /// (counted from the creation of the Timer, not the first use).
    #[inline]
    pub fn with_timeout(mut self, timeout: AzDuration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }

    /// Crate-internal: Invokes the timer if the timer should run. Otherwise returns `UpdateScreen::DoNothing`
    pub fn invoke(&mut self, data: &mut RefAny, callback_info: CallbackInfo, frame_start: StdInstant) -> TimerCallbackReturn {

        let instant_now = StdInstant::now();
        let delay = self.delay.clone().into_option().unwrap_or_else(|| AzDuration::from_millis(0));

        // Check if the timers timeout is reached
        if let OptionDuration::Some(timeout) = self.timeout {
            if instant_now - self.created.get() > timeout.get() {
                return TimerCallbackReturn { should_update: UpdateScreen::DoNothing, should_terminate: TerminateTimer::Terminate };
            }
        }

        if let OptionDuration::Some(interval) = self.interval {
            let last_run = match self.last_run.as_option() {
                Some(s) => s.get(),
                None => self.created.get() + delay.get(),
            };
            if instant_now - last_run < interval.get() {
                return TimerCallbackReturn { should_update: UpdateScreen::DoNothing, should_terminate: TerminateTimer::Continue };
            }
        }

        let run_count = self.run_count;
        let timer_callback_info = TimerCallbackInfo {
            callback_info,
            frame_start: AzInstantPtr::new(frame_start),
            call_count: run_count,
        };
        let res = (self.callback.cb)(data, &mut self.data, timer_callback_info);

        self.last_run = OptionInstantPtr::Some(instant_now.into());
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

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadSender {
    pub ptr: *const c_void, // *const Box<Sender<ThreadReceiveMsg>>
}

unsafe impl Send for ThreadSender { }

impl ThreadSender {

    pub fn new() -> (Self, Receiver<ThreadReceiveMsg>){
        let (sender, receiver) = std::sync::mpsc::channel::<ThreadReceiveMsg>();
        let sender: Sender<ThreadReceiveMsg> = sender;
        (Self { ptr: Box::into_raw(Box::new(sender)) as *const c_void }, receiver)
    }

    fn downcast<'a>(&'a mut self) -> &mut Sender<ThreadReceiveMsg> {
        unsafe { &mut *(self.ptr as *mut Sender<ThreadReceiveMsg>) }
    }

    // send data from the user thread to the main thread
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        self.downcast().send(msg).is_ok()
    }
}

impl Drop for ThreadSender {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr as *mut Sender<ThreadReceiveMsg>) };
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadReceiver {
    pub ptr: *const c_void, // *const Box<Receiver<ThreadSendMsg>>
}

unsafe impl Send for ThreadReceiver { }

impl ThreadReceiver {
    pub fn new() -> (Self, Sender<ThreadSendMsg>) {
        let (sender, receiver) = std::sync::mpsc::channel::<ThreadSendMsg>();
        let receiver: Receiver<ThreadSendMsg> = receiver;
        (Self { ptr: Box::into_raw(Box::new(receiver)) as *const c_void }, sender)
    }

    fn downcast<'a>(&'a mut self) -> &mut Receiver<ThreadSendMsg> {
        unsafe { &mut *(self.ptr as *mut Receiver<ThreadSendMsg>) }
    }

    // receive data from the main thread
    pub fn recv(&mut self) -> Option<ThreadSendMsg> {
        self.downcast().try_recv().ok()
    }
}

impl Drop for ThreadReceiver {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr as *mut Receiver<ThreadSendMsg>) };
        // ThreadSendMsg::TerminateThread
    }
}

/// A `Thread` is a seperate thread that is owned by the framework.
///
/// In difference to a `Thread`, you don't have to `await()` the result of a `Thread`,
/// you can just hand the Thread to the framework (via `AppResources::add_Thread`) and
/// the framework will automatically update the UI when the Thread is finished.
/// This is useful to offload actions such as loading long files, etc. to a background thread.
///
/// Azul will join the thread automatically after it is finished (joining won't block the UI).
#[derive(Debug)]
pub struct Thread {
    // Thread handle of the currently in-progress Thread
    pub thread: Option<JoinHandle<()>>,
    pub sender: Sender<ThreadSendMsg>,
    pub receiver: Receiver<ThreadReceiveMsg>,
    pub writeback_data: RefAny,
    pub dropcheck: Weak<()>,
}

impl Thread {

    /// Creates a new Thread from a callback and a set of input data - which has to be wrapped in an `Arc<Mutex<T>>>`.
    pub(crate) fn new(mut thread_initialize_data: RefAny, mut writeback_data: RefAny, callback: ThreadCallbackType) -> Self {

        let (sender_receiver, receiver_receiver) = ThreadSender::new();
        let (receiver_sender, sender_sender) = ThreadReceiver::new();

        let thread_check = Arc::new(());
        let thread_weak = Arc::downgrade(&thread_check);

        let thread_handle = thread::spawn(move || {
            let _ = thread_check;
            callback(thread_initialize_data.clone_into_library_memory(), sender_receiver, receiver_sender);
            // thread_check gets dropped here, signals that the thread has finished
        });

        Self {
            thread: Some(thread_handle),
            sender: sender_sender,
            receiver: receiver_receiver,
            dropcheck: thread_weak,
            writeback_data: writeback_data.clone_into_library_memory(),
        }
    }

    /// Returns true if the Thread has been finished, false otherwise
    pub(crate) fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        if let Some(thread_handle) = self.thread.take() {
            let _ = self.sender.send(ThreadSendMsg::TerminateThread);
            let _ = thread_handle.join(); // ignore the result, don't panic
        }
    }
}

/// Run all currently registered timers
#[must_use = "the UpdateScreen result of running timers should not be ignored"]
pub fn run_all_timers(
    data: &mut RefAny,
    current_timers: &mut FastHashMap<TimerId, Timer>,
    frame_start: StdInstant,

    current_window_state: &FullWindowState,
    modifiable_window_state: &mut WindowState,
    gl_context: &GlContextPtr,
    resources : &mut AppResources,
    timers: &mut FastHashMap<TimerId, Timer>,
    threads: &mut FastHashMap<ThreadId, Thread>,
    new_windows: &mut Vec<WindowCreateOptions>,
    current_window_handle: &RawWindowHandle,
    node_hierarchy: &BTreeMap<DomId, AzNodeVec>,
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
            stop_propagation,
            focus_target,
            current_scroll_states,
            css_properties_changed_in_callbacks,
            nodes_scrolled_in_callback,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        let TimerCallbackReturn { should_update, should_terminate } = timer.invoke(data, callback_info, frame_start);

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
pub fn clean_up_finished_threads(
    cleanup_threads: &mut FastHashMap<ThreadId, Thread>,

    current_window_state: &FullWindowState,
    modifiable_window_state: &mut WindowState,
    gl_context: &GlContextPtr,
    resources : &mut AppResources,
    timers: &mut FastHashMap<TimerId, Timer>,
    threads: &mut FastHashMap<ThreadId, Thread>,
    new_windows: &mut Vec<WindowCreateOptions>,
    current_window_handle: &RawWindowHandle,
    node_hierarchy: &BTreeMap<DomId, AzNodeVec>,
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

    cleanup_threads.retain(|_thread_id, thread| {

        let _ = thread.sender.send(ThreadSendMsg::Tick);

        let update = match thread.receiver.try_recv().ok() {
            None => UpdateScreen::DoNothing,
            Some(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg { data, callback })) => {
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
            Some(ThreadReceiveMsg::Update(update_screen)) => update_screen,
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

        !thread.is_finished()
    });

    update_screen
}