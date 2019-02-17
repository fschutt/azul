use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
    fmt,
    hash::{Hash, Hasher},
};
use {
    dom::{UpdateScreen, DontRedraw},
    app_resources::AppResources,
};

/// Should a daemon terminate or not - used to remove active daemons
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TerminateDaemon {
    /// Remove the daemon from the list of active daemons
    Terminate,
    /// Do nothing and let the daemons continue to run
    Continue,
}

static MAX_DAEMON_ID: AtomicUsize = AtomicUsize::new(0);

/// Generate a new, unique DaemonId
fn new_daemon_id() -> DaemonId {
    DaemonId(MAX_DAEMON_ID.fetch_add(1, Ordering::SeqCst))
}

/// ID for uniquely identifying a daemon
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DaemonId(usize);

impl DaemonId {
    /// Generates a new, unique `DaemonId`.
    pub fn new() -> Self {
        new_daemon_id()
    }
}

/// A `Daemon` is a function that is run on every frame.
///
/// The reason for needing this is simple - there are often a lot of visual tasks
/// (such as animations, fetching the next frame for a GIF or video, etc.)
/// going on, but we don't want to create a new thread for each of these tasks.
///
/// They are fast enough to run under 16ms, so they can run on the main thread.
/// A daemon can also act as a timer, so that a function is called every X duration.
pub struct Daemon<T> {
    /// Stores when the daemon was created (usually acquired by `Instant::now()`)
    pub created: Instant,
    /// When the daemon was last called (`None` only when the daemon hasn't been called yet).
    pub last_run: Option<Instant>,
    /// If the daemon shouldn't start instantly, but rather be delayed by a certain timeframe
    pub delay: Option<Duration>,
    /// How frequently the daemon should run
    /// (i.e. `Some(Duration::from_millis(16))` to run the timer every 16ms).
    /// If set to `None`, (default value) will execute the timer on every frame,
    /// might be  performance intensive.
    pub interval: Option<Duration>,
    /// When to stop the daemon (for example, you can stop the
    /// execution after 5s using `Some(Duration::from_secs(5))`).
    pub timeout: Option<Duration>,
    /// Callback to be called for this daemon
    pub callback: DaemonCallback<T>,
}

pub type DaemonCallbackType<T> = fn(&mut T, app_resources: &mut AppResources) -> (UpdateScreen, TerminateDaemon);

/// Callback that can runs on every frame on the main thread - can modify the app data model
pub struct DaemonCallback<T>(pub DaemonCallbackType<T>);

impl_callback!(DaemonCallback<T>);

impl<T> Daemon<T> {

    /// Create a new daemon
    pub fn new(callback: DaemonCallback<T>,) -> Self {
        Daemon {
            created: Instant::now(),
            last_run: None,
            delay: None,
            interval: None,
            timeout: None,
            callback,
        }
    }

    /// Delays the daemon to not start immediately but rather
    /// start after a certain time frame has elapsed.
    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Converts the daemon into a timer, running the function only
    /// if the given `Duration` has elapsed since the last run
    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    /// Converts the daemon into a countdown, by giving it a maximum duration
    /// (counted from the creation of the Daemon, not the first use).
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Crate-internal: Invokes the daemon if the timer and
    /// the `self.timeout` allow it to
    pub(crate) fn invoke_callback_with_data(
        &mut self,
        data: &mut T,
        app_resources: &mut AppResources)
    -> (UpdateScreen, TerminateDaemon)
    {
        let instant_now = Instant::now();
        let delay = self.delay.unwrap_or_else(|| Duration::from_millis(0));

        // Check if the daemons timeout is reached
        if let Some(timeout) = self.timeout {
            if instant_now - self.created > timeout {
                return (DontRedraw, TerminateDaemon::Terminate);
            }
        }

        if let Some(interval) = self.interval {
            let last_run = match self.last_run {
                Some(s) => s,
                None => self.created + delay,
            };
            if instant_now - last_run < interval {
                return (DontRedraw, TerminateDaemon::Continue);
            }
        }

        let res = (self.callback.0)(data, app_resources);

        self.last_run = Some(instant_now);

        res
    }
}

// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)] for Daemon<T>

impl<T> fmt::Debug for Daemon<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "Daemon {{ \
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

impl<T> Clone for Daemon<T> {
    fn clone(&self) -> Self {
        Daemon { .. *self }
    }
}

impl<T> Hash for Daemon<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.created.hash(state);
        self.last_run.hash(state);
        self.delay.hash(state);
        self.interval.hash(state);
        self.timeout.hash(state);
        self.callback.hash(state);
    }
}

impl<T> PartialEq for Daemon<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.created == rhs.created &&
        self.last_run == rhs.last_run &&
        self.delay == rhs.delay &&
        self.interval == rhs.interval &&
        self.timeout == rhs.timeout &&
        self.callback == rhs.callback
    }
}

impl<T> Eq for Daemon<T> { }

impl<T> Copy for Daemon<T> { }
