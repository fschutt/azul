//! The boundary between AzReview and whatever answers it: the agents' backends.
//!
//! Every backend - `claude -p`, `gemini`, the Anthropic and Gemini HTTP APIs, the scripted mock -
//! differs only in TRANSPORT: it takes a [`Call`] (a system prompt, a prompt, a tool policy, a
//! working folder) and returns text with its usage ([`Backend::complete`]). What goes into the call
//! is built by ONE pure function per kind of request ([`agent_review::build_call`]) and what comes
//! back is read by ONE strict, bounded parser ([`agent_review::parse_body`]); [`run_task`] is the
//! three steps in a row. So the mock goes through the same parser as the real ones, and a new
//! backend is a new transport and nothing else.
//!
//! A task is a row of [`TASKS`]: its key in the settings, its tool policy, its defaults. A fourth
//! task is a new row, a new request builder and a new parser; the settings, the pool and the mock
//! are keyed by [`Task`] and need no edit.
//!
//! Nothing here writes a file, starts a window or reads the environment: a backend only reads the
//! context it is given, a failure is a typed [`AgentError`] in words the user can act on (each with
//! its localization key), and a request runs on a worker thread ([`crate::agent_pool`]) that the
//! window can cancel.

use std::{path::PathBuf, time::Instant};

use crate::{
    agent_limits::{Limits, Notes},
    agent_review::{self, Reply, ReviewRequest},
};

// ==== Tasks ====

/// What an agent is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Task {
    /// Read-only questions and analysis of the code: the answer is text.
    Exploration,
    /// Proposals: rewrite a comment, replace or insert lines: the answer is suggestions.
    Editing,
    /// A cut list for a video of the review session.
    Export,
}

/// What a task may do to the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tools {
    /// Nothing: the prompt holds everything the agent needs.
    None,
    /// Read the files of the workspace (and search them), nothing else.
    ReadOnly,
}

/// One row per task: where its settings start.
#[derive(Debug)]
pub struct TaskSpec {
    pub task: Task,
    /// The task's word in the settings file (`agent-<key>-<field>`).
    pub key: &'static str,
    /// The localization key of the task's name.
    pub label: &'static str,
    pub tools: Tools,
    /// How long a call may take, seconds.
    pub timeout_secs: u32,
    /// The most tokens a call may write.
    pub max_output_tokens: u32,
    /// The most a `claude -p` call may cost, in US cents (0: no limit).
    pub budget_cents: u32,
    /// The most turns of a `claude -p` call (0: no limit; the flag is a hidden one of the CLI).
    pub max_turns: u32,
    /// The model of the CLI's own family names the task starts with: an alias, so it follows the
    /// newest model of the family.
    pub claude_model: &'static str,
}

/// The tasks and their defaults.
pub static TASKS: [TaskSpec; 3] = [
    TaskSpec {
        task: Task::Exploration,
        key: "exploration",
        label: "azreview-agent-task-exploration",
        tools: Tools::ReadOnly,
        timeout_secs: 180,
        max_output_tokens: 4_096,
        budget_cents: 100,
        max_turns: 0,
        claude_model: "sonnet",
    },
    TaskSpec {
        task: Task::Editing,
        key: "editing",
        label: "azreview-agent-task-editing",
        tools: Tools::None,
        timeout_secs: 90,
        max_output_tokens: 2_048,
        budget_cents: 30,
        max_turns: 0,
        claude_model: "haiku",
    },
    TaskSpec {
        task: Task::Export,
        key: "export",
        label: "azreview-agent-task-export",
        tools: Tools::None,
        timeout_secs: 300,
        max_output_tokens: 8_192,
        budget_cents: 100,
        max_turns: 0,
        claude_model: "sonnet",
    },
];

impl Task {
    /// Every task, in the order of [`TASKS`].
    pub fn all() -> impl Iterator<Item = Task> {
        TASKS.iter().map(|spec| spec.task)
    }

    /// The task's row.
    #[must_use]
    pub fn spec(self) -> &'static TaskSpec {
        TASKS
            .iter()
            .find(|spec| spec.task == self)
            .unwrap_or(&TASKS[0])
    }

    /// The task's word in the settings file.
    #[must_use]
    pub fn key(self) -> &'static str {
        self.spec().key
    }

    /// The task a settings word names.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        TASKS
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| spec.task)
    }
}

// ==== Backends ====

/// What answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackendKind {
    /// The Claude Code CLI in print mode (`claude -p`; no key needed, it uses its own login).
    ClaudePrint,
    /// The Anthropic Messages API over HTTPS.
    AnthropicApi,
    /// The `gemini` CLI in headless mode.
    GeminiCli,
    /// The Gemini `generateContent` API over HTTPS.
    GeminiApi,
    /// A script that answers after a scripted delay (the end-to-end tests, a demo).
    Mock,
}

impl BackendKind {
    pub const ALL: [Self; 5] = [
        Self::ClaudePrint,
        Self::AnthropicApi,
        Self::GeminiCli,
        Self::GeminiApi,
        Self::Mock,
    ];

    /// The word in the settings file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ClaudePrint => "claude-print",
            Self::AnthropicApi => "anthropic-api",
            Self::GeminiCli => "gemini-cli",
            Self::GeminiApi => "gemini-api",
            Self::Mock => "mock",
        }
    }

    /// The backend a settings word names.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The localization key of the backend's name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudePrint => "azreview-agent-backend-claude-print",
            Self::AnthropicApi => "azreview-agent-backend-anthropic-api",
            Self::GeminiCli => "azreview-agent-backend-gemini-cli",
            Self::GeminiApi => "azreview-agent-backend-gemini-api",
            Self::Mock => "azreview-agent-backend-mock",
        }
    }

    /// The backend's name in English, for logs.
    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
            Self::ClaudePrint => "claude -p",
            Self::AnthropicApi => "Anthropic API",
            Self::GeminiCli => "gemini CLI",
            Self::GeminiApi => "Gemini API",
            Self::Mock => "mock",
        }
    }

    /// It runs a program.
    #[must_use]
    pub const fn is_process(self) -> bool {
        matches!(self, Self::ClaudePrint | Self::GeminiCli)
    }

    /// It talks HTTPS.
    #[must_use]
    pub const fn is_http(self) -> bool {
        matches!(self, Self::AnthropicApi | Self::GeminiApi)
    }

    /// A key reference means something to it (an API needs one; a CLI hands it to the program).
    #[must_use]
    pub const fn takes_key(self) -> bool {
        !matches!(self, Self::Mock)
    }

    /// The program of a CLI backend.
    #[must_use]
    pub const fn program(self) -> Option<&'static str> {
        match self {
            Self::ClaudePrint => Some("claude"),
            Self::GeminiCli => Some("gemini"),
            _ => None,
        }
    }

    /// The address of an HTTP backend's service.
    #[must_use]
    pub const fn default_base_url(self) -> Option<&'static str> {
        match self {
            Self::AnthropicApi => Some("https://api.anthropic.com"),
            Self::GeminiApi => Some("https://generativelanguage.googleapis.com"),
            _ => None,
        }
    }
}

/// Who answered: the backend and the model it was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub kind: BackendKind,
    /// The model the settings name (empty: the backend's own default).
    pub model: String,
}

impl Identity {
    /// `claude -p, haiku`, `gemini CLI, its default model`.
    #[must_use]
    pub fn label(&self) -> String {
        if self.model.trim().is_empty() {
            format!("{}, its default model", self.kind.english())
        } else {
            format!("{}, {}", self.kind.english(), self.model.trim())
        }
    }
}

// ==== Time and cancellation ====

/// What tells the time and waits: the wall clock, or one a test moves.
pub trait Clock {
    fn now_ms(&mut self) -> u64;
    fn sleep_ms(&mut self, ms: u64);
}

/// The wall clock of a worker.
#[derive(Debug)]
pub struct WallClock(Instant);

impl WallClock {
    #[must_use]
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for WallClock {
    fn now_ms(&mut self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// A clock that moves only when something sleeps on it (or [`ManualClock::advance`]): a scripted
/// delay of seconds takes no time. A process under test does take real time, so a clock made
/// [`ManualClock::with_real_pause`] also lets that much real time pass on every sleep.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now: u64,
    pause_ms: u64,
}

impl ManualClock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every sleep also waits `ms` of real time.
    #[must_use]
    pub fn with_real_pause(ms: u64) -> Self {
        Self {
            now: 0,
            pause_ms: ms,
        }
    }

    pub fn advance(&mut self, ms: u64) {
        self.now = self.now.saturating_add(ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&mut self) -> u64 {
        self.now
    }

    fn sleep_ms(&mut self, ms: u64) {
        self.advance(ms);
        if self.pause_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.pause_ms));
        }
    }
}

/// Nothing cancels it: what a test (or a call that cannot be cancelled) hands as the cancel check.
#[must_use]
pub fn never_cancelled() -> bool {
    false
}

/// What a backend is handed besides the call: the clock to time and wait with, and the question
/// whether the window cancelled this call (looked at between two steps of any wait).
pub struct Control<'a> {
    pub clock: &'a mut dyn Clock,
    pub cancel: &'a dyn Fn() -> bool,
}

impl<'a> Control<'a> {
    pub fn new(clock: &'a mut dyn Clock, cancel: &'a dyn Fn() -> bool) -> Self {
        Self { clock, cancel }
    }

    #[must_use]
    pub fn cancelled(&self) -> bool {
        (self.cancel)()
    }

    pub fn now_ms(&mut self) -> u64 {
        self.clock.now_ms()
    }

    pub fn sleep_ms(&mut self, ms: u64) {
        self.clock.sleep_ms(ms);
    }
}

// ==== Requests, calls, answers ====

/// What the window asks an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Exploration or editing, about strokes and the code around them.
    Review(ReviewRequest),
}

impl Request {
    #[must_use]
    pub fn task(&self) -> Task {
        match self {
            Self::Review(review) => review.task,
        }
    }

    /// The conversation the request belongs to.
    #[must_use]
    pub fn conversation(&self) -> u64 {
        match self {
            Self::Review(review) => review.conversation,
        }
    }
}

/// A request made into what a model is sent.
#[derive(Debug)]
pub struct Call<'a> {
    /// What it was made from: the mock reads it, a real backend does not.
    pub request: &'a Request,
    /// The standing instructions: the task and the format of the answer.
    pub system: String,
    /// What is asked now: the strokes, the words, the code.
    pub prompt: String,
    /// The folder an agent with tools reads from (the workspace); none for a task without tools.
    pub workdir: Option<PathBuf>,
    pub tools: Tools,
    /// What was capped or dropped to make it.
    pub diagnostics: Vec<String>,
}

/// What a backend counted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// What the service says the call cost, US dollars.
    pub cost_usd: Option<f64>,
    /// How long it took, ms (filled in by [`run_task`]).
    pub elapsed_ms: u64,
    pub turns: Option<u32>,
}

/// What a backend answered: the text of the model's reply and what it counted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Completion {
    pub text: String,
    pub usage: Usage,
    /// The model that answered, as the service says (it may differ from the one asked for).
    pub model: String,
}

/// What a request came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Review(Reply),
}

/// The finished answer to a request.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub body: Body,
    pub usage: Usage,
    pub identity: Identity,
    /// What was capped, repaired or dropped, in words.
    pub diagnostics: Vec<String>,
}

impl Response {
    /// The first `max` characters of what the answer says (the "first words" of a backend test).
    #[must_use]
    pub fn first_words(&self, max: usize) -> String {
        let text = match &self.body {
            Body::Review(reply) => {
                if reply.answer.trim().is_empty() {
                    format!("{} suggestion(s)", reply.suggestions.len())
                } else {
                    reply.answer.trim().to_string()
                }
            }
        };
        crate::agent_limits::cap_chars(&text.replace('\n', " "), max).0
    }
}

/// A transport: something that answers a [`Call`] with text.
pub trait Backend: Send {
    /// Who this is.
    fn identity(&self) -> Identity;

    /// Sends the call and waits for the text of the answer. It looks at `ctl.cancelled()` between
    /// the steps of any wait and gives up with [`AgentError::Cancelled`] (a backend that cannot be
    /// interrupted says so in its own docs and checks before and after).
    ///
    /// # Errors
    /// [`AgentError`] in words the user can act on.
    fn complete(
        &mut self,
        call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError>;
}

/// Builds the call for `request`, sends it through `backend`, reads the answer: the three steps
/// of every request, whoever answers.
///
/// # Errors
/// [`AgentError`]: the request could not be built, the backend failed, or the answer could not be
/// read.
pub fn run_task(
    backend: &mut dyn Backend,
    request: &Request,
    limits: &Limits,
    ctl: &mut Control<'_>,
) -> Result<Response, AgentError> {
    if ctl.cancelled() {
        return Err(AgentError::Cancelled);
    }
    let call = match request {
        Request::Review(review) => agent_review::build_call(request, review, limits)?,
    };
    let started = ctl.now_ms();
    let completion = backend.complete(&call, ctl)?;
    let elapsed_ms = ctl.now_ms().saturating_sub(started);
    let mut notes = Notes::new();
    notes.extend(call.diagnostics.iter().cloned());
    let body = match request {
        Request::Review(review) => Body::Review(agent_review::parse_body(
            review,
            &completion.text,
            limits,
            &mut notes,
        )?),
    };
    Ok(Response {
        body,
        usage: Usage {
            elapsed_ms,
            ..completion.usage
        },
        identity: backend.identity(),
        diagnostics: notes.finish(),
    })
}

// ==== Failures ====

/// Why a call came to nothing, in terms the user can act on.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentError {
    /// The program is not there: where it was looked for.
    NotInstalled {
        program: String,
        searched: Vec<String>,
    },
    /// The program is there but not signed in.
    NotLoggedIn { program: String, detail: String },
    /// The variable or keychain item the settings name holds no key.
    KeyMissing { reference: String },
    /// The key could not be read (the keychain refused or is not available).
    KeyDenied { reference: String, why: String },
    /// No answer in time.
    Timeout { secs: u32 },
    /// The window cancelled the call.
    Cancelled,
    /// Too many agents are working already.
    Busy { running: usize, cap: usize },
    /// The program could not be started.
    Spawn { program: String, why: String },
    /// The program stopped without an answer: its exit code and the end of what it said.
    Exit {
        program: String,
        code: Option<i32>,
        detail: String,
    },
    /// The service answered with an error status.
    Http {
        status: u16,
        kind: String,
        detail: String,
    },
    /// The service is busy or the account is over a limit.
    RateLimited {
        retry_after_secs: Option<u32>,
        detail: String,
    },
    /// The service could not be reached.
    Network { why: String },
    /// The call would cost more than the limit.
    Budget { limit_usd: f64 },
    /// The agent used its turns without an answer.
    MaxTurns { turns: u32 },
    /// The agent declined (a safety stop, a refusal).
    Refused { detail: String },
    /// What came back could not be read as an answer: why, and the start of it.
    BadOutput { why: String, excerpt: String },
    /// Something is bigger than a limit.
    TooLarge {
        what: String,
        size: usize,
        limit: usize,
    },
    /// The settings cannot be used.
    Settings { why: String },
    /// This cannot be done here.
    Unsupported { why: String },
}

impl AgentError {
    /// The localization key of the sentence that says it (`resources/*.ftl`).
    #[must_use]
    pub fn l10n_key(&self) -> &'static str {
        match self {
            Self::NotInstalled { .. } => "azreview-agent-error-not-installed",
            Self::NotLoggedIn { .. } => "azreview-agent-error-not-logged-in",
            Self::KeyMissing { .. } => "azreview-agent-error-key-missing",
            Self::KeyDenied { .. } => "azreview-agent-error-key-denied",
            Self::Timeout { .. } => "azreview-agent-error-timeout",
            Self::Cancelled => "azreview-agent-error-cancelled",
            Self::Busy { .. } => "azreview-agent-error-busy",
            Self::Spawn { .. } => "azreview-agent-error-spawn",
            Self::Exit { .. } => "azreview-agent-error-exit",
            Self::Http { .. } => "azreview-agent-error-http",
            Self::RateLimited { .. } => "azreview-agent-error-rate-limited",
            Self::Network { .. } => "azreview-agent-error-network",
            Self::Budget { .. } => "azreview-agent-error-budget",
            Self::MaxTurns { .. } => "azreview-agent-error-max-turns",
            Self::Refused { .. } => "azreview-agent-error-refused",
            Self::BadOutput { .. } => "azreview-agent-error-bad-output",
            Self::TooLarge { .. } => "azreview-agent-error-too-large",
            Self::Settings { .. } => "azreview-agent-error-settings",
            Self::Unsupported { .. } => "azreview-agent-error-unsupported",
        }
    }

    /// The short stable name of the failure (`timeout`, `not-installed`): for logs and scripts.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        // `azreview-agent-error-not-installed`: what follows the third hyphen
        self.l10n_key().splitn(4, '-').nth(3).unwrap_or("unknown")
    }

    /// The arguments of the localized sentence, by the names its placeholders have.
    #[must_use]
    pub fn l10n_args(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NotInstalled { program, searched } => vec![
                ("program", program.clone()),
                (
                    "where",
                    if searched.is_empty() {
                        "the PATH".to_string()
                    } else {
                        searched.join(", ")
                    },
                ),
            ],
            Self::NotLoggedIn { program, .. } | Self::Spawn { program, .. } => {
                let mut args = vec![("program", program.clone())];
                if let Self::Spawn { why, .. } = self {
                    args.push(("why", why.clone()));
                }
                args
            }
            Self::KeyMissing { reference } => vec![("reference", reference.clone())],
            Self::KeyDenied { reference, why } => {
                vec![("reference", reference.clone()), ("why", why.clone())]
            }
            Self::Timeout { secs } => vec![("secs", secs.to_string())],
            Self::Cancelled => Vec::new(),
            Self::Busy { running, cap } => {
                vec![("running", running.to_string()), ("cap", cap.to_string())]
            }
            Self::Exit {
                program,
                code,
                detail,
            } => vec![
                ("program", program.clone()),
                (
                    "code",
                    code.map_or_else(|| "-".to_string(), |c| c.to_string()),
                ),
                ("detail", detail.clone()),
            ],
            Self::Http {
                status,
                kind,
                detail,
            } => vec![
                ("status", status.to_string()),
                ("kind", kind.clone()),
                ("detail", detail.clone()),
            ],
            Self::RateLimited { detail, .. } | Self::Refused { detail } => {
                vec![("detail", detail.clone())]
            }
            Self::Network { why } | Self::Settings { why } | Self::Unsupported { why } => {
                vec![("why", why.clone())]
            }
            Self::Budget { limit_usd } => vec![("limit", format!("${limit_usd:.2}"))],
            Self::MaxTurns { turns } => vec![("turns", turns.to_string())],
            Self::BadOutput { why, excerpt } => {
                vec![("why", why.clone()), ("excerpt", excerpt.clone())]
            }
            Self::TooLarge { what, size, limit } => vec![
                ("what", what.clone()),
                ("size", size.to_string()),
                ("limit", limit.to_string()),
            ],
        }
    }

    /// The sentence in English: what `resources/en.ftl` says with the same arguments.
    #[must_use]
    pub fn message(&self) -> String {
        let args = self.l10n_args();
        let arg = |name: &str| {
            args.iter()
                .find(|(n, _)| *n == name)
                .map_or(String::new(), |(_, v)| v.clone())
        };
        match self {
            Self::NotInstalled { .. } => {
                format!("{} is not installed (looked in {})", arg("program"), arg("where"))
            }
            Self::NotLoggedIn { .. } => format!(
                "{} is not signed in: open a terminal, run {} and sign in, then try again",
                arg("program"),
                arg("program")
            ),
            Self::KeyMissing { .. } => format!(
                "The key {} is not set: put the key into that environment variable or keychain item (never into the settings)",
                arg("reference")
            ),
            Self::KeyDenied { .. } => {
                format!("The key {} could not be read: {}", arg("reference"), arg("why"))
            }
            Self::Timeout { .. } => format!("Timeout after {} s", arg("secs")),
            Self::Cancelled => "Cancelled".to_string(),
            Self::Busy { .. } => format!(
                "{} agents are working already (at most {} at once): wait for one to finish",
                arg("running"),
                arg("cap")
            ),
            Self::Spawn { .. } => format!("{} could not be started: {}", arg("program"), arg("why")),
            Self::Exit { .. } => format!(
                "{} stopped with exit code {}: {}",
                arg("program"),
                arg("code"),
                arg("detail")
            ),
            Self::Http { .. } => format!(
                "The service answered {} {}: {}",
                arg("status"),
                arg("kind"),
                arg("detail")
            ),
            Self::RateLimited { .. } => {
                format!("The service is busy or over its limit: {}", arg("detail"))
            }
            Self::Network { .. } => format!("The service could not be reached: {}", arg("why")),
            Self::Budget { .. } => format!(
                "The call would cost more than the limit of {}: raise it in the settings",
                arg("limit")
            ),
            Self::MaxTurns { .. } => format!(
                "The agent used all {} turns without an answer: raise the limit in the settings or ask a narrower question",
                arg("turns")
            ),
            Self::Refused { .. } => format!("The agent declined: {}", arg("detail")),
            Self::BadOutput { .. } => {
                format!("The answer could not be read ({}): {}", arg("why"), arg("excerpt"))
            }
            Self::TooLarge { .. } => format!(
                "{} is too large ({} of {}): shorten it or raise the limit in the settings",
                arg("what"),
                arg("size"),
                arg("limit")
            ),
            Self::Settings { .. } => format!("The agent settings are not usable: {}", arg("why")),
            Self::Unsupported { .. } => format!("Not supported here: {}", arg("why")),
        }
    }

    /// Trying again may help (a timeout, a busy service, a lost connection, a server error).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Timeout { .. }
            | Self::RateLimited { .. }
            | Self::Network { .. }
            | Self::Busy { .. } => true,
            Self::Http { status, .. } => *status >= 500,
            _ => false,
        }
    }

    /// The seconds a busy service asked to wait, when it said.
    #[must_use]
    pub fn retry_after_secs(&self) -> Option<u32> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => *retry_after_secs,
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for AgentError {}

// ==== Secrets in text ====

/// The beginnings of the tokens services hand out.
static KEY_PREFIXES: [&str; 8] = [
    "sk-ant-",
    "sk-",
    "AIza",
    "ya29.",
    "ghp_",
    "gho_",
    "github_pat_",
    "xox",
];

/// The most characters a word of a message is read as one token.
const MAX_TOKEN: usize = 512;

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// `text` with the secrets taken out: every one of `secrets` (a key the caller holds), what looks
/// like a service's token, the value after `Bearer`, after a key's name in a query or a header. A
/// message that is shown or logged goes through this first (an HTTP library's errors name the URL,
/// a program's standard error may echo its environment).
#[must_use]
pub fn scrub(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.len() >= 6 {
            out = out.replace(secret, "***");
        }
    }
    // the value after a marker, up to a stop character
    let after = |s: &str, markers: &[&str], stop: fn(char) -> bool| {
        let lower = s.to_ascii_lowercase();
        let mut result = String::with_capacity(s.len());
        let mut at = 0;
        'scan: while at < s.len() {
            // a marker starts a word: `key=` is a key's name, `monkey=` is not
            let word_start = at == 0 || !s.as_bytes()[at - 1].is_ascii_alphanumeric();
            for marker in markers {
                if word_start && lower[at..].starts_with(marker) {
                    let value_start = at + marker.len();
                    let value_start = value_start
                        + s[value_start..]
                            .chars()
                            .take_while(|c| *c == ' ' || *c == '\t')
                            .count();
                    let value_len: usize = s[value_start..]
                        .chars()
                        .take_while(|c| !stop(*c))
                        .map(char::len_utf8)
                        .sum();
                    result.push_str(&s[at..value_start]);
                    if value_len > 0 {
                        result.push_str("***");
                    }
                    at = value_start + value_len;
                    continue 'scan;
                }
            }
            let Some(c) = s[at..].chars().next() else {
                break;
            };
            result.push(c);
            at += c.len_utf8();
        }
        result
    };
    out = after(
        &out,
        &[
            "x-api-key:",
            "x-goog-api-key:",
            "authorization:",
            "api-key:",
            "proxy-authorization:",
        ],
        |c| c == '\n' || c == '\r',
    );
    out = after(&out, &["bearer "], char::is_whitespace);
    out = after(
        &out,
        &[
            "api_key=",
            "apikey=",
            "key=",
            "token=",
            "access_token=",
            "password=",
            "secret=",
        ],
        |c| c.is_whitespace() || matches!(c, '&' | '"' | '\'' | ')' | ',' | ';'),
    );
    // a token that starts like a service's
    let mut result = String::with_capacity(out.len());
    let mut token = String::new();
    let flush = |token: &mut String, result: &mut String| {
        if token.len() >= 16
            && token.len() <= MAX_TOKEN
            && KEY_PREFIXES.iter().any(|p| token.starts_with(p))
        {
            result.push_str("***");
        } else {
            result.push_str(token);
        }
        token.clear();
    };
    for c in out.chars() {
        if is_token_char(c) {
            token.push(c);
        } else {
            flush(&mut token, &mut result);
            result.push(c);
        }
    }
    flush(&mut token, &mut result);
    result
}

#[cfg(test)]
#[path = "agent_backend_test.rs"]
mod agent_backend_test;
