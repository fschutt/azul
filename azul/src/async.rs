//! Asynchronous task helpers (`Timer`, `Task`, `Thread`)

use std::{
    sync::{Arc, Mutex, Weak, atomic::{AtomicUsize, Ordering}},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
    fmt,
    hash::{Hash, Hasher},
};
use {
    dom::{UpdateScreen, DontRedraw},
    app_resources::AppResources,
};

/// Should a timer terminate or not - used to remove active timers
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TerminateTimer {
    /// Remove the timer from the list of active timers
    Terminate,
    /// Do nothing and let the timers continue to run
    Continue,
}

static MAX_DAEMON_ID: AtomicUsize = AtomicUsize::new(0);

/// Generate a new, unique TimerId
fn new_timer_id() -> TimerId {
    TimerId(MAX_DAEMON_ID.fetch_add(1, Ordering::SeqCst))
}

/// ID for uniquely identifying a timer
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TimerId(usize);

impl TimerId {
    /// Generates a new, unique `TimerId`.
    pub fn new() -> Self {
        new_timer_id()
    }
}

/// A `Timer` is a function that is run on every frame.
///
/// The reason for needing this is simple - there are often a lot of visual tasks
/// (such as animations, fetching the next frame for a GIF or video, etc.)
/// going on, but we don't want to create a new thread for each of these tasks.
///
/// They are fast enough to run under 16ms, so they can run on the main thread.
/// A timer can also act as a timer, so that a function is called every X duration.
pub struct Timer<T> {
    /// Stores when the timer was created (usually acquired by `Instant::now()`)
    pub created: Instant,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: Option<Instant>,
    /// If the timer shouldn't start instantly, but rather be delayed by a certain timeframe
    pub delay: Option<Duration>,
    /// How frequently the timer should run
    /// (i.e. `Some(Duration::from_millis(16))` to run the timer every 16ms).
    /// If set to `None`, (default value) will execute the timer on every frame,
    /// might be  performance intensive.
    pub interval: Option<Duration>,
    /// When to stop the timer (for example, you can stop the
    /// execution after 5s using `Some(Duration::from_secs(5))`).
    pub timeout: Option<Duration>,
    /// Callback to be called for this timer
    pub callback: TimerCallback<T>,
}

pub type TimerCallbackType<T> = fn(&mut T, app_resources: &mut AppResources) -> (UpdateScreen, TerminateTimer);

/// Callback that can runs on every frame on the main thread - can modify the app data model
pub struct TimerCallback<T>(pub TimerCallbackType<T>);

impl_callback!(TimerCallback<T>);

impl<T> Timer<T> {

    /// Create a new timer
    pub fn new(callback: TimerCallbackType<T>,) -> Self {
        Timer {
            created: Instant::now(),
            last_run: None,
            delay: None,
            interval: None,
            timeout: None,
            callback: TimerCallback(callback),
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
    pub(crate) fn invoke_callback_with_data(
        &mut self,
        data: &mut T,
        app_resources: &mut AppResources)
    -> (UpdateScreen, TerminateTimer)
    {
        let instant_now = Instant::now();
        let delay = self.delay.unwrap_or_else(|| Duration::from_millis(0));

        // Check if the timers timeout is reached
        if let Some(timeout) = self.timeout {
            if instant_now - self.created > timeout {
                return (DontRedraw, TerminateTimer::Terminate);
            }
        }

        if let Some(interval) = self.interval {
            let last_run = match self.last_run {
                Some(s) => s,
                None => self.created + delay,
            };
            if instant_now - last_run < interval {
                return (DontRedraw, TerminateTimer::Continue);
            }
        }

        let res = (self.callback.0)(data, app_resources);

        self.last_run = Some(instant_now);

        res
    }
}

// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)] for Timer<T>

impl<T> fmt::Debug for Timer<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "Timer {{ \
                created: {:?}, \
                last_run: {:?}, \
                delay: {:?}, \
                interval: {:?}, \
                timeout: {:?}, \
                callback: {:?}, \
            }}",
            self.created,
            self.last_run,
            self.delay,
            self.interval,
            self.timeout,
            self.callback,
        )
    }
}

impl<T> Clone for Timer<T> {
    fn clone(&self) -> Self {
        Timer { .. *self }
    }
}

impl<T> Hash for Timer<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.created.hash(state);
        self.last_run.hash(state);
        self.delay.hash(state);
        self.interval.hash(state);
        self.timeout.hash(state);
        self.callback.hash(state);
    }
}

impl<T> PartialEq for Timer<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.created == rhs.created &&
        self.last_run == rhs.last_run &&
        self.delay == rhs.delay &&
        self.interval == rhs.interval &&
        self.timeout == rhs.timeout &&
        self.callback == rhs.callback
    }
}

impl<T> Eq for Timer<T> { }

impl<T> Copy for Timer<T> { }

pub struct Task<T> {
    // Task is in progress
    join_handle: Option<JoinHandle<()>>,
    dropcheck: Weak<()>,
    /// Timers that run directly after completion of this task
    pub(crate) after_completion_timers: Vec<(TimerId, Timer<T>)>
}

impl<T> Task<T> {
    pub fn new<U: Send + 'static>(data: &Arc<Mutex<U>>, callback: fn(Arc<Mutex<U>>, Arc<()>)) -> Self {

        let thread_check = Arc::new(());
        let thread_weak = Arc::downgrade(&thread_check);
        let app_state_clone = data.clone();

        let thread_handle = thread::spawn(move || {
            callback(app_state_clone, thread_check)
        });

        Self {
            join_handle: Some(thread_handle),
            dropcheck: thread_weak,
            after_completion_timers: Vec::new(),
        }
    }

    /// Returns true if the task has been finished, false otherwise
    pub(crate) fn is_finished(&self) -> bool {
        self.dropcheck.upgrade().is_none()
    }

    /// Stores timers that will run after the task has finished.
    ///
    /// Often necessary to "clean up" or copy data from the background task into the UI.
    #[inline]
    pub fn then(mut self, deamons: &[(TimerId, Timer<T>)]) -> Self {
        self.after_completion_timers.extend(deamons.iter().cloned());
        self
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let Some(thread_handle) = self.join_handle.take() {
            let _ = thread_handle.join().unwrap();
        }
    }
}

pub struct Thread<T> {
    data: Option<Arc<Mutex<T>>>,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AwaitError {
    ArcUnlockError,
    ThreadJoinError,
    MutexIntoInnerError,
}

impl<T> Thread<T> {

    /// Creates a new thread that spawns a certain (pure) function on a separate thread.
    /// This is a workaround until `await` is implemented. Note that invoking this function
    /// will create an OS-level thread
    ///
    /// ```rust
    /// # use azul::async::Thread;
    /// #
    /// fn pure_function(input: usize) -> usize { input + 1 }
    ///
    /// let thread_1 = Thread::new(5, pure_function);
    /// let thread_2 = Thread::new(10, pure_function);
    /// let thread_3 = Thread::new(20, pure_function);
    ///
    /// // thread_1, thread_2 and thread_3 run in parallel here...
    ///
    /// let result_1 = thread_1.await();
    /// let result_2 = thread_2.await();
    /// let result_3 = thread_3.await();
    ///
    /// assert_eq!(result_1, Ok(6));
    /// assert_eq!(result_2, Ok(11));
    /// assert_eq!(result_3, Ok(21));
    /// ```
    pub fn new<U>(initial_data: U, callback: fn(U) -> T) -> Self where T: Send + 'static, U: Send + 'static {

        use std::mem;

        // Reserve memory for T and zero it
        let data = Arc::new(Mutex::new(unsafe { mem::zeroed() }));
        let data_arc = data.clone();

        // For some reason, Rust doesn't realize that we're *moving* the data into the
        // child thread, which is why the 'static is unnecessary - that would only be necessary
        // if we'd reference the data from the main thread
        let thread_handle = thread::spawn(move || {
            *data_arc.lock().unwrap() = callback(initial_data);
        });

        Self {
            data: Some(data),
            join_handle: Some(thread_handle),
        }
    }

    /// Block until the internal thread has finished and return T
    pub fn await(mut self) -> Result<T, AwaitError> {

        // .await() can only be called once, so these .unwrap()s are safe
        let handle = self.join_handle.take().unwrap();
        let data = self.data.take().unwrap();

        handle.join().map_err(|_| AwaitError::ThreadJoinError)?;

        let data_arc = Arc::try_unwrap(data).map_err(|_| AwaitError::ArcUnlockError)?;
        let data = data_arc.into_inner().map_err(|_| AwaitError::MutexIntoInnerError)?;

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
