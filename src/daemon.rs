use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
    fmt,
    hash::{Hash, Hasher},
};
use {
    dom::UpdateScreen,
    app_resources::AppResources,
};

/// Should a daemon terminate or not - used to remove active daemons
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TerminateDaemon {
    Terminate,
    Continue,
}

static MAX_DAEMON_ID: AtomicUsize = AtomicUsize::new(0);

pub fn new_daemon_id() -> DaemonId {
    DaemonId(MAX_DAEMON_ID.fetch_add(1, Ordering::SeqCst))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DaemonId(usize);

pub struct Daemon<T> {
    created: Instant,
    last_run: Instant,
    run_every: Option<Duration>,
    max_timeout: Option<Duration>,
    callback: DaemonCallback<T>,
    pub(crate) id: DaemonId,
}

pub struct DaemonCallback<T>(pub fn(&mut T, app_resources: &mut AppResources) -> (UpdateScreen, TerminateDaemon));

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for DaemonCallback<T>

impl<T> fmt::Debug for DaemonCallback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DaemonCallback @ 0x{:x}", self.0 as usize)
    }
}

impl<T> Clone for DaemonCallback<T> {
    fn clone(&self) -> Self {
        DaemonCallback(self.0.clone())
    }
}

impl<T> Hash for DaemonCallback<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        state.write_usize(self.0 as usize);
    }
}

impl<T> PartialEq for DaemonCallback<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 as usize == rhs.0 as usize
    }
}

impl<T> Eq for DaemonCallback<T> { }

impl<T> Copy for DaemonCallback<T> { }

impl<T> Daemon<T> {
    pub fn unique(callback: DaemonCallback<T>) -> Self {
        Self::with_id(callback, new_daemon_id())
    }

    pub fn with_id(callback: DaemonCallback<T>, id: DaemonId) -> Self {
        Daemon {
            created: Instant::now(),
            last_run: Instant::now(),
            run_every: None,
            max_timeout: None,
            callback,
            id,
        }
    }

    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self {
            max_timeout: Some(timeout),
            .. self
        }
    }

    pub fn run_every(self, every: Duration) -> Self {
        Self {
            run_every: Some(every),
            last_run: self.last_run - every,
            .. self
        }
    }

    pub(crate) fn invoke_callback_with_data(&mut self, data: &mut T, app_resources: &mut AppResources) -> (UpdateScreen, TerminateDaemon) {

        // Check if the deamons timeout is reached
        if let Some(max_timeout) = self.max_timeout {
            if Instant::now() - self.created > max_timeout {
                return (UpdateScreen::DontRedraw, TerminateDaemon::Terminate);
            }
        }

        if let Some(run_every) = self.run_every {
            if Instant::now() - self.last_run < run_every {
                return (UpdateScreen::DontRedraw, TerminateDaemon::Continue);
            }
        }

        let res = (self.callback.0)(data, app_resources);

        self.last_run = Instant::now();

        res
    }
}

// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)] for Deamon<T>

impl<T> fmt::Debug for Daemon<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Daemon {{ \
            created: {:?}, \
            run_every: {:?}, \
            last_run: {:?}, \
            max_timeout: {:?}, \
            callback: {:?}, \
            id: {:?}, \
        }}",
        self.created,
        self.run_every,
        self.last_run,
        self.max_timeout,
        self.callback,
        self.id)
    }
}

impl<T> Clone for Daemon<T> {
    fn clone(&self) -> Self {
        Daemon {
            created: self.created,
            run_every: self.run_every,
            last_run: self.last_run,
            max_timeout: self.max_timeout,
            callback: self.callback,
            id: self.id,
        }
    }
}

impl<T> Hash for Daemon<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.created.hash(state);
        self.run_every.hash(state);
        self.last_run.hash(state);
        self.max_timeout.hash(state);
        self.callback.hash(state);
        self.id.hash(state);
    }
}

impl<T> PartialEq for Daemon<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.created == rhs.created &&
        self.run_every == rhs.run_every &&
        self.last_run == rhs.last_run &&
        self.max_timeout == rhs.max_timeout &&
        self.callback == rhs.callback &&
        self.id == rhs.id
    }
}

impl<T> Eq for Daemon<T> { }

impl<T> Copy for Daemon<T> { }