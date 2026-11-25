//! The agents at work: how many run at once, which was cancelled, which is overdue - and the
//! function a worker thread runs for one job.
//!
//! The [`Pool`] lives on the window's thread (nothing here is shared but each job's cancel flag, an
//! `Arc<AtomicBool>` that belongs to that job): the window asks it for a [`Ticket`] before it starts
//! a worker, hands the ticket to the worker, and gives the ticket back with [`Pool::finish`] when the
//! worker's last message arrives. A cap bounds the jobs that run at once ([`DEFAULT_CAP`]: two
//! conversations), a second request for a conversation cancels its first, and a worker that never
//! reports is forgotten once its deadline and a grace time have passed ([`Pool::reap`]), so a lost
//! worker cannot hold a slot for ever.
//!
//! [`run_job`] is the worker's whole body: it builds the backend (on the worker, so a keychain read
//! or a search of the PATH never waits on the window), runs the request, and reports through a
//! callback; the window's thread glue turns each [`JobMessage`] into the window's message, as the
//! dictation worker does.
//!
//! # How the window uses the agents (in order)
//!
//! 1. At the start, once: the settings (`AgentSettings::from_values(&settings.values)`: its
//!    problems are for the log), the environment (`Env::from_pairs(std::env::vars())`), a [`Pool`]
//!    of `settings.max_concurrent`, and a `Wiring` (the keys with `AzulKeychain` as the keychain, a
//!    `Host` of the PATH and the home folder, the limits `settings.limits()`, a scratch folder under
//!    the data root, `AzulTransport` as the transport of the APIs).
//! 2. A conversation asks: build the request from what the window knows (`agent_review`: the words
//!    as timed utterances, the strokes as samples, the code around them), `pool.start(conversation,
//!    task, now_ms, timeout_secs)` (a [`AgentError::Busy`] says so on the bottom bar), then a worker
//!    thread that calls [`run_job`] with a [`JobSpec`] whose factory is
//!    `agent_factory::factory(task, settings.task(task), wiring)`, and a `ThreadWriteBackMsg` for
//!    every [`JobMessage`].
//! 3. The window's message callback: `JobEvent::Done` gives `pool.finish(job, now_ms)` (`None`: a job
//!    that was given up on, its message is ignored), then the `Response` (suggestions become the
//!    yellow chips, the answer the conversation's text, `diagnostics` the log) or the failure
//!    (`l10n_key()` and `l10n_args()` through the window's `t_args`; `kind_name()` for the e2e lines).
//! 4. Cancel is `pool.cancel(id)` (the worker kills its process or drops its answer); a timer calls
//!    `pool.reap(now_ms)` for the workers that never report.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    agent_backend::{run_task, AgentError, Backend, Clock, Control, Request, Response, Task},
    agent_limits::Limits,
};

/// Jobs that run at once when nothing says otherwise.
pub const DEFAULT_CAP: usize = 2;
pub const MIN_CAP: usize = 1;
pub const MAX_CAP: usize = 8;

/// How long past its deadline a job is waited for before it is given up on, ms.
pub const GRACE_MS: u64 = 10_000;

/// A running job: what the worker is handed.
#[derive(Debug, Clone)]
pub struct Ticket {
    pub id: u64,
    cancel: Arc<AtomicBool>,
}

impl Ticket {
    /// The window cancelled this job.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
struct Job {
    id: u64,
    conversation: u64,
    task: Task,
    started_ms: u64,
    deadline_ms: u64,
    cancel: Arc<AtomicBool>,
}

impl Job {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// What a started job is.
#[derive(Debug, Clone)]
pub struct Started {
    pub ticket: Ticket,
    /// The job of the same conversation that this one replaced (it was cancelled).
    pub replaced: Option<u64>,
}

/// A job that finished (or was given up on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub id: u64,
    pub conversation: u64,
    pub task: Task,
    /// How long it was running, ms.
    pub elapsed_ms: u64,
}

/// The jobs of one window.
#[derive(Debug)]
pub struct Pool {
    cap: usize,
    next: u64,
    jobs: Vec<Job>,
}

impl Default for Pool {
    fn default() -> Self {
        Self::new(DEFAULT_CAP)
    }
}

impl Pool {
    /// A pool that runs `cap` jobs at once (held to 1 to 8).
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.clamp(MIN_CAP, MAX_CAP),
            next: 1,
            jobs: Vec::new(),
        }
    }

    #[must_use]
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Changes the cap (held to 1 to 8); the word about it when it was held.
    pub fn set_cap(&mut self, cap: usize) -> Option<String> {
        self.cap = cap.clamp(MIN_CAP, MAX_CAP);
        (cap != self.cap).then(|| {
            format!(
                "{cap} agents at once is outside {MIN_CAP} to {MAX_CAP}: using {}",
                self.cap
            )
        })
    }

    /// The jobs that count against the cap: the ones not cancelled (a cancelled job is being torn
    /// down and frees its slot at once).
    #[must_use]
    pub fn running(&self) -> usize {
        self.jobs.iter().filter(|j| !j.cancelled()).count()
    }

    /// Jobs of any kind that have not reported yet (cancelled ones included).
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.jobs.len()
    }

    /// Whether the conversation has a job that was not cancelled.
    #[must_use]
    pub fn is_running(&self, conversation: u64) -> bool {
        self.jobs
            .iter()
            .any(|j| j.conversation == conversation && !j.cancelled())
    }

    /// Starts a job for `conversation`: its ticket. A job of the same conversation still running
    /// is cancelled first (the reviewer asked again).
    ///
    /// # Errors
    /// [`AgentError::Busy`] when the cap is reached.
    pub fn start(
        &mut self,
        conversation: u64,
        task: Task,
        now_ms: u64,
        timeout_secs: u32,
    ) -> Result<Started, AgentError> {
        let replaced = self
            .jobs
            .iter()
            .find(|j| j.conversation == conversation && !j.cancelled())
            .map(|j| {
                j.cancel.store(true, Ordering::Relaxed);
                j.id
            });
        // a job it replaced is cancelled and no longer counts, so a conversation that asks again is
        // never the one that finds the pool full
        let running = self.running();
        if running >= self.cap {
            return Err(AgentError::Busy {
                running,
                cap: self.cap,
            });
        }
        let id = self.next;
        self.next += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.push(Job {
            id,
            conversation,
            task,
            started_ms: now_ms,
            deadline_ms: now_ms
                .saturating_add(u64::from(timeout_secs) * 1_000)
                .saturating_add(GRACE_MS),
            cancel: cancel.clone(),
        });
        Ok(Started {
            ticket: Ticket { id, cancel },
            replaced,
        })
    }

    /// Cancels job `id`; whether it was there and not cancelled yet.
    pub fn cancel(&mut self, id: u64) -> bool {
        self.jobs
            .iter()
            .find(|j| j.id == id && !j.cancelled())
            .map(|j| j.cancel.store(true, Ordering::Relaxed))
            .is_some()
    }

    /// Cancels the jobs of a conversation; how many.
    pub fn cancel_conversation(&mut self, conversation: u64) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.conversation == conversation && !j.cancelled())
            .map(|j| j.cancel.store(true, Ordering::Relaxed))
            .count()
    }

    /// Cancels every job; how many were running.
    pub fn cancel_all(&mut self) -> usize {
        self.jobs
            .iter()
            .filter(|j| !j.cancelled())
            .map(|j| j.cancel.store(true, Ordering::Relaxed))
            .count()
    }

    /// The worker of job `id` reported its end: the job is done. `None` for a job that is not here
    /// (already given up on: its late message is to be ignored).
    pub fn finish(&mut self, id: u64, now_ms: u64) -> Option<Ended> {
        let at = self.jobs.iter().position(|j| j.id == id)?;
        let job = self.jobs.remove(at);
        Some(Ended {
            id: job.id,
            conversation: job.conversation,
            task: job.task,
            elapsed_ms: now_ms.saturating_sub(job.started_ms),
        })
    }

    /// Gives up on the jobs past their deadline and the grace time: cancelled and forgotten, so the
    /// window can say that no answer came.
    pub fn reap(&mut self, now_ms: u64) -> Vec<Ended> {
        let mut gone = Vec::new();
        self.jobs.retain(|j| {
            if now_ms > j.deadline_ms {
                j.cancel.store(true, Ordering::Relaxed);
                gone.push(Ended {
                    id: j.id,
                    conversation: j.conversation,
                    task: j.task,
                    elapsed_ms: now_ms.saturating_sub(j.started_ms),
                });
                false
            } else {
                true
            }
        });
        gone
    }
}

// ==== The worker ====

/// What a worker reports.
#[derive(Debug)]
pub enum JobEvent {
    /// The worker has begun.
    Started,
    /// The worker is done: the answer or why there is none.
    Done(Box<Result<Response, AgentError>>),
}

/// A report of a worker, with whose it is.
#[derive(Debug)]
pub struct JobMessage {
    pub job: u64,
    pub conversation: u64,
    pub event: JobEvent,
}

impl JobMessage {
    /// The line the end-to-end scripts read on standard output (`AZREVIEW_AGENT started|done|failed
    /// <conversation> <task> ...`), as the dictation has its `AZREVIEW_DICTATION` lines.
    #[must_use]
    pub fn log_line(&self, task: Task) -> String {
        let key = task.key();
        match &self.event {
            JobEvent::Started => {
                format!("AZREVIEW_AGENT started {} {key} job={}", self.conversation, self.job)
            }
            JobEvent::Done(result) => match result.as_ref() {
                Ok(response) => {
                    let items = response
                        .reply()
                        .map(|r| r.suggestions.len())
                        .or_else(|| response.cut_list().map(|l| l.segments.len()))
                        .unwrap_or(0);
                    format!(
                        "AZREVIEW_AGENT done {} {key} {} items={items} ms={}",
                        self.conversation,
                        response.identity.kind.key(),
                        response.usage.elapsed_ms
                    )
                }
                Err(error) => format!(
                    "AZREVIEW_AGENT failed {} {key} {} {}",
                    self.conversation,
                    error.kind_name(),
                    error.message().replace('\n', " ")
                ),
            },
        }
    }
}

/// Makes the backend on the worker.
pub type Factory = Box<dyn FnOnce() -> Result<Box<dyn Backend>, AgentError> + Send>;

/// Everything a worker needs for one job.
pub struct JobSpec {
    pub conversation: u64,
    pub request: Request,
    pub limits: Limits,
    pub make: Factory,
}

/// The whole body of a worker: reports that it began, makes the backend, runs the request, reports
/// the end. A backend that panics ends in a failure message, not in a window that waits for ever.
pub fn run_job(
    spec: JobSpec,
    ticket: &Ticket,
    clock: &mut dyn Clock,
    send: &mut dyn FnMut(JobMessage),
) {
    let JobSpec {
        conversation,
        request,
        limits,
        make,
    } = spec;
    let job = ticket.id;
    send(JobMessage {
        job,
        conversation,
        event: JobEvent::Started,
    });
    let cancel = || ticket.cancelled();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mut backend = make()?;
        let mut ctl = Control::new(clock, &cancel);
        run_task(backend.as_mut(), &request, &limits, &mut ctl)
    }))
    .unwrap_or_else(|_| {
        Err(AgentError::Unsupported {
            why: "the agent's worker stopped unexpectedly".to_string(),
        })
    });
    send(JobMessage {
        job,
        conversation,
        event: JobEvent::Done(Box::new(outcome)),
    });
}

#[cfg(test)]
#[path = "agent_pool_test.rs"]
mod agent_pool_test;
