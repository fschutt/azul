//! AzReview's own switches (`--dwell-ms`, `--dwell-input`, `--silence-ms`, `--standby`, `--standby-idle-ms`) and the
//! dictation settings they override, as pure functions: the settings file's values and the switches in, the numbers the
//! window uses out, every value outside what a person can mean capped with a word about it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    dictation,
    dwell::{self, Allow},
    standby::{self, Standby},
};

/// The settings file's value: which pointers may start a dictation (`off`, `pen`, `any`).
pub const KEY_INPUT: &str = "dictation-input";
/// The settings file's value: how long the pen rests before it dictates, ms.
pub const KEY_REST: &str = "dictation-rest-ms";
/// The settings file's value: how long without words ends a dictation, ms.
pub const KEY_SILENCE: &str = "dictation-silence-ms";

/// The settings file's value: whether the microphone is kept ready for a dictation (`off`, `near`, `always`).
pub const KEY_STANDBY: &str = "dictation-standby";
/// The settings file's value: how long the pen is away before the microphone is closed, ms.
pub const KEY_STANDBY_IDLE: &str = "dictation-standby-idle-ms";

/// The settings file's value: a dictation that is over asks the agent by itself (`on` or `off`).
pub const KEY_ASK_AGENT: &str = "dictation-ask-agent";
/// How long the mock agent takes to answer when nothing says otherwise, ms.
pub const DEFAULT_AGENT_DELAY_MS: u64 = 800;

/// The settings file's value: the folder reviewed last.
pub const KEY_FOLDER: &str = "last-folder";

/// The lines AzReview adds to the kit's usage text.
pub const HELP: &str = "    --dwell-ms <MS>          How long the pen rests before it dictates (this run only)\n    \
                        --dwell-input <NAME>     off | pen | any: which pointers may start a dictation (this run only)\n    \
                        --silence-ms <MS>        How long without words ends a dictation (this run only)\n    \
                        --standby <NAME>         off | near | always: when the microphone is kept ready (this run only)\n    \
                        --standby-idle-ms <MS>   How long the pen is away before the microphone is closed (this run only)\n    \
                        --agent <NAME>           claude-print | anthropic-api | gemini-cli | gemini-api | mock: one backend for every task (this run only)\n    \
                        --agent-delay-ms <MS>    How long the mock agent takes to answer (this run only)\n    \
                        --agent-auto <on|off>    Whether a dictation that is over asks the agent by itself (this run only)\n";

/// The switches of AzReview itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Switches {
    /// `--dwell-ms`.
    pub dwell_ms: Option<u64>,
    /// `--dwell-input`.
    pub dwell_input: Option<Allow>,
    /// `--silence-ms`.
    pub silence_ms: Option<u64>,
    /// `--standby`.
    pub standby: Option<Standby>,
    /// `--standby-idle-ms`.
    pub standby_idle_ms: Option<u64>,
    /// `--agent`.
    pub agent: Option<crate::agent_backend::BackendKind>,
    /// `--agent-delay-ms`.
    pub agent_delay_ms: Option<u64>,
    /// `--agent-auto`.
    pub agent_auto: Option<bool>,
}

impl Switches {
    /// Takes AzReview's switches out of `argv` (without the program name); what is left is for the
    /// kit's parser, which refuses what it does not know.
    ///
    /// # Errors
    /// The switch that could not be read, by its name.
    pub fn extract(argv: Vec<String>) -> Result<(Self, Vec<String>), String> {
        let mut sw = Self::default();
        let mut rest = Vec::new();
        let mut args = argv.into_iter();
        while let Some(arg) = args.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                _ => (arg.clone(), None),
            };
            if !matches!(
                name.as_str(),
                "--dwell-ms"
                    | "--dwell-input"
                    | "--silence-ms"
                    | "--standby"
                    | "--standby-idle-ms"
                    | "--agent"
                    | "--agent-delay-ms"
                    | "--agent-auto"
            ) {
                rest.push(arg);
                continue;
            }
            let value = match inline {
                Some(v) => v,
                None => args.next().ok_or_else(|| format!("{name} needs a value"))?,
            };
            match name.as_str() {
                "--dwell-ms" => sw.dwell_ms = Some(millis(&name, &value)?),
                "--silence-ms" => sw.silence_ms = Some(millis(&name, &value)?),
                "--standby-idle-ms" => sw.standby_idle_ms = Some(millis(&name, &value)?),
                "--agent-delay-ms" => sw.agent_delay_ms = Some(millis(&name, &value)?),
                "--agent" => {
                    sw.agent = Some(crate::agent_backend::BackendKind::from_key(&value).ok_or_else(|| {
                        let keys: Vec<&str> = crate::agent_backend::BackendKind::ALL.iter().map(|k| k.key()).collect();
                        format!("{name}: expected {}, got {value:?}", keys.join("|"))
                    })?);
                }
                "--agent-auto" => {
                    sw.agent_auto = Some(
                        on_off(&value).ok_or_else(|| format!("{name}: expected on|off, got {value:?}"))?,
                    );
                }
                "--standby" => {
                    sw.standby = Some(
                        Standby::parse(&value)
                            .ok_or_else(|| format!("{name}: expected off|near|always, got {value:?}"))?,
                    );
                }
                _ => {
                    sw.dwell_input = Some(
                        Allow::parse(&value)
                            .ok_or_else(|| format!("{name}: expected off|pen|any, got {value:?}"))?,
                    );
                }
            }
        }
        Ok((sw, rest))
    }
}

/// `on` or `off`, whatever the case.
fn on_off(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

fn millis(name: &str, value: &str) -> Result<u64, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("{name}: expected a number of milliseconds, got {value:?}"))
}

/// What the window runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub allow: Allow,
    pub rest_ms: u64,
    pub silence_ms: u64,
    /// When the microphone is kept ready for a dictation.
    pub standby: Standby,
    /// How long the pen is away before it is closed, ms.
    pub standby_idle_ms: u64,
    /// One backend for every task (`--agent`), or none: the settings file's.
    pub agent: Option<crate::agent_backend::BackendKind>,
    /// How long the mock agent takes to answer, ms.
    pub agent_delay_ms: u64,
    /// A dictation that is over asks the agent by itself.
    pub agent_auto: bool,
    /// What was changed on the way (a value capped, a word nobody knows), for the log.
    pub notes: Vec<String>,
}

/// A number under `key` of the settings values: `None` for no value, the default (with a note) for
/// one that is no number.
fn number(values: &BTreeMap<String, String>, key: &str, default: u64, notes: &mut Vec<String>) -> Option<u64> {
    let raw = values.get(key)?;
    if let Ok(n) = raw.trim().parse::<u64>() {
        Some(n)
    } else {
        notes.push(format!("the setting {key} is {raw:?}, no number of milliseconds: using {default}"));
        Some(default)
    }
}

/// The dictation's numbers: a switch over the settings file's value over the default.
#[must_use]
pub fn resolve(values: &BTreeMap<String, String>, sw: &Switches) -> Resolved {
    let mut notes = Vec::new();
    let allow = match (sw.dwell_input, values.get(KEY_INPUT)) {
        (Some(a), _) => a,
        (None, Some(raw)) => Allow::parse(raw).unwrap_or_else(|| {
            notes.push(format!("the setting {KEY_INPUT} is {raw:?}, not off, pen or any: using pen"));
            Allow::Pen
        }),
        (None, None) => Allow::Pen,
    };
    let rest = sw
        .dwell_ms
        .or_else(|| number(values, KEY_REST, dwell::DEFAULT_REST_MS, &mut notes))
        .unwrap_or(dwell::DEFAULT_REST_MS);
    let (rest_ms, why) = dwell::cap_rest_ms(rest);
    notes.extend(why);
    let silence = sw
        .silence_ms
        .or_else(|| number(values, KEY_SILENCE, dictation::DEFAULT_SILENCE_MS, &mut notes))
        .unwrap_or(dictation::DEFAULT_SILENCE_MS);
    let (silence_ms, why) = dictation::cap_silence_ms(silence);
    notes.extend(why);
    let keep_ready = match (sw.standby, values.get(KEY_STANDBY)) {
        (Some(s), _) => s,
        (None, Some(raw)) => Standby::parse(raw).unwrap_or_else(|| {
            notes.push(format!(
                "the setting {KEY_STANDBY} is {raw:?}, not off, near or always: using {}",
                Standby::default().name()
            ));
            Standby::default()
        }),
        (None, None) => Standby::default(),
    };
    let idle = sw
        .standby_idle_ms
        .or_else(|| number(values, KEY_STANDBY_IDLE, standby::DEFAULT_IDLE_MS, &mut notes))
        .unwrap_or(standby::DEFAULT_IDLE_MS);
    let (standby_idle_ms, why) = standby::cap_idle_ms(idle);
    notes.extend(why);
    let agent_auto = match (sw.agent_auto, values.get(KEY_ASK_AGENT)) {
        (Some(on), _) => on,
        (None, Some(raw)) => on_off(raw).unwrap_or_else(|| {
            notes.push(format!("the setting {KEY_ASK_AGENT} is {raw:?}, not on or off: using on"));
            true
        }),
        (None, None) => true,
    };
    Resolved {
        allow,
        rest_ms,
        silence_ms,
        standby: keep_ready,
        standby_idle_ms,
        agent: sw.agent,
        agent_delay_ms: sw.agent_delay_ms.unwrap_or(DEFAULT_AGENT_DELAY_MS),
        agent_auto,
        notes,
    }
}

/// The folder to review: the one named on the command line; else the one reviewed last, when it is
/// still a folder and this run may take it (a fixture run never takes what the user did); else none (the
/// empty path: the window offers to open one). Never the current folder: an app started from a
/// terminal, the Finder or a launcher reviews what the user chose, not wherever it happened to start.
pub fn pick_root(
    named: Option<PathBuf>,
    last: Option<&str>,
    may_take_last: bool,
    is_dir: impl Fn(&Path) -> bool,
) -> PathBuf {
    named
        .or_else(|| {
            last.map(str::trim)
                .filter(|l| !l.is_empty() && may_take_last)
                .map(PathBuf::from)
                .filter(|p| is_dir(p))
        })
        .unwrap_or_default()
}
