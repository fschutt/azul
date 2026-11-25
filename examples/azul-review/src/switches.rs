//! AzReview's own switches (`--dwell-ms`, `--dwell-input`, `--silence-ms`) and the dictation settings
//! they override, as pure functions: the settings file's values and the switches in, the numbers the
//! window uses out, every value outside what a person can mean capped with a word about it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    dictation,
    dwell::{self, Allow},
};

/// The settings file's value: which pointers may start a dictation (`off`, `pen`, `any`).
pub const KEY_INPUT: &str = "dictation-input";
/// The settings file's value: how long the pen rests before it dictates, ms.
pub const KEY_REST: &str = "dictation-rest-ms";
/// The settings file's value: how long without words ends a dictation, ms.
pub const KEY_SILENCE: &str = "dictation-silence-ms";

/// The settings file's value: the folder reviewed last.
pub const KEY_FOLDER: &str = "last-folder";

/// The lines AzReview adds to the kit's usage text.
pub const HELP: &str = "    --dwell-ms <MS>          How long the pen rests before it dictates (this run only)\n    \
                        --dwell-input <NAME>     off | pen | any: which pointers may start a dictation (this run only)\n    \
                        --silence-ms <MS>        How long without words ends a dictation (this run only)\n";

/// The switches of AzReview itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Switches {
    /// `--dwell-ms`.
    pub dwell_ms: Option<u64>,
    /// `--dwell-input`.
    pub dwell_input: Option<Allow>,
    /// `--silence-ms`.
    pub silence_ms: Option<u64>,
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
            if !matches!(name.as_str(), "--dwell-ms" | "--dwell-input" | "--silence-ms") {
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
    Resolved {
        allow,
        rest_ms,
        silence_ms,
        notes,
    }
}

/// The current folder as a folder to review: a filesystem root is none (an app started from the Finder has
/// `/` as its folder, and a walk of the whole disk is no review), which is the empty path.
#[must_use]
pub fn usable_cwd(cwd: PathBuf) -> PathBuf {
    if cwd.parent().is_none() {
        PathBuf::new()
    } else {
        cwd
    }
}

/// The folder to review: the one named on the command line; else the one reviewed last, when it is
/// still a folder and this run may take it (an app started from the Finder has the root of the disk as
/// its folder; a fixture run never takes what the user did); else the current folder.
pub fn pick_root(
    named: Option<PathBuf>,
    last: Option<&str>,
    may_take_last: bool,
    is_dir: impl Fn(&Path) -> bool,
    cwd: PathBuf,
) -> PathBuf {
    named
        .or_else(|| {
            last.map(str::trim)
                .filter(|l| !l.is_empty() && may_take_last)
                .map(PathBuf::from)
                .filter(|p| is_dir(p))
        })
        .unwrap_or(cwd)
}

#[cfg(test)]
#[path = "switches_test.rs"]
mod switches_test;
