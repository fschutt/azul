//! The agents of the window: what the window keeps of them, and the calls that ask one and take its answer.
//!
//! [`Hub`] is made once at the start: the settings (the settings file's `agent-*` rows, held to what they can mean, with
//! the problems for the log), the environment as the process had it, the pool of jobs and the wiring every backend gets
//! (the keychain, the folders programs live in, the limits, a scratch folder, the transport of the APIs). A conversation
//! asks with [`ask`]: the request is built from what the window knows (`conversation_request`), the pool gives a ticket
//! (or says it is busy), a worker thread runs the job (`agent_pool::run_job`) and every message it sends is taken by
//! [`on_job_message`] on the window's thread, which gives the answer to the conversation. An answer for a file that is not
//! open any more waits in [`Hub::late`] and lands when the file is opened again.
//!
//! stdout, for scripts: `AZREVIEW_AGENT started|done|failed <conversation> <task> ...` (`JobMessage::log_line`: the
//! reason an agent did nothing and what it heard are in that line and nowhere on the screen).

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};

use azul::{prelude::*, task::TerminateTimer, time::SystemTimeDiff};
use azul_appkit::l10n::{t, t_args, Arg};

use crate::{
    agent_azul::{AzulKeychain, AzulTransport},
    agent_backend::{AgentError, BackendKind, Request, Response, Task, WallClock},
    agent_factory::{factory, Wiring},
    agent_pool::{run_job, JobEvent, JobMessage, JobSpec, Pool, Ticket},
    agent_process::Host,
    agent_review::Reply,
    agent_settings::{AgentSettings, Env, Keys, Problem, TaskSettings},
    chips, conversation_request, AppState,
};

/// How often the pool is asked for the workers that never reported, ms.
const REAP_TICK_MS: u64 = 5_000;
/// How many answers wait for a file that is not open: more than a session has, and a bound on the memory.
pub const MAX_LATE: usize = 200;

/// What an answer is, as plain data for the conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Reply(Reply),
    /// Why there is none, in the reviewer's words.
    Failed(String),
}

impl Outcome {
    /// How this end of a conversation is told.
    #[must_use]
    pub fn finish(&self) -> Finish {
        match self {
            Self::Reply(reply) if reply.is_no_action() => Finish::Quiet,
            Self::Reply(reply) if !reply.suggestions.is_empty() => Finish::Suggested(reply.suggestions.len()),
            Self::Reply(_) => Finish::Done,
            Self::Failed(_) => Finish::Failed,
        }
    }

    /// The state of the conversation's chip once this has landed.
    #[must_use]
    pub fn chip_state(&self) -> chips::State {
        match self.finish() {
            Finish::Done => chips::State::Done,
            Finish::Suggested(_) => chips::State::Suggested,
            Finish::Quiet => chips::State::Draft,
            Finish::Failed => chips::State::Failed,
        }
    }
}

/// An answer for a conversation of a file that was not open when it came.
#[derive(Debug, Clone, PartialEq)]
pub struct Late {
    pub file: String,
    pub note: u64,
    pub outcome: Outcome,
}

/// How the end of a conversation's agent job was, for whoever listens: the companion's host forwards it to the phone (a
/// haptic when the agent is finished).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// Words came back and there is nothing to take.
    Done,
    /// This many suggestions wait for the reviewer.
    Suggested(usize),
    /// The agent chose to do nothing: nothing is shown.
    Quiet,
    /// The agent could not answer.
    Failed,
}

/// A conversation that finished: in which file (as the tree names it), which one, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub file: String,
    pub conversation: u64,
    pub finish: Finish,
}

/// Who is told when a conversation finishes: the window calls it on its own thread, right when the answer is taken (also
/// for a file that is not open), so it must not block.
pub trait Events: Send + Sync {
    fn finished(&self, event: &Finished);
}

/// Nobody listens.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoEvents;

impl Events for NoEvents {
    fn finished(&self, _event: &Finished) {}
}

/// Where an answer went.
#[derive(Debug, Clone, PartialEq)]
pub enum Answered {
    /// It is for the open file: the conversation takes it now.
    Lands(Outcome),
    /// It is for a file that is not open: it waits there, and the file's chip has this state meanwhile.
    Waits(chips::State),
}

/// What a job that runs is for.
#[derive(Debug, Clone)]
struct InFlight {
    file: String,
    note: u64,
    task: Task,
}

/// What the window keeps of its agents.
pub struct Hub {
    pub settings: AgentSettings,
    /// What was wrong with the settings (for the log).
    pub problems: Vec<Problem>,
    pub pool: Pool,
    wiring: Arc<Wiring>,
    inflight: HashMap<u64, InFlight>,
    /// Answers for files that are not open.
    pub late: Vec<Late>,
    /// A dictation that is over asks the agent by itself.
    pub auto_ask: bool,
    /// Who is told when a conversation finishes.
    pub events: Arc<dyn Events>,
    /// The timer that looks for workers that never report is armed.
    reaping: bool,
}

impl Hub {
    /// The agents of a window: `values` are the settings file's rows, `env` the process's environment, `backend` one
    /// backend for every task (the mock, for a script), `mock_delay_ms` how long the mock takes to answer.
    #[must_use]
    pub fn new(
        values: &BTreeMap<String, String>,
        env: impl IntoIterator<Item = (String, String)>,
        data_root: &Path,
        backend: Option<BackendKind>,
        mock_delay_ms: u64,
        auto_ask: bool,
    ) -> Self {
        let env: Vec<(String, String)> = env.into_iter().collect();
        let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone());
        let home = env.iter().find(|(k, _)| k == "HOME").map(|(_, v)| v.clone());
        let (mut settings, problems) = AgentSettings::from_values(values);
        if let Some(kind) = backend {
            for task in Task::all() {
                settings.tasks.insert(task, TaskSettings::defaults(task, kind));
            }
        }
        let keys = Keys { env: Env::from_pairs(env), keychain: Some(Arc::new(AzulKeychain)) };
        let mut wiring = Wiring::new(keys, Host::new(path.as_deref(), home.as_deref()), settings.limits());
        wiring.scratch = Some(data_root.join("agent-scratch"));
        wiring.mock_delay_ms = mock_delay_ms;
        wiring.transport = Some(Arc::new(AzulTransport::new("AzReview")));
        let pool = Pool::new(settings.max_concurrent);
        Self {
            settings,
            problems,
            pool,
            wiring: Arc::new(wiring),
            inflight: HashMap::new(),
            late: Vec::new(),
            auto_ask,
            events: Arc::new(NoEvents),
            reaping: false,
        }
    }

    /// An answer came for `flight`: whoever listens is told that the conversation finished, and the answer goes where it
    /// belongs (`open` is the file that is open now): to the conversation, or to wait for its file.
    fn answered(&mut self, open: Option<&str>, flight: &InFlight, outcome: Outcome) -> Answered {
        self.events.finished(&Finished { file: flight.file.clone(), conversation: flight.note, finish: outcome.finish() });
        if open == Some(flight.file.as_str()) {
            return Answered::Lands(outcome);
        }
        let state = outcome.chip_state();
        self.keep_late(Late { file: flight.file.clone(), note: flight.note, outcome });
        Answered::Waits(state)
    }

    /// How many jobs run.
    #[must_use]
    pub fn running(&self) -> usize {
        self.pool.running()
    }

    /// An agent is at work on conversation `note` of `file`.
    #[must_use]
    pub fn is_working_on(&self, file: &str, note: u64) -> bool {
        self.inflight.values().any(|f| f.file == file && f.note == note)
    }

    /// Keeps an answer for a file that is not open (the oldest goes past `MAX_LATE`, and the log says so).
    fn keep_late(&mut self, late: Late) {
        if self.late.len() >= MAX_LATE {
            let gone = self.late.remove(0);
            eprintln!("[AzReview] an answer for {} was dropped: {MAX_LATE} wait for files that are not open", gone.file);
        }
        self.late.push(late);
    }

    /// The answers waiting for `file`, taken out.
    pub fn take_late(&mut self, file: &str) -> Vec<Late> {
        let (mine, rest): (Vec<Late>, Vec<Late>) = std::mem::take(&mut self.late).into_iter().partition(|l| l.file == file);
        self.late = rest;
        mine
    }
}

/// What an answer is to the reviewer: the reply, or why there is none in words (this app's, then the agent's own).
#[must_use]
pub fn outcome_of(result: &Result<Response, AgentError>) -> Outcome {
    match result {
        Ok(response) => match response.reply() {
            Some(reply) => Outcome::Reply(reply.clone()),
            None => Outcome::Failed(t("azreview-agent-no-reply")),
        },
        Err(error) => {
            let args: Vec<(&str, Arg)> = error.l10n_args().into_iter().map(|(k, v)| (k, Arg::from(v))).collect();
            Outcome::Failed(t_args(error.l10n_key(), &args))
        }
    }
}

/// Gives `outcome` to the conversation: the chip state it comes to, and what landed (none for a failure).
pub fn land(note: &mut crate::model::Note, outcome: Outcome, now_unix_ms: u64) -> chips::State {
    match outcome {
        Outcome::Reply(reply) => {
            let _ = note.talk.receive(&reply, now_unix_ms);
        }
        Outcome::Failed(why) => note.talk.fail(why),
    }
    note.talk.chip_state()
}

// ==== Asking ====

/// The worker's start: the ticket and the job (taken out once).
struct JobInit {
    ticket: Ticket,
    spec: Option<JobSpec>,
}

extern "C" fn job_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((ticket, spec)) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut j| j.spec.take().map(|spec| (j.ticket.clone(), spec)))
    else {
        return;
    };
    let mut clock = WallClock::new();
    run_job(spec, &ticket, &mut clock, &mut |message| {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_job_message,
            RefAny::new(Some(message)),
        )));
    });
}

/// Why an ask came to nothing, as the conversation says it.
fn refused(error: &AgentError) -> String {
    let args: Vec<(&str, Arg)> = error.l10n_args().into_iter().map(|(k, v)| (k, Arg::from(v))).collect();
    t_args(error.l10n_key(), &args)
}

/// Asks an agent about conversation `note_id`: the request is built from the words and the page, the job started on a
/// thread of its own. A pool that is full, a conversation that works already, or no file open, is the conversation's own
/// word (a failure it shows), never a window that waits.
pub fn ask(data: &mut RefAny, info: &mut CallbackInfo, note_id: u64) -> Update {
    let spec = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(file) = s.file().cloned() else {
            return Update::DoNothing;
        };
        let Some(at) = s.notes.iter().position(|n| n.id == note_id) else {
            return Update::DoNothing;
        };
        if !s.notes[at].talk.can_ask() {
            return Update::DoNothing;
        }
        let task = Task::Editing;
        let now = s.voice.now_ms();
        let timeout = s.hub.settings.task(task).timeout_secs;
        let started = match s.hub.pool.start(note_id, task, now, timeout) {
            Ok(started) => started,
            Err(error) => {
                s.notes[at].talk.fail(refused(&error));
                s.sync_chip(note_id);
                return Update::RefreshDom;
            }
        };
        let language = crate::voice::language_of(&s.kit);
        let unix_ms = unix_now_ms();
        let strokes = s.strokes_for_request(note_id);
        let request = conversation_request::review_request(
            &s.notes[at],
            &file,
            &language,
            strokes,
            Some(s.root.clone()).filter(|r| !r.as_os_str().is_empty()),
            task,
            unix_ms,
        );
        s.notes[at].talk.asking();
        s.hub.inflight.insert(started.ticket.id, InFlight { file: file.display.clone(), note: note_id, task });
        let spec = JobSpec {
            conversation: note_id,
            request: Request::Review(request),
            limits: s.hub.settings.limits(),
            make: factory(task, s.hub.settings.task(task), s.hub.wiring.clone()),
        };
        s.sync_chip(note_id);
        (started.ticket, spec)
    };
    let (ticket, spec) = spec;
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(JobInit { ticket, spec: Some(spec) }), data.clone(), job_worker),
    );
    ensure_reaper(data, info);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The wall clock, Unix ms.
#[must_use]
pub fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A worker's message, on the window's thread.
extern "C" fn on_job_message(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(message) = msg.downcast_mut::<Option<JobMessage>>().and_then(|mut m| m.take()) else {
        return Update::DoNothing;
    };
    let mut save = false;
    {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(flight) = s.hub.inflight.get(&message.job).cloned() else {
            // a job that was given up on: its late message is ignored
            return Update::DoNothing;
        };
        println!("{}", message.log_line(flight.task));
        let JobEvent::Done(result) = message.event else {
            return Update::DoNothing;
        };
        let now = s.voice.now_ms();
        if s.hub.pool.finish(message.job, now).is_none() {
            s.hub.inflight.remove(&message.job);
            return Update::DoNothing;
        }
        s.hub.inflight.remove(&message.job);
        let open = s.file().map(|f| f.display.clone());
        match s.hub.answered(open.as_deref(), &flight, outcome_of(&result)) {
            Answered::Lands(outcome) => {
                let unix = unix_now_ms();
                if let Some(note) = s.notes.iter_mut().find(|n| n.id == flight.note) {
                    land(note, outcome, unix);
                    save = true;
                }
                s.sync_chip(flight.note);
            }
            // the file is not open: the answer waits for it, and its chip is told now
            Answered::Waits(state) => {
                s.chips.set_state_of(&flight.file, flight.note, state);
            }
        }
    }
    if save {
        crate::save_session(&app, &mut info);
    }
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

// ==== The workers that never report ====

/// Arms the timer that gives up on workers past their deadline, once, while a job runs.
fn ensure_reaper(data: &mut RefAny, info: &mut CallbackInfo) {
    let wanted = data.downcast_mut::<AppState>().is_some_and(|mut s| {
        let wanted = !s.hub.reaping;
        if wanted {
            s.hub.reaping = true;
        }
        wanted
    });
    if wanted {
        let timer = Timer::create(data.clone(), on_reap_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(REAP_TICK_MS)));
        info.add_timer(TimerId::unique(), timer);
    }
}

extern "C" fn on_reap_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let (gone, idle) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return TimerCallbackReturn::terminate_unchanged();
        };
        let now = s.voice.now_ms();
        let gone = s.hub.pool.reap(now);
        let mut changed = Vec::new();
        for ended in &gone {
            if let Some(flight) = s.hub.inflight.remove(&ended.id) {
                let why = t("azreview-agent-no-answer");
                println!("AZREVIEW_AGENT failed {} {} timeout {why}", flight.note, flight.task.key());
                let open = s.file().map(|f| f.display.clone());
                match s.hub.answered(open.as_deref(), &flight, Outcome::Failed(why)) {
                    Answered::Lands(outcome) => {
                        let unix = unix_now_ms();
                        if let Some(note) = s.notes.iter_mut().find(|n| n.id == flight.note) {
                            land(note, outcome, unix);
                        }
                        s.sync_chip(flight.note);
                    }
                    Answered::Waits(state) => {
                        s.chips.set_state_of(&flight.file, flight.note, state);
                    }
                }
                changed.push(flight.note);
            }
        }
        let idle = s.hub.pool.in_flight() == 0;
        if idle {
            s.hub.reaping = false;
        }
        (changed, idle)
    };
    let update = if gone.is_empty() { Update::DoNothing } else { Update::RefreshDom };
    if !gone.is_empty() {
        info.callback_info.trigger_all_virtual_view_rerender();
    }
    TimerCallbackReturn::create(update, if idle { TerminateTimer::Terminate } else { TerminateTimer::Continue })
}

#[cfg(test)]
#[path = "agent_window_test.rs"]
mod agent_window_test;
