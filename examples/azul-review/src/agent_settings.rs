//! Which model does what: the agents' settings, as the gear's agent panel reads and writes them.
//!
//! Per task ([`Task`]: exploration, editing, export) the settings say which backend answers
//! ([`BackendKind`]), with which model (a name, as data: [`PRESETS`] are suggestions, empty means
//! the backend's own default), how long a call may take, how much it may write or cost, where the
//! KEY is - never the key itself: a reference, `env:NAME` for an environment variable or
//! `keychain:ITEM` for the system's keychain, and anything that looks like a key is refused - which
//! program to run, which address to call, and extra arguments (the ones that would widen what the
//! agent may do are refused). Two settings are global: how many agents work at once and how large
//! the limits are ([`crate::agent_limits`]).
//!
//! The file is the app's `settings.json` `values`: flat strings, `agent-<task>-<field>`, read once
//! and handed on ([`AgentSettings::from_values`]: forgiving, every complaint a [`Problem`]; a value
//! outside what a person can mean is capped with a note, never a crash). The panel needs no logic of
//! its own: [`FIELDS`] says which rows a backend has, [`AgentSettings::get`] and
//! [`AgentSettings::set`] read and write a row by its name, and the problems come back as words
//! with localization keys.
//!
//! The environment is read once, by the app at its start, into an [`Env`] that travels with the
//! settings; a key reference is resolved from it ([`Keys::resolve`]) or from the keychain
//! ([`Keychain`], which the app implements with the engine's keyring).

use std::{collections::BTreeMap, fmt, sync::Arc};

use crate::{
    agent_backend::{AgentError, BackendKind, Identity, Request, Response, Task},
    agent_limits::{self, Limits},
    agent_review::{Context, ReviewRequest, Utterance},
};

/// The prefix of every agent setting in the settings file.
pub const KEY_PREFIX: &str = "agent-";
/// The global settings.
pub const KEY_MAX_CONCURRENT: &str = "agent-max-concurrent";
pub const KEY_LIMIT_SCALE: &str = "agent-limit-scale";

/// The most characters of a text field.
const MAX_TEXT: usize = 512;
const MAX_MODEL: usize = 120;
const MAX_ARGS: usize = 1_000;

// ==== Models ====

/// The model names a backend is suggested: aliases for the Claude CLI (they follow the newest model
/// of the family), the names the Gemini CLI itself knows.
pub static PRESETS: [(BackendKind, &[&str]); 5] = [
    (
        BackendKind::ClaudePrint,
        &["haiku", "sonnet", "opus", "fable"],
    ),
    (BackendKind::AnthropicApi, &[]),
    (
        BackendKind::GeminiCli,
        &[
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-3-flash-preview",
            "gemini-3-pro-preview",
        ],
    ),
    (
        BackendKind::GeminiApi,
        &[
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-3-flash-preview",
        ],
    ),
    (BackendKind::Mock, &["mock"]),
];

/// The names suggested for `backend`.
#[must_use]
pub fn presets(backend: BackendKind) -> &'static [&'static str] {
    for (kind, names) in &PRESETS {
        if *kind == backend {
            return names;
        }
    }
    &[]
}

/// The model a task starts with on `backend`.
#[must_use]
pub fn default_model(task: Task, backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::ClaudePrint => task.spec().claude_model,
        BackendKind::GeminiApi => "gemini-2.5-flash",
        BackendKind::Mock => "mock",
        BackendKind::GeminiCli | BackendKind::AnthropicApi => "",
    }
}

// ==== Keys ====

/// Where a key is kept: never in the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyRef {
    /// An environment variable.
    Env(String),
    /// An item of the system's keychain.
    Keychain(String),
}

impl KeyRef {
    /// The reference as it is written in the settings.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Env(name) => format!("env:{name}"),
            Self::Keychain(item) => format!("keychain:{item}"),
        }
    }

    /// Reads a reference: `env:NAME`, `keychain:ITEM`, or a bare NAME in capitals (an environment
    /// variable). Nothing is no reference. What looks like a key itself is refused, and never
    /// repeated in the answer.
    ///
    /// # Errors
    /// [`ProblemKind::KeyLooksLikeSecret`] or [`ProblemKind::KeyRefInvalid`].
    pub fn parse(text: &str) -> Result<Option<Self>, ProblemKind> {
        let t = text.trim();
        if t.is_empty() {
            return Ok(None);
        }
        let env_name = |n: &str| {
            let mut chars = n.chars();
            n.len() <= 128
                && chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        let item_name = |n: &str| {
            !n.is_empty()
                && n.len() <= 128
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        };
        if looks_like_secret(t) {
            return Err(ProblemKind::KeyLooksLikeSecret);
        }
        if let Some(name) = t.strip_prefix("env:") {
            return if looks_like_secret(name) {
                Err(ProblemKind::KeyLooksLikeSecret)
            } else if env_name(name) {
                Ok(Some(Self::Env(name.to_string())))
            } else {
                Err(ProblemKind::KeyRefInvalid)
            };
        }
        if let Some(item) = t.strip_prefix("keychain:") {
            return if item_name(item) {
                Ok(Some(Self::Keychain(item.to_string())))
            } else {
                Err(ProblemKind::KeyRefInvalid)
            };
        }
        // a bare name of capitals is a variable
        if env_name(t)
            && t.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            return Ok(Some(Self::Env(t.to_string())));
        }
        Err(ProblemKind::KeyRefInvalid)
    }
}

/// Whether `text` looks like a key itself and not the name of one: a service's token prefix, or a
/// long run of letters and digits that is no variable name.
#[must_use]
pub fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    if t.len() >= 20
        && ["sk-", "AIza", "ya29.", "ghp_", "github_pat_", "xox"]
            .iter()
            .any(|p| t.starts_with(p))
    {
        return true;
    }
    let shaped_like_a_name = t
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    t.len() >= 32
        && !t.contains(char::is_whitespace)
        && !t.contains(':')
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !shaped_like_a_name
}

/// A key that is not echoed: its debug text hides it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The key, for the one place that sends it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// The process's environment, taken once at the start of the app and handed on (nothing below
/// reads the live environment).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    vars: BTreeMap<String, String>,
}

impl Env {
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            vars: pairs.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }

    /// The variables, for a program that is started with them.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// The system's keychain, as far as the agents read it (the app implements it with the engine's
/// keyring, a test with a map).
pub trait Keychain: Send + Sync {
    /// The secret of `item`: `Ok(None)` when there is none.
    ///
    /// # Errors
    /// Why the keychain could not be read (refused, not available).
    fn read(&self, item: &str) -> Result<Option<String>, String>;
}

/// Where keys are found.
#[derive(Clone, Default)]
pub struct Keys {
    pub env: Env,
    pub keychain: Option<Arc<dyn Keychain>>,
}

impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keys")
            .field("env_vars", &self.env.vars.len())
            .field("keychain", &self.keychain.is_some())
            .finish()
    }
}

impl Keys {
    /// The key a reference names.
    ///
    /// # Errors
    /// [`AgentError::KeyMissing`] when there is none, [`AgentError::KeyDenied`] when the keychain
    /// refuses, [`AgentError::Unsupported`] when there is no keychain here.
    pub fn resolve(&self, reference: &KeyRef) -> Result<Secret, AgentError> {
        let missing = || AgentError::KeyMissing {
            reference: reference.text(),
        };
        match reference {
            KeyRef::Env(name) => {
                match self.env.get(name).map(str::trim).filter(|v| !v.is_empty()) {
                    Some(value) => Ok(Secret::new(value)),
                    None => Err(missing()),
                }
            }
            KeyRef::Keychain(item) => {
                let Some(keychain) = &self.keychain else {
                    return Err(AgentError::Unsupported {
                        why: "there is no keychain to read the key from".to_string(),
                    });
                };
                match keychain.read(item) {
                    Ok(Some(value)) if !value.trim().is_empty() => Ok(Secret::new(value.trim())),
                    Ok(_) => Err(missing()),
                    Err(why) => Err(AgentError::KeyDenied {
                        reference: reference.text(),
                        why: crate::agent_backend::scrub(&why, &[]),
                    }),
                }
            }
        }
    }
}

// ==== Problems ====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The setting is not usable.
    Error,
    /// The setting was changed to be usable.
    Note,
}

/// What is wrong with a setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemKind {
    UnknownBackend {
        word: String,
    },
    NotANumber {
        word: String,
    },
    OutOfRange {
        value: u64,
        min: u32,
        max: u32,
        used: u32,
    },
    /// Not `env:NAME` or `keychain:ITEM` (the text is not repeated: it may be a key).
    KeyRefInvalid,
    /// The text looks like a key itself (not repeated).
    KeyLooksLikeSecret,
    ArgForbidden {
        arg: String,
    },
    ModelRequired,
    UrlInvalid {
        text: String,
    },
    UnknownField {
        word: String,
    },
}

/// A complaint about one setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub task: Option<Task>,
    /// The field's word (`model`, `timeout`; the global settings' own).
    pub field: &'static str,
    pub severity: Severity,
    pub kind: ProblemKind,
}

impl Problem {
    fn new(task: Option<Task>, field: &'static str, severity: Severity, kind: ProblemKind) -> Self {
        Self {
            task,
            field,
            severity,
            kind,
        }
    }

    /// `editing.model`, or the global setting's word.
    #[must_use]
    pub fn path(&self) -> String {
        self.task.map_or_else(
            || self.field.to_string(),
            |t| format!("{}.{}", t.key(), self.field),
        )
    }

    #[must_use]
    pub fn l10n_key(&self) -> &'static str {
        match self.kind {
            ProblemKind::UnknownBackend { .. } => "azreview-agent-problem-unknown-backend",
            ProblemKind::NotANumber { .. } => "azreview-agent-problem-not-a-number",
            ProblemKind::OutOfRange { .. } => "azreview-agent-problem-out-of-range",
            ProblemKind::KeyRefInvalid => "azreview-agent-problem-key-invalid",
            ProblemKind::KeyLooksLikeSecret => "azreview-agent-problem-key-secret",
            ProblemKind::ArgForbidden { .. } => "azreview-agent-problem-arg-forbidden",
            ProblemKind::ModelRequired => "azreview-agent-problem-model-required",
            ProblemKind::UrlInvalid { .. } => "azreview-agent-problem-url-invalid",
            ProblemKind::UnknownField { .. } => "azreview-agent-problem-unknown-field",
        }
    }

    #[must_use]
    pub fn l10n_args(&self) -> Vec<(&'static str, String)> {
        let mut args = vec![("field", self.path())];
        match &self.kind {
            ProblemKind::UnknownBackend { word }
            | ProblemKind::NotANumber { word }
            | ProblemKind::UnknownField { word } => {
                args.push(("word", word.clone()));
            }
            ProblemKind::OutOfRange {
                value,
                min,
                max,
                used,
            } => {
                args.push(("value", value.to_string()));
                args.push(("min", min.to_string()));
                args.push(("max", max.to_string()));
                args.push(("used", used.to_string()));
            }
            ProblemKind::ArgForbidden { arg } => args.push(("arg", arg.clone())),
            ProblemKind::UrlInvalid { text } => args.push(("text", text.clone())),
            ProblemKind::KeyRefInvalid
            | ProblemKind::KeyLooksLikeSecret
            | ProblemKind::ModelRequired => {}
        }
        args
    }

    /// The sentence in English: what `resources/en.ftl` says with the same arguments.
    #[must_use]
    pub fn message(&self) -> String {
        let field = self.path();
        match &self.kind {
            ProblemKind::UnknownBackend { word } => format!(
                "{field}: \"{word}\" is not a backend: use claude-print, anthropic-api, gemini-cli, gemini-api or mock"
            ),
            ProblemKind::NotANumber { word } => format!("{field}: \"{word}\" is no number"),
            ProblemKind::OutOfRange { value, min, max, used } => {
                format!("{field}: {value} is outside {min} to {max}, using {used}")
            }
            ProblemKind::KeyRefInvalid => {
                format!("{field}: write the key as env:NAME (an environment variable) or keychain:ITEM")
            }
            ProblemKind::KeyLooksLikeSecret => format!(
                "{field}: this looks like a key itself: never put a key in the settings; keep it in an environment variable or the keychain and write its name here"
            ),
            ProblemKind::ArgForbidden { arg } => {
                format!("{field}: {arg} is not allowed here (it would widen what the agent may do)")
            }
            ProblemKind::ModelRequired => format!("{field}: name a model"),
            ProblemKind::UrlInvalid { text } => format!("{field}: {text} is no http:// or https:// address"),
            ProblemKind::UnknownField { word } => format!("{field}: no such setting ({word})"),
        }
    }
}

// ==== The settings of a task ====

/// How one task is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSettings {
    pub backend: BackendKind,
    /// Empty: the backend's own default.
    pub model: String,
    pub timeout_secs: u32,
    pub max_output_tokens: u32,
    /// What a `claude -p` call may cost, in US cents (0: no limit).
    pub budget_cents: u32,
    /// The most turns of a `claude -p` call (0: no limit).
    pub max_turns: u32,
    /// The reference to the key as written (`env:NAME`, `keychain:ITEM`; empty: none).
    pub key: String,
    /// The program to run (empty: look for it).
    pub program: String,
    /// The address of the service (empty: the usual one).
    pub base_url: String,
    /// Extra arguments for a program, as one text.
    pub extra_args: String,
}

impl TaskSettings {
    /// What `task` starts with on `backend`.
    #[must_use]
    pub fn defaults(task: Task, backend: BackendKind) -> Self {
        let spec = task.spec();
        Self {
            backend,
            model: default_model(task, backend).to_string(),
            timeout_secs: spec.timeout_secs,
            max_output_tokens: spec.max_output_tokens,
            budget_cents: spec.budget_cents,
            max_turns: spec.max_turns,
            key: String::new(),
            program: String::new(),
            base_url: String::new(),
            extra_args: String::new(),
        }
    }

    /// Who this is: the backend and its model.
    #[must_use]
    pub fn identity(&self) -> Identity {
        Identity {
            kind: self.backend,
            model: self.model.trim().to_string(),
        }
    }

    /// The key reference, read.
    ///
    /// # Errors
    /// What is wrong with the text.
    pub fn key_ref(&self) -> Result<Option<KeyRef>, ProblemKind> {
        KeyRef::parse(&self.key)
    }

    /// The extra arguments as separate words.
    #[must_use]
    pub fn args(&self) -> Vec<String> {
        split_args(&self.extra_args)
    }
}

// ==== The fields (the gear panel's rows) ====

/// What a row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Backend,
    Model,
    /// A whole number between the two.
    Number {
        min: u32,
        max: u32,
    },
    KeyRef,
    Program,
    Url,
    Args,
}

/// One row of the panel.
#[derive(Debug)]
pub struct FieldSpec {
    /// The row's word in the settings file and in [`AgentSettings::get`] and [`AgentSettings::set`].
    pub key: &'static str,
    /// The localization key of its label.
    pub label: &'static str,
    /// The localization key of its hint.
    pub hint: &'static str,
    pub kind: FieldKind,
    /// The backends it matters to.
    pub applies: &'static [BackendKind],
}

const CLAUDE: BackendKind = BackendKind::ClaudePrint;
const GEMINI: BackendKind = BackendKind::GeminiCli;
const ANTHROPIC: BackendKind = BackendKind::AnthropicApi;
const GEMINI_API: BackendKind = BackendKind::GeminiApi;
const MOCK: BackendKind = BackendKind::Mock;

/// The rows, in the order the panel shows them.
pub static FIELDS: [FieldSpec; 10] = [
    FieldSpec {
        key: "backend",
        label: "azreview-agent-field-backend",
        hint: "azreview-agent-hint-backend",
        kind: FieldKind::Backend,
        applies: &[CLAUDE, ANTHROPIC, GEMINI, GEMINI_API, MOCK],
    },
    FieldSpec {
        key: "model",
        label: "azreview-agent-field-model",
        hint: "azreview-agent-hint-model",
        kind: FieldKind::Model,
        applies: &[CLAUDE, ANTHROPIC, GEMINI, GEMINI_API],
    },
    FieldSpec {
        key: "timeout",
        label: "azreview-agent-field-timeout",
        hint: "azreview-agent-hint-timeout",
        kind: FieldKind::Number { min: 5, max: 1_800 },
        applies: &[CLAUDE, ANTHROPIC, GEMINI, GEMINI_API],
    },
    FieldSpec {
        key: "max-tokens",
        label: "azreview-agent-field-max-tokens",
        hint: "azreview-agent-hint-max-tokens",
        kind: FieldKind::Number {
            min: 128,
            max: 64_000,
        },
        applies: &[ANTHROPIC, GEMINI_API],
    },
    FieldSpec {
        key: "budget",
        label: "azreview-agent-field-budget",
        hint: "azreview-agent-hint-budget",
        kind: FieldKind::Number { min: 0, max: 5_000 },
        applies: &[CLAUDE],
    },
    FieldSpec {
        key: "max-turns",
        label: "azreview-agent-field-max-turns",
        hint: "azreview-agent-hint-max-turns",
        kind: FieldKind::Number { min: 0, max: 100 },
        applies: &[CLAUDE],
    },
    FieldSpec {
        key: "key",
        label: "azreview-agent-field-key",
        hint: "azreview-agent-hint-key",
        kind: FieldKind::KeyRef,
        applies: &[CLAUDE, ANTHROPIC, GEMINI, GEMINI_API],
    },
    FieldSpec {
        key: "program",
        label: "azreview-agent-field-program",
        hint: "azreview-agent-hint-program",
        kind: FieldKind::Program,
        applies: &[CLAUDE, GEMINI],
    },
    FieldSpec {
        key: "base-url",
        label: "azreview-agent-field-base-url",
        hint: "azreview-agent-hint-base-url",
        kind: FieldKind::Url,
        applies: &[ANTHROPIC, GEMINI_API],
    },
    FieldSpec {
        key: "args",
        label: "azreview-agent-field-args",
        hint: "azreview-agent-hint-args",
        kind: FieldKind::Args,
        applies: &[CLAUDE, GEMINI],
    },
];

/// A setting that is not a task's: how many agents work at once, how large the limits are.
#[derive(Debug)]
pub struct GlobalSpec {
    /// The word in the settings file.
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub min: u32,
    pub max: u32,
}

/// The global rows, in the order the panel shows them.
pub static GLOBALS: [GlobalSpec; 2] = [
    GlobalSpec {
        key: KEY_MAX_CONCURRENT,
        label: "azreview-agent-field-max-concurrent",
        hint: "azreview-agent-hint-max-concurrent",
        min: 1,
        max: 8,
    },
    GlobalSpec {
        key: KEY_LIMIT_SCALE,
        label: "azreview-agent-field-limit-scale",
        hint: "azreview-agent-hint-limit-scale",
        min: 1,
        max: 8,
    },
];

/// The rows that matter to `backend`.
#[must_use]
pub fn fields_for(backend: BackendKind) -> Vec<&'static FieldSpec> {
    FIELDS
        .iter()
        .filter(|f| f.applies.contains(&backend))
        .collect()
}

fn field(key: &str) -> Option<&'static FieldSpec> {
    FIELDS.iter().find(|f| f.key == key)
}

// ==== Extra arguments ====

/// The arguments that would widen what an agent may do (tools, permissions): refused, by name, in
/// both the `--name value` and the `--name=value` form.
static FORBIDDEN_ARGS: [&str; 13] = [
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--permission-mode",
    "--permission-prompts",
    "--tools",
    "--allowedtools",
    "--allowed-tools",
    "--disallowedtools",
    "--disallowed-tools",
    "--yolo",
    "-y",
    "--approval-mode",
    "--settings",
];

/// `text` as separate arguments: spaces separate, a pair of quotes holds spaces together.
#[must_use]
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    args
}

/// The first of the arguments of `text` that is forbidden.
#[must_use]
pub fn forbidden_arg(text: &str) -> Option<String> {
    split_args(text).into_iter().find(|arg| {
        let name = arg.split('=').next().unwrap_or("").to_ascii_lowercase();
        FORBIDDEN_ARGS.contains(&name.as_str())
    })
}

// ==== All the settings ====

/// The agents' settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSettings {
    pub tasks: BTreeMap<Task, TaskSettings>,
    /// How many agents work at once (1 to 8).
    pub max_concurrent: usize,
    /// How much larger than the defaults the limits are (1 to 8).
    pub limit_scale: u32,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            tasks: Task::all()
                .map(|t| (t, TaskSettings::defaults(t, BackendKind::ClaudePrint)))
                .collect(),
            max_concurrent: crate::agent_pool::DEFAULT_CAP,
            limit_scale: 1,
        }
    }
}

/// `value` held to `min..=max`: the number to use and the problem when it was held.
fn held(
    value: u64,
    min: u32,
    max: u32,
    task: Option<Task>,
    field: &'static str,
) -> (u32, Option<Problem>) {
    let used = u32::try_from(value.clamp(u64::from(min), u64::from(max))).unwrap_or(max);
    let note = (u64::from(used) != value).then(|| {
        Problem::new(
            task,
            field,
            Severity::Note,
            ProblemKind::OutOfRange {
                value,
                min,
                max,
                used,
            },
        )
    });
    (used, note)
}

impl AgentSettings {
    /// The settings of `task`.
    #[must_use]
    pub fn task(&self, task: Task) -> TaskSettings {
        self.tasks
            .get(&task)
            .cloned()
            .unwrap_or_else(|| TaskSettings::defaults(task, BackendKind::ClaudePrint))
    }

    /// The limits these settings ask for.
    #[must_use]
    pub fn limits(&self) -> Limits {
        Limits::default().scaled(self.limit_scale)
    }

    /// The value of a row, as the panel shows it.
    #[must_use]
    pub fn get(&self, task: Task, field_key: &str) -> Option<String> {
        let t = self.tasks.get(&task)?;
        Some(match field_key {
            "backend" => t.backend.key().to_string(),
            "model" => t.model.clone(),
            "timeout" => t.timeout_secs.to_string(),
            "max-tokens" => t.max_output_tokens.to_string(),
            "budget" => t.budget_cents.to_string(),
            "max-turns" => t.max_turns.to_string(),
            "key" => t.key.clone(),
            "program" => t.program.clone(),
            "base-url" => t.base_url.clone(),
            "args" => t.extra_args.clone(),
            _ => return None,
        })
    }

    /// Writes a row. A number outside what a person can mean is held to the range, and the note
    /// comes back as `Ok(Some(..))`; what cannot be used is an error and nothing is written.
    ///
    /// # Errors
    /// The [`Problem`] with the value.
    pub fn set(
        &mut self,
        task: Task,
        field_key: &str,
        value: &str,
    ) -> Result<Option<Problem>, Problem> {
        let Some(spec) = field(field_key) else {
            return Err(Problem::new(
                Some(task),
                "field",
                Severity::Error,
                ProblemKind::UnknownField {
                    word: field_key.to_string(),
                },
            ));
        };
        let name = spec.key;
        let err = |kind: ProblemKind| Problem::new(Some(task), name, Severity::Error, kind);
        let entry = self
            .tasks
            .entry(task)
            .or_insert_with(|| TaskSettings::defaults(task, BackendKind::ClaudePrint));
        let value = value.trim();
        match spec.kind {
            FieldKind::Backend => {
                let kind = BackendKind::from_key(value).ok_or_else(|| {
                    err(ProblemKind::UnknownBackend {
                        word: value.to_string(),
                    })
                })?;
                // the model of the old backend's default does not mean anything on the new one
                if entry.model.trim().is_empty()
                    || entry.model == default_model(task, entry.backend)
                {
                    entry.model = default_model(task, kind).to_string();
                }
                if kind != BackendKind::ClaudePrint
                    && entry.budget_cents == task.spec().budget_cents
                {
                    entry.budget_cents = 0;
                } else if kind == BackendKind::ClaudePrint && entry.budget_cents == 0 {
                    entry.budget_cents = task.spec().budget_cents;
                }
                entry.backend = kind;
                Ok(None)
            }
            FieldKind::Model => {
                let (model, _) =
                    agent_limits::cap_chars(&value.replace(['\r', '\n', '\t'], " "), MAX_MODEL);
                entry.model = model.trim().to_string();
                Ok(None)
            }
            FieldKind::Number { min, max } => {
                let number: u64 = value.parse().map_err(|_| {
                    err(ProblemKind::NotANumber {
                        word: value.to_string(),
                    })
                })?;
                let (used, note) = held(number, min, max, Some(task), name);
                match name {
                    "timeout" => entry.timeout_secs = used,
                    "max-tokens" => entry.max_output_tokens = used,
                    "budget" => entry.budget_cents = used,
                    _ => entry.max_turns = used,
                }
                Ok(note)
            }
            FieldKind::KeyRef => {
                let key = KeyRef::parse(value).map_err(err)?;
                entry.key = key.map(|k| k.text()).unwrap_or_default();
                Ok(None)
            }
            FieldKind::Program => {
                entry.program =
                    agent_limits::cap_chars(&value.replace(['\r', '\n'], ""), MAX_TEXT).0;
                Ok(None)
            }
            FieldKind::Url => {
                if !value.is_empty() {
                    let rest = value
                        .strip_prefix("https://")
                        .or_else(|| value.strip_prefix("http://"));
                    let authority = rest.map(|r| r.split('/').next().unwrap_or(""));
                    let valid = authority.is_some_and(|a| !a.is_empty() && !a.contains('@'))
                        && !value.contains(char::is_whitespace)
                        && !value.contains('?')
                        && value.len() <= MAX_TEXT;
                    if !valid {
                        return Err(err(ProblemKind::UrlInvalid {
                            text: agent_limits::cap_chars(value, 80).0,
                        }));
                    }
                }
                entry.base_url = value.trim_end_matches('/').to_string();
                Ok(None)
            }
            FieldKind::Args => {
                if let Some(arg) = forbidden_arg(value) {
                    return Err(err(ProblemKind::ArgForbidden { arg }));
                }
                entry.extra_args =
                    agent_limits::cap_chars(&value.replace(['\r', '\n'], " "), MAX_ARGS).0;
                Ok(None)
            }
        }
    }

    /// Writes the global how-many-at-once setting (held to 1 to 8).
    pub fn set_max_concurrent(&mut self, value: u64) -> Option<Problem> {
        let (used, note) = held(value, 1, 8, None, "max-concurrent");
        self.max_concurrent = used as usize;
        note
    }

    /// Writes the global size of the limits (held to 1 to 8).
    pub fn set_limit_scale(&mut self, value: u64) -> Option<Problem> {
        let (used, note) = held(value, 1, 8, None, "limit-scale");
        self.limit_scale = used;
        note
    }

    /// The settings as the settings file keeps them: every row, `agent-<task>-<field>`.
    #[must_use]
    pub fn to_values(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        for task in Task::all() {
            for spec in &FIELDS {
                if let Some(v) = self.get(task, spec.key) {
                    values.insert(format!("{KEY_PREFIX}{}-{}", task.key(), spec.key), v);
                }
            }
        }
        values.insert(
            KEY_MAX_CONCURRENT.to_string(),
            self.max_concurrent.to_string(),
        );
        values.insert(KEY_LIMIT_SCALE.to_string(), self.limit_scale.to_string());
        values
    }

    /// Reads the settings from the settings file's values: what is missing is the default, what is
    /// outside what a person can mean is held (a note), what cannot be read is a problem and keeps
    /// the default. A key written into a setting is dropped, and said.
    #[must_use]
    pub fn from_values(values: &BTreeMap<String, String>) -> (Self, Vec<Problem>) {
        let mut settings = Self::default();
        let mut problems = Vec::new();
        for (key, value) in values {
            let Some(rest) = key.strip_prefix(KEY_PREFIX) else {
                continue;
            };
            if key == KEY_MAX_CONCURRENT || key == KEY_LIMIT_SCALE {
                match value.trim().parse::<u64>() {
                    Ok(n) => {
                        let note = if key == KEY_MAX_CONCURRENT {
                            settings.set_max_concurrent(n)
                        } else {
                            settings.set_limit_scale(n)
                        };
                        problems.extend(note);
                    }
                    Err(_) => problems.push(Problem::new(
                        None,
                        if key == KEY_MAX_CONCURRENT {
                            "max-concurrent"
                        } else {
                            "limit-scale"
                        },
                        Severity::Error,
                        ProblemKind::NotANumber {
                            word: value.clone(),
                        },
                    )),
                }
                continue;
            }
            let known = Task::all().any(|t| {
                FIELDS
                    .iter()
                    .any(|f| rest == format!("{}-{}", t.key(), f.key))
            });
            if !known {
                problems.push(Problem::new(
                    None,
                    "field",
                    Severity::Note,
                    ProblemKind::UnknownField { word: key.clone() },
                ));
            }
        }
        // the backend first: it changes what the other rows start with
        for pass in [true, false] {
            for task in Task::all() {
                for spec in &FIELDS {
                    if (spec.key == "backend") != pass {
                        continue;
                    }
                    let Some(value) =
                        values.get(&format!("{KEY_PREFIX}{}-{}", task.key(), spec.key))
                    else {
                        continue;
                    };
                    match settings.set(task, spec.key, value) {
                        Ok(note) => problems.extend(note),
                        Err(problem) => problems.push(problem),
                    }
                }
            }
        }
        (settings, problems)
    }

    /// What is wrong with the settings as they stand (a model missing where one is needed, a key
    /// reference that cannot be read): the panel shows it beside the row.
    #[must_use]
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for task in Task::all() {
            let t = self.task(task);
            if t.backend.is_http() && t.model.trim().is_empty() {
                problems.push(Problem::new(
                    Some(task),
                    "model",
                    Severity::Error,
                    ProblemKind::ModelRequired,
                ));
            }
            if let Err(kind) = t.key_ref() {
                problems.push(Problem::new(Some(task), "key", Severity::Error, kind));
            }
            if let Some(arg) = forbidden_arg(&t.extra_args) {
                problems.push(Problem::new(
                    Some(task),
                    "args",
                    Severity::Error,
                    ProblemKind::ArgForbidden { arg },
                ));
            }
        }
        problems
    }
}

// ==== Test this backend ====

/// A tiny request for the "test this backend" button: no strokes, one line of code, one sentence.
#[must_use]
pub fn test_request(language: &str) -> Request {
    Request::Review(ReviewRequest {
        task: Task::Editing,
        conversation: 0,
        language: language.to_string(),
        utterances: vec![Utterance {
            text: "This is a connection test. Reply with the answer ok and no suggestions."
                .to_string(),
            start_ms: 0,
            end_ms: 1_000,
        }],
        strokes: Vec::new(),
        context: Context {
            file: "test.txt".to_string(),
            page: 0,
            first_line: 1,
            lines: vec!["ok".to_string()],
            total_lines: 1,
            focus: None,
            words_under: Vec::new(),
            grid: None,
        },
        turns: Vec::new(),
        workspace: None,
    })
}

/// What the button shows.
#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    pub ok: bool,
    pub identity: Identity,
    pub latency_ms: u64,
    /// The first words of the answer (empty on a failure).
    pub first_words: String,
    pub error: Option<AgentError>,
}

/// The report of a test call.
#[must_use]
pub fn test_report(result: &Result<Response, AgentError>, asked: &Identity) -> TestReport {
    match result {
        Ok(response) => TestReport {
            ok: true,
            identity: response.identity.clone(),
            latency_ms: response.usage.elapsed_ms,
            first_words: response.first_words(60),
            error: None,
        },
        Err(error) => TestReport {
            ok: false,
            identity: asked.clone(),
            latency_ms: 0,
            first_words: String::new(),
            error: Some(error.clone()),
        },
    }
}

#[cfg(test)]
#[path = "agent_settings_test.rs"]
mod agent_settings_test;
