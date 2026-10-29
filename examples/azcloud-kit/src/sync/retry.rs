//! Bounded retries: a request the drive failed for a moment (a 5xx, a dropped connection) is
//! tried again with exponential backoff, full jitter and a ceiling; a 429 waits as long as its
//! `Retry-After` says and never asks early; anything the user must act on (refused
//! credentials, a full drive, a bad request) is not retried at all. One file that keeps
//! failing fails alone: the pass lists it and goes on with the others.
//!
//! - [`with_retries`] runs one request under a [`RetryPolicy`] and a shared [`Gate`] (after a
//!   429 every thread of the pass waits for the gate, not only the one that got it).
//! - [`Retrying`] wraps any [`RemoteStore`] so every call of a pass goes through it.
//! - [`RetryingDrive`] does the same below a drive's encryption: every request to the bucket
//!   (an index page, a pack, a blob) waits for the gate and is tried again on its own.
//! - [`PassBackoff`] spaces whole passes out after failures (the watch loop's schedule) and
//!   keeps a `Retry-After` past the process, so the next pass does not ask early either.

use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use azul_storage::{
    checked::{CheckedUpload, FileCheck},
    BatchDelete, BatchMove, BatchPut, BatchStream, ByteRange, Drive, DriveError, ListPage,
    ListRequest, ObjectInfo, Precondition,
};

use crate::{
    error::{CloudError, CloudResult},
    state::{read_json, write_json},
    store::{Conditional, RemoteObject, RemoteStore},
};

/// The backoff state's file in a pairing's state folder.
pub const RETRY_FILE: &str = "retry.json";

/// How one request is retried: the kit's one policy ([`crate::policy::Policy`]; a pass's
/// requests take [`RetryPolicy::REQUEST`]).
pub use crate::policy::Policy as RetryPolicy;

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
    match error.root() {
        CloudError::Drive(e) => classify_drive(e),
        _ => Verdict::GiveUp,
    }
}

/// Whether the drive's answer `error` passes by itself, as the kit reads every answer
/// ([`crate::answer`]): a 429 or a rate limit (with its `Retry-After`, never less than a
/// second), a 5xx other than a full drive, a dropped connection.
#[must_use]
pub fn classify_drive(error: &DriveError) -> Verdict {
    match crate::answer::of_drive_error(error) {
        // A Retry-After of 0 named another node, which the drive's failover asked already: the
        // request waits for the backoff, not for nothing.
        crate::answer::Outcome::Transient { retry_after, .. } => {
            Verdict::Retry(retry_after.filter(|pause| !pause.is_zero()))
        }
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
pub use crate::policy::random_jitter;

/// The error a request refused before it was sent: the gate stays closed longer than a pass
/// waits. It is a 429 (the server's own answer, kept).
fn gate_closed(left: Duration) -> CloudError {
    CloudError::Drive(drive_gate_closed(left))
}

/// [`gate_closed`] as the drive's own error.
fn drive_gate_closed(left: Duration) -> DriveError {
    DriveError::Service(azul_storage::ServiceError {
        status: 429,
        code: String::from("SlowDown"),
        message: format!(
            "the drive asked to wait {} s more; nothing was sent early",
            left.as_secs()
        ),
        retry_after: Some(left.as_secs().max(1)),
        ..azul_storage::ServiceError::default()
    })
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
    request: impl FnMut() -> CloudResult<T>,
) -> CloudResult<T> {
    retry_loop(policy, gate, jitter, &classify, &gate_closed, request)
}

/// [`with_retries`] for any error: `verdict` tells what passes by itself, `closed` is the
/// error of a request the gate keeps back longer than a pass waits.
fn retry_loop<T, E>(
    policy: &RetryPolicy,
    gate: &Gate,
    jitter: &dyn Fn() -> f64,
    verdict: &dyn Fn(&E) -> Verdict,
    closed: &dyn Fn(Duration) -> E,
    mut request: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut attempt = 0u32;
    loop {
        wait_for_gate(policy, gate).map_err(closed)?;
        attempt += 1;
        let error = match request() {
            Ok(value) => return Ok(value),
            Err(e) => e,
        };
        let Verdict::Retry(server_wait) = verdict(&error) else {
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

/// Waits for the gate; `Err(left)` when it stays closed longer than a pass waits.
fn wait_for_gate(policy: &RetryPolicy, gate: &Gate) -> Result<(), Duration> {
    if let Some(left) = gate.remaining() {
        if left > policy.max_wait {
            return Err(left);
        }
        gate.wait();
    }
    Ok(())
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
        wait_for_gate(&self.policy, &self.gate).map_err(gate_closed)?;
        let sent = self.inner.put_from(key, body, size);
        if let Err(e) = &sent {
            if let Verdict::Retry(Some(wait)) = classify(e) {
                self.gate.hold_for(wait);
            }
        }
        sent
    }
}

/// A drive whose every request goes through [`retry_loop`] under one gate: what passes by
/// itself is tried again, a 429 holds every request back for its `Retry-After`. It sits
/// right on the bucket, below the encryption, so each index page, pack and blob is retried
/// alone and nothing goes out while the drive asked to wait. A body read from a stream cannot
/// be sent twice: it waits for the gate and gets one try (the pass retries its file).
pub struct RetryingDrive {
    pub inner: Arc<dyn Drive>,
    pub policy: RetryPolicy,
    pub gate: Arc<Gate>,
}

impl RetryingDrive {
    /// `inner` under the request policy and a gate of its own.
    #[must_use]
    pub fn new(inner: Arc<dyn Drive>) -> RetryingDrive {
        RetryingDrive {
            inner,
            policy: RetryPolicy::REQUEST,
            gate: Arc::new(Gate::new()),
        }
    }

    fn call<T>(&self, request: impl FnMut() -> Result<T, DriveError>) -> Result<T, DriveError> {
        retry_loop(
            &self.policy,
            &self.gate,
            &random_jitter,
            &classify_drive,
            &drive_gate_closed,
            request,
        )
    }

    /// One try after the gate (a body that cannot be read twice); a 429 still closes the gate.
    fn once<T>(&self, request: impl FnOnce() -> Result<T, DriveError>) -> Result<T, DriveError> {
        wait_for_gate(&self.policy, &self.gate).map_err(drive_gate_closed)?;
        let sent = request();
        if let Err(e) = &sent {
            if let Verdict::Retry(Some(wait)) = classify_drive(e) {
                self.gate.hold_for(wait);
            }
        }
        sent
    }

    /// A batch: sent whole, then only its items that failed for a moment are sent again
    /// (together, after the gate or the backoff), as often as the policy allows.
    fn batch<I: Copy, T>(
        &self,
        items: &[I],
        send: impl Fn(&[I]) -> Vec<Result<T, DriveError>>,
    ) -> Vec<Result<T, DriveError>> {
        let mut results: Vec<Option<Result<T, DriveError>>> = items.iter().map(|_| None).collect();
        let mut open: Vec<usize> = (0..items.len()).collect();
        let mut attempt = 0u32;
        while !open.is_empty() {
            if let Err(left) = wait_for_gate(&self.policy, &self.gate) {
                for i in open.drain(..) {
                    results[i] = Some(Err(drive_gate_closed(left)));
                }
                break;
            }
            attempt += 1;
            let round: Vec<I> = open.iter().map(|&i| items[i]).collect();
            let mut answers = send(&round).into_iter();
            let mut again = Vec::new();
            let mut backoff = false;
            for i in open.drain(..) {
                let answer = answers.next().unwrap_or_else(|| {
                    Err(DriveError::Transport(String::from("the batch gave no answer")))
                });
                if let Err(e) = &answer {
                    if let Verdict::Retry(server_wait) = classify_drive(e) {
                        match server_wait {
                            Some(wait) => self.gate.hold_for(wait),
                            None => backoff = true,
                        }
                        if attempt < self.policy.attempts {
                            again.push(i);
                            continue;
                        }
                    }
                }
                results[i] = Some(answer);
            }
            open = again;
            if backoff && !open.is_empty() {
                std::thread::sleep(self.policy.delay(attempt, random_jitter()));
            }
        }
        results
            .into_iter()
            .map(|r| r.unwrap_or_else(|| Err(DriveError::Transport(String::from("not sent")))))
            .collect()
    }
}

impl Drive for RetryingDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.call(|| self.inner.list(request))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.call(|| self.inner.get(key))
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.call(|| self.inner.get_range(key, range))
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.call(|| self.inner.put(key, bytes))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.call(|| self.inner.delete(key))
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.call(|| self.inner.head(key))
    }

    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.once(|| self.inner.put_from(key, body))
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        // A lost answer may have landed: a retry that then loses the race reads the index
        // again, as any lost compare-and-swap does.
        self.call(|| self.inner.put_if(key, bytes, condition))
    }

    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.call(|| self.inner.copy(from, to))
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.call(|| self.inner.create_folder(prefix))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.call(|| self.inner.rename(from, to))
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.call(|| self.inner.delete_folder(prefix))
    }

    fn local_path(&self, key: &str) -> Option<std::path::PathBuf> {
        self.inner.local_path(key)
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.call(|| self.inner.metadata(key))
    }

    fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        // The file is opened again for every try.
        self.call(|| self.inner.put_file(key, path, progress))
    }

    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &FileCheck,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<CheckedUpload, DriveError> {
        // Opened and checked again for every try: a file that changed meanwhile says so.
        self.call(|| self.inner.put_file_checked(key, path, check, progress))
    }

    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.once(|| self.inner.put_from_if(key, body, condition))
    }

    fn put_many(&self, items: &[BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        self.batch(items, |round| self.inner.put_many(round))
    }

    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        self.batch(items, |round| self.inner.delete_many(round))
    }

    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        self.batch(items, |round| self.inner.move_many(round))
    }

    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        match wait_for_gate(&self.policy, &self.gate) {
            Ok(()) => {
                let sent = self.inner.put_from_many(items);
                for e in sent.iter().filter_map(|r| r.as_ref().err()) {
                    if let Verdict::Retry(Some(wait)) = classify_drive(e) {
                        self.gate.hold_for(wait);
                    }
                }
                sent
            }
            Err(left) => items.iter().map(|_| Err(drive_gate_closed(left))).collect(),
        }
    }
}

/// When the next pass may run, after the last ones failed: kept in the state folder so a
/// `Retry-After` outlives the process.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PassBackoff {
    /// Passes that failed in a row.
    pub failures: u32,
    /// No pass before this (seconds since 1970): the backoff, or the drive's own wait.
    pub not_before: f64,
    /// No request before this (seconds since 1970): the drive's `Retry-After` alone. Even a
    /// pass the user asks for waits for it.
    #[serde(default)]
    pub server_not_before: f64,
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
            Verdict::Retry(Some(server)) => {
                self.server_not_before = self.server_not_before.max(now + server.as_secs_f64());
                server
            }
            Verdict::Retry(None) => policy.delay(self.failures, 0.5 + jitter / 2.0),
            Verdict::GiveUp => Self::CEILING,
        };
        self.not_before = now + wait.as_secs_f64();
        self.last_error = error.to_string();
    }

    /// A 429 seen inside a pass that otherwise went through: the next pass still waits for it.
    pub fn hold_until(&mut self, at: f64) {
        self.not_before = self.not_before.max(at);
        self.server_not_before = self.server_not_before.max(at);
    }

    /// How long a pass the user asked for waits at `now`: only for the drive's `Retry-After`.
    #[must_use]
    pub fn server_wait(&self, now: f64) -> Duration {
        Duration::from_secs_f64((self.server_not_before - now).max(0.0))
    }

    /// How long to wait before the next pass, at `now`.
    #[must_use]
    pub fn wait(&self, now: f64) -> Duration {
        Duration::from_secs_f64((self.not_before - now).max(0.0))
    }
}
