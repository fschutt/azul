//! A transport that does not give up when a signal interrupts a wait.
//!
//! A blocking read or write on a socket can come back `ErrorKind::Interrupted` (EINTR): a signal
//! whose handler was installed without `SA_RESTART` (the quit signals of the run loop, a child that
//! exits in a process that handles `SIGCHLD`, a profiler) arrived on the thread while it waited. That
//! is no failure of the connection: the call is to be made again, with the time that is left of the
//! deadline. ureq 3.4's TCP transport hands it up as `Error::Io` (`std` retries `Interrupted` in
//! `write_all` but not in `read`), so a request that was only slow came back as "Interrupted system
//! call (os error 4)", and one a signal hit in the last moment was no timeout. [`Uninterrupted`] wraps
//! each transport the default connector makes (the TLS one over the TCP one included: a read that
//! failed leaves the state of both as it was) and starts the wait again until the deadline it was
//! given; at the deadline the answer is the timeout the wait would have given.

use std::{io::ErrorKind, time::Instant};

use ureq::{
    unversioned::transport::{
        time::Duration, Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout,
        Transport,
    },
    Error,
};

/// The most times one wait is started again (a storm of signals is a fault to report, not to wait
/// out): the last try's answer is the answer.
const MAX_RESTARTS: u32 = 10_000;

/// Runs `attempt` with `timeout`, and again with what is left of it while it comes back
/// `Interrupted`; at the deadline the answer is the timeout of `timeout.reason`.
pub(crate) fn restarting<T>(
    timeout: NextTimeout,
    mut attempt: impl FnMut(NextTimeout) -> Result<T, Error>,
) -> Result<T, Error> {
    let began = Instant::now();
    let mut now = timeout;
    for _ in 0..MAX_RESTARTS {
        match attempt(now) {
            Err(Error::Io(e)) if e.kind() == ErrorKind::Interrupted => {
                if let Duration::Exact(total) = timeout.after {
                    let spent = began.elapsed();
                    if spent >= total {
                        return Err(Error::Timeout(timeout.reason));
                    }
                    now = NextTimeout {
                        after: Duration::Exact(total - spent),
                        reason: timeout.reason,
                    };
                }
            }
            other => return other,
        }
    }
    attempt(now)
}

/// A transport whose waits survive a signal.
#[derive(Debug)]
pub(crate) struct Uninterrupted(pub(crate) Box<dyn Transport>);

impl Transport for Uninterrupted {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.0.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), Error> {
        let inner = &mut self.0;
        restarting(timeout, |t| inner.transmit_output(amount, t))
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, Error> {
        let inner = &mut self.0;
        restarting(timeout, |t| inner.await_input(t))
    }

    fn is_open(&mut self) -> bool {
        self.0.is_open()
    }

    fn is_tls(&self) -> bool {
        self.0.is_tls()
    }
}

/// ureq's default connector (TCP, TLS, proxies), its transports made [`Uninterrupted`].
#[derive(Debug, Default)]
pub(crate) struct UninterruptedConnector(DefaultConnector);

impl Connector<()> for UninterruptedConnector {
    type Out = Box<dyn Transport>;

    fn connect(
        &self,
        details: &ConnectionDetails<'_>,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, Error> {
        Ok(self
            .0
            .connect(details, chained)?
            .map(|transport| -> Box<dyn Transport> { Box::new(Uninterrupted(transport)) }))
    }
}

/// The connector every agent of the engine is made with.
pub(crate) fn connector() -> UninterruptedConnector {
    UninterruptedConnector::default()
}

#[cfg(test)]
#[path = "http_retry_test.rs"]
mod http_retry_test;
