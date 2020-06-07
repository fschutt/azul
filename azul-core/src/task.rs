use std::{
    sync::{Arc, Mutex, Weak, atomic::{AtomicUsize, Ordering}},
    thread::{self, JoinHandle},
    ffi::c_void,
};
use std::time::Instant as StdInstant;
use std::time::Duration as StdDuration;
use crate::{
    FastHashMap,
    callbacks::{
        TimerCallback, TimerCallbackInfo, RefAny,
        TimerCallbackReturn, TimerCallbackType, UpdateScreen,
    },
    app_resources::AppResources,
};

/// Should a timer terminate or not - used to remove active timers
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum TerminateTimer {
    /// Remove the timer from the list of active timers
    Terminate,
    /// Do nothing and let the timers continue to run
    Continue,
}

static MAX_DAEMON_ID: AtomicUsize = AtomicUsize::new(0);

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerId { id: usize }

impl TimerId {
    /// Generates a new, unique `TimerId`.
    pub fn new() -> Self {
        TimerId { id: MAX_DAEMON_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

#[repr(C)]
pub struct AzInstantPtr { /* ptr: *const StdInstant */ ptr: *const c_void }

impl AzInstantPtr {
    fn now() -> Self { StdInstant::now().into() }
    fn get(&self) -> Instant { let p = unsafe { Box::<StdInstant>::from_raw(self.ptr as *mut AzInstantPtr) }; let val = *p; std::mem::forget(p); val }
}

impl Clone for AzInstantPtr {
    fn clone(&self) -> Self {
        self.get().into()
    }
}

impl From<StdInstant> for AzInstantPtr {
    fn from(s: StdInstant) -> AzInstantPtr {
        Self { ptr: Box::into_raw(Box::new(s)) }
    }
}

impl From<StdInstant> for AzInstantPtr {
    fn from(s: StdInstant) -> AzInstantPtr {
        Self { ptr: Box::into_raw(Box::new(s)) }
    }
}

impl Drop for AzInstantPtr {
    fn drop(&mut self) {
        let _ = unsafe { Box::<StdInstant>::from_raw(self.ptr as *mut AzInstantPtr) };
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
        AzDuration { secs: d.as_secs(), nanos: d.as_nanos() }
    }
}

impl From<AzDuration> for StdDuration {
    fn from(d: AzDuration) -> StdDuration {
        StdDuration::new(d.secs, d.nanos)
    }
}

impl_option!(AzInstantPtr, OptionInstantPtr);
impl_option!(AzDuration, OptionDuration);

/// A `Timer` is a function that is run on every frame.
///
/// There are often a lot of visual tasks such as animations or fetching the
/// next frame for a GIF or video, etc. - that need to run every frame or every X milliseconds,
/// but they aren't heavy enough to warrant creating a thread - otherwise the framework
/// would create too many threads, which leads to a lot of context switching and bad performance.
///
/// The callback of a `Timer` should be fast enough to run under 16ms,
/// otherwise running timers will block the main UI thread.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    /// Stores when the timer was created (usually acquired by `Instant::now()`)
    pub created: AzInstantPtr,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: OptionInstantPtr,
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
    pub fn new(callback: TimerCallbackType) -> Self {
        Timer {
            created: Instant::now(),
            last_run: None,
            delay: None,
            interval: None,
            timeout: None,
            callback: TimerCallback { cb: callback },
        }
    }

    /// Delays the timer to not start immediately but rather
    /// start after a certain time frame has elapsed.
    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Converts the timer into a timer, running the function only
    /// if the given `Duration` has elapsed since the last run
    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    /// Converts the timer into a countdown, by giving it a maximum duration
    /// (counted from the creation of the Timer, not the first use).
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Crate-internal: Invokes the timer if the timer and
    /// the `self.timeout` allow it to
    pub fn invoke<'a>(&mut self, info: TimerCallbackInfo<'a>) -> TimerCallbackReturn {

        let instant_now = Instant::now();
        let delay = self.delay.unwrap_or_else(|| Duration::from_millis(0));

        // Check if the timers timeout is reached
        if let Some(timeout) = self.timeout {
            if instant_now - self.created > timeout {
                return (UpdateScreen::DontRedraw, TerminateTimer::Terminate);
            }
        }

        if let Some(interval) = self.interval {
            let last_run = match self.last_run {
                Some(s) => s,
                None => self.created + delay,
            };
            if instant_now - last_run < interval {
                return (UpdateScreen::DontRedraw, TerminateTimer::Continue);
            }
        }

        let res = (self.callback.0)(info);

        self.last_run = Some(instant_now);

        res
    }
}

/// Simple struct that is used by Azul internally to determine when the thread has finished executing.
/// When this struct goes out of scope, Azul will call `.join()` on the thread (so in order to not
/// block the main thread, simply let it go out of scope naturally.
pub struct DropCheck(Arc<()>);

/// A `Task` is a seperate thread that is owned by the framework.
///
/// In difference to a `Thread`, you don't have to `await()` the result of a `Task`,
/// you can just hand the task to the framework (via `AppResources::add_task`) and
/// the framework will automatically update the UI when the task is finished.
/// This is useful to offload actions such as loading long files, etc. to a background thread.
///
/// Azul will join the thread automatically after it is finished (joining won't block the UI).
#[derive(Debug)]
pub struct Task {
    // Thread handle of the currently in-progress task
    join_handle: Option<JoinHandle<UpdateScreen>>,
    dropcheck: Weak<()>,
    /// Timer that will run directly after this task is completed.
    pub after_completion_timer: Option<Timer>,
}

pub type TaskCallback<U> = fn(Arc<Mutex<U>>, DropCheck) -> UpdateScreen;

impl Task {

    /// Creates a new task from a callback and a set of input data - which has to be wrapped in an `Arc<Mutex<T>>>`.
    pub fn new<U: Send + 'static>(data: Arc<Mutex<U>>, callback: TaskCallback<U>) -> Self {

        let thread_check = Arc::new(());
        let thread_weak = Arc::downgrade(&thread_check);
        let thread_handle = thread::spawn(move || callback(data, DropCheck(thread_check)));

        Self {
            join_handle: Some(thread_handle),
            dropcheck: thread_weak,
            after_completion_timer: None,
        }
    }

    /// Stores a `Timer` that will run after the task has finished.
    ///
    /// Often necessary to "clean up" or copy data from the background task into the UI.
    #[inline]
    pub fn then(mut self, timer: Timer) -> Self {
        self.after_completion_timer = Some(timer);
        self
    }

    /// Returns true if the task has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(thread_handle) = self.join_handle.take() {
            let _ = thread_handle.join().unwrap();
        }
    }
}

/// A `Thread` is a simple abstraction over `std::thread` that allows to offload a pure
/// function to a different thread (essentially emulating async / await for older compilers).
///
/// # Warning
///
/// `Thread` panics if it goes out of scope before `.block()` was called.
pub struct Thread<T> {
    join_handle: Option<JoinHandle<T>>,
}

/// Error that can happen while calling `.block()`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum BlockError {
    /// Arc::into_inner() failed
    ArcUnlockError,
    /// The background thread panicked
    ThreadJoinError,
    /// Mutex::into_inner() failed
    MutexIntoInnerError,
}

impl<T> Thread<T> {

    /// Creates a new thread that spawns a certain (pure) function on a separate thread.
    /// This is a workaround until `await` is implemented. Note that invoking this function
    /// will create an OS-level thread.
    ///
    /// **Warning**: You *must* call `.await()`, otherwise the `Thread` will panic when it is dropped!
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate azul_core;
    /// # use azul_core::task::Thread;
    /// #
    /// fn pure_function(input: usize) -> usize { input + 1 }
    ///
    /// let thread_1 = Thread::new(5, pure_function);
    /// let thread_2 = Thread::new(10, pure_function);
    /// let thread_3 = Thread::new(20, pure_function);
    ///
    /// // thread_1, thread_2 and thread_3 run in parallel here...
    ///
    /// let result_1 = thread_1.block();
    /// let result_2 = thread_2.block();
    /// let result_3 = thread_3.block();
    ///
    /// assert_eq!(result_1, Ok(6));
    /// assert_eq!(result_2, Ok(11));
    /// assert_eq!(result_3, Ok(21));
    /// ```
    pub fn new<U>(initial_data: U, callback: fn(U) -> T) -> Self where T: Send + 'static, U: Send + 'static {
        Self {
            join_handle: Some(thread::spawn(move || callback(initial_data))),
        }
    }

    /// Block until the internal thread has finished and return T
    pub fn block(mut self) -> Result<T, BlockError> {
        // .block() can only be called once, so these .unwrap()s are safe
        let handle = self.join_handle.take().unwrap();
        let data = handle.join().map_err(|_| BlockError::ThreadJoinError)?;
        Ok(data)
    }
}

impl<T> Drop for Thread<T> {
    fn drop(&mut self) {
        if self.join_handle.take().is_some() {
            panic!("Thread has not been await()-ed correctly!");
        }
    }
}

/// Run all currently registered timers
#[must_use = "the UpdateScreen result of running timers should not be ignored"]
pub fn run_all_timers(
    timers: &mut FastHashMap<TimerId, Timer>,
    data: &mut RefAny,
    resources: &mut AppResources,
) -> UpdateScreen {

    let mut should_update_screen = UpdateScreen::DontRedraw;
    let mut timers_to_terminate = Vec::new();

    for (key, timer) in timers.iter_mut() {
        let (should_update, should_terminate) = timer.invoke(TimerCallbackInfo {
            state: data,
            app_resources: resources,
        });

        if should_update == UpdateScreen::Redraw {
            should_update_screen = UpdateScreen::Redraw;
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

/// Remove all tasks that have finished executing
#[must_use = "the UpdateScreen result of running tasks should not be ignored"]
pub fn clean_up_finished_tasks(
    tasks: &mut Vec<Task>,
    timers: &mut FastHashMap<TimerId, Timer>,
) -> UpdateScreen {

    let old_count = tasks.len();
    let mut timers_to_add = Vec::new();

    tasks.retain(|task| {
        if task.is_finished() {
            if let Some(timer) = task.after_completion_timer {
                timers_to_add.push((TimerId::new(), timer));
            }
            false
        } else {
            true
        }
    });

    let timers_is_empty = timers_to_add.is_empty();
    let new_count = tasks.len();

    // Start all the timers that should run after the completion of the task
    for (timer_id, timer) in timers_to_add {
        timers.insert(timer_id, timer);
    }

    if old_count == new_count && timers_is_empty {
        UpdateScreen::DontRedraw
    } else {
        UpdateScreen::Redraw
    }
}