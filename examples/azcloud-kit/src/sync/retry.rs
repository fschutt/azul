//! Bounded retries: a request the drive failed for a moment (a 5xx, a dropped connection) is
//! tried again with exponential backoff, full jitter and a ceiling; a 429 waits as long as its
//! `Retry-After` says and never asks early; anything the user must act on (refused
//! credentials, a full drive, a bad request) is not retried at all. One file that keeps
//! failing fails alone: the pass lists it and goes on with the others.
//!
//! - [`with_retries`] runs one request under a [`RetryPolicy`] and a shared [`Gate`] (after a
//!   429 every thread of the pass waits for the gate, not only the one that got it).
//! - [`Retrying`] wraps any [`RemoteStore`] so every call of a pass goes through it.
//! - [`PassBackoff`] spaces whole passes out after failures (the watch loop's schedule) and
//!   keeps a `Retry-After` past the process, so the next pass does not ask early either.

use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{CloudError, CloudResult},
    state::{read_json, write_json},
    store::{Conditional, RemoteObject, RemoteStore},
};

/// The backoff state's file in a pairing's state folder.
pub const RETRY_FILE: &str = "retry.json";

/// How one request is retried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Tries in all (the first one included).
    pub attempts: u32,
    /// The first wait; each next one doubles it, up to `ceiling`.
    pub base: Duration,
    pub ceiling: Duration,
    /// The longest a request waits for a `Retry-After` inside a pass; a longer one fails the
    /// request (nothing is sent early) and the pass's schedule keeps it.
    pub max_wait: Duration,
}

impl RetryPolicy {
    /// A request of a pass: four tries, 100 ms doubling up to 5 s.
    pub const REQUEST: RetryPolicy = RetryPolicy {
        attempts: 4,
        base: Duration::from_millis(100),
        ceiling: Duration::from_secs(5),
        max_wait: Duration::from_secs(30),
    };

    /// The wait before try `attempt` (1 = the first retry): `base * 2^(attempt-1)` capped at
    /// the ceiling, times `jitter` in [0, 1) ("full jitter": devices that failed together do
    /// not come back together).
    #[must_use]
    pub fn delay(&self, attempt: u32, jitter: f64) -> Duration {
        let exp = self
            .base
            .saturating_mul(1u32 << attempt.saturating_sub(1).min(20))
            .min(self.ceiling);
        exp.mul_f64(jitter.clamp(0.0, 1.0))
    }
}

/// What to do after a failed request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Try again; with the server's own wait (a 429's `Retry-After`) when it gave one.
    Retry(Option<Duration>),
    /// Do not: retrying cannot help (refused credentials, a full drive, a bad request, a lost
    /// compare-and-swap - the caller reads again instead).
    GiveUp,
}

/// Whether `error` passes by itself.
#[must_use]
pub fn classify(error: &CloudError) -> Verdict {
    use azul_storage::DriveError;
    match error.root() {
        CloudError::Drive(DriveError::Service(service)) => match service.status {
            429 => Verdict::Retry(Some(Duration::from_secs(service.retry_after.unwrap_or(1)))),
            500..=599 if service.status != 507 => {
                Verdict::Retry(service.retry_after.map(Duration::from_secs))
            }
            _ => Verdict::GiveUp,
        },
        CloudError::Drive(DriveError::Transport(_)) => Verdict::Retry(None),
        _ => Verdict::GiveUp,
    }
}

/// The moment before which no request of this pairing may go out (a 429's `Retry-After`).
/// Shared by every thread of a pass.
#[derive(Debug, Default)]
pub struct Gate {
    not_before: Mutex<Option<Instant>>,
}

impl Gate {
    /// A gate that is open.
    #[must_use]
    pub fn new() -> Gate {
        Gate::default()
    }

    /// Closes the gate for `wait` from now (a later end wins over an earlier one).
    pub fn hold_for(&self, wait: Duration) {
        let until = Instant::now() + wait;
        let mut slot = self.not_before.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.map_or(true, |t| t < until) {
            *slot = Some(until);
        }
    }

    /// How long the gate stays closed; `None` when it is open.
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        let slot = self.not_before.lock().unwrap_or_else(PoisonError::into_inner);
        slot.and_then(|t| t.checked_duration_since(Instant::now()))
            .filter(|d| !d.is_zero())
    }

    /// Waits until the gate opens.
    pub fn wait(&self) {
        while let Some(left) = self.remaining() {
            std::thread::sleep(left);
        }
    }
}

/// A number in [0, 1) from the OS random source (the jitter of every wait).
#[must_use]
pub fn random_jitter() -> f64 {
    let mut bytes = [0u8; 8];
    if getrandom::getrandom(&mut bytes).is_err() {
        return 0.5;
    }
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// The error a request refused before it was sent: the gate stays closed longer than a pass
/// waits. It is a 429 (the server's own answer, kept).
fn gate_closed(left: Duration) -> CloudError {
    CloudError::Drive(azul_storage::DriveError::Service(azul_storage::ServiceError {
        status: 429,
        code: String::from("SlowDown"),
        message: format!(
            "the drive asked to wait {} s more; nothing was sent early",
            left.as_secs()
        ),
        retry_after: Some(left.as_secs().max(1)),
        ..azul_storage::ServiceError::default()
    }))
}

/// Runs `request` under `policy`: waits for the `gate` before every try, retries what
/// passes by itself, closes the gate for a 429's `Retry-After`. `jitter` gives the random
/// factor of each wait.
///
/// # Errors
///
/// The last error, when the tries ran out or retrying cannot help.
pub fn with_retries<T>(
    policy: &RetryPolicy,
    gate: &Gate,
    jitter: &dyn Fn() -> f64,
    mut request: impl FnMut() -> CloudResult<T>,
) -> CloudResult<T> {
    let mut attempt = 0u32;
    loop {
        if let Some(left) = gate.remaining() {
            if left > policy.max_wait {
                return Err(gate_closed(left));
            }
            gate.wait();
        }
        attempt += 1;
        let error = match request() {
            Ok(value) => return Ok(value),
            Err(e) => e,
        };
        let Verdict::Retry(server_wait) = classify(&error) else {
            return Err(error);
        };
        if let Some(wait) = server_wait {
            gate.hold_for(wait);
        }
        if attempt >= policy.attempts {
            return Err(error);
        }
        if server_wait.is_none() {
            std::thread::sleep(policy.delay(attempt, jitter()));
        }
    }
}

/// A store whose every call goes through [`with_retries`] (one gate for all of them).
pub struct Retrying<S> {
    pub inner: S,
    pub policy: RetryPolicy,
    pub gate: Arc<Gate>,
}

impl<S: RemoteStore> Retrying<S> {
    /// `inner` under the request policy and a gate of its own.
    #[must_use]
    pub fn new(inner: S) -> Retrying<S> {
        Retrying {
            inner,
            policy: RetryPolicy::REQUEST,
            gate: Arc::new(Gate::new()),
        }
    }

    fn call<T>(&self, request: impl FnMut() -> CloudResult<T>) -> CloudResult<T> {
        with_retries(&self.policy, &self.gate, &random_jitter, request)
    }
}

impl<S: RemoteStore> RemoteStore for Retrying<S> {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.call(|| self.inner.get_unless(key, etag))
    }

    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        self.call(|| self.inner.fetch(key, size))
    }

    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.call(|| self.inner.put(key, data))
    }

    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        // A conditional PUT whose answer was lost may have landed: it is retried as it is, and
        // a lost race after that reads the index again (the merge sees its own commit).
        self.call(|| self.inner.put_if(key, data, if_match))
    }

    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        self.call(|| self.inner.head(key))
    }

    fn delete(&self, key: &str) -> CloudResult<()> {
        self.call(|| self.inner.delete(key))
    }

    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.call(|| self.inner.list(prefix))
    }

    fn put_from(&self, key: &str, body: &mut dyn Read, size: u64) -> CloudResult<String> {
        // A body read once cannot be sent again: one try (the pass retries the file).
        if let Some(left) = self.gate.remaining() {
            if left > self.policy.max_wait {
                return Err(gate_closed(left));
            }
            self.gate.wait();
        }
        self.inner.put_from(key, body, size)
    }
}

/// When the next pass may run, after the last ones failed: kept in the state folder so a
/// `Retry-After` outlives the process.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PassBackoff {
    /// Passes that failed in a row.
    pub failures: u32,
    /// No pass before this (seconds since 1970).
    pub not_before: f64,
    /// Why the last one failed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_error: String,
}

impl PassBackoff {
    /// The first wait after a failed pass, and the longest.
    pub const BASE: Duration = Duration::from_secs(15);
    pub const CEILING: Duration = Duration::from_secs(15 * 60);

    /// The state kept in `state_dir`.
    #[must_use]
    pub fn load(state_dir: &Path) -> PassBackoff {
        read_json(&state_dir.join(RETRY_FILE))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Writes it into `state_dir`.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, state_dir: &Path) -> CloudResult<()> {
        write_json(&state_dir.join(RETRY_FILE), self, true)
    }

    /// A pass went through: the next one may run at once.
    pub fn succeeded(&mut self) {
        *self = PassBackoff::default();
    }

    /// A pass failed with `error` at `now`: the next waits for the backoff (with `jitter`)
    /// or the drive's `Retry-After`, whichever is later. A refusal the user must act on
    /// (credentials, a full drive) waits the ceiling: asking more often changes nothing.
    pub fn failed(&mut self, error: &CloudError, now: f64, jitter: f64) {
        self.failures = self.failures.saturating_add(1);
        let policy = RetryPolicy {
            attempts: u32::MAX,
            base: Self::BASE,
            ceiling: Self::CEILING,
            max_wait: Self::CEILING,
        };
        let wait = match classify(error) {
            // The drive's own wait is the wait; otherwise half fixed, half jitter, so a pass
            // never comes back right away.
            Verdict::Retry(Some(server)) => server,
            Verdict::Retry(None) => policy.delay(self.failures, 0.5 + jitter / 2.0),
            Verdict::GiveUp => Self::CEILING,
        };
        self.not_before = now + wait.as_secs_f64();
        self.last_error = error.to_string();
    }

    /// A 429 seen inside a pass that otherwise went through: the next pass still waits for it.
    pub fn hold_until(&mut self, at: f64) {
        if at > self.not_before {
            self.not_before = at;
        }
    }

    /// How long to wait before the next pass, at `now`.
    #[must_use]
    pub fn wait(&self, now: f64) -> Duration {
        Duration::from_secs_f64((self.not_before - now).max(0.0))
    }
}
