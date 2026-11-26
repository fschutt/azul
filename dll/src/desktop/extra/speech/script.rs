//! The script of a scripted run (`AZ_SPEECH_SCRIPT`, a JSON file a test hook names): what the
//! scripted recognizers of the chain do, call after call. std only.
//!
//! ```json
//! {
//!   "native":  [ {"refuse": "denied", "message": "the microphone is turned off for Terminal"} ],
//!   "builtin": [ {"say": "navigate to Leipzig", "delay_ms": 300, "language": "en"},
//!                {"say": [["delete", 0, 300, 0.99], ["it", 300, 500, 0.8]]},
//!                {"silence": true},
//!                {"fail": "the model ran out of memory", "after_ms": 400, "partial": "delete the"} ],
//!   "then": "repeat",
//!   "preference": "native_first"
//! }
//! ```
//!
//! A step is exactly one of `say` (a text, whose words are 300 ms long at probability 0.95, or a
//! list of `[word, start_ms, end_ms, probability]`), `silence`, `refuse` (`denied` or
//! `unavailable`, with a `message`) and `fail` (a message, `after_ms`, the `partial` text said so
//! far). `delay_ms` (200 by default) is when the words are said, `language` what was recognised.
//! `then` is what happens after the last step: `silence` (the default) or `repeat` (the last
//! step again). A stream (`SpeechSession::start_stream`) walks the same steps: a `say` step is one
//! utterance whose words come as `Partial` events when they end (at their `end_ms`, or at the step's
//! `delay_ms` when that is sooner) and as a `Final` at `delay_ms`, a `silence` step is a stretch of
//! quiet that gives a `Quiet` event after a second, a `refuse` ends the stream (before any event
//! the next recognizer takes it) and a `fail` gives the words of `partial` as partials and then
//! ends the stream with the failure. A recognizer that has no list in the script hears nothing. `preference` is who is
//! asked first: `automatic` (the default: by the call's language and the vocabulary, as a real run
//! does), `native_first` or `builtin_first`.

use super::{
    json::{self, Value},
    mock::{words_of, Step, Then},
    order::Preference,
    recognizer::{Outcome, Word},
};

/// The most steps of one recognizer, words of one step, and the longest text read.
const MAX_STEPS: usize = 1_000;
const MAX_WORDS: usize = 2_000;
const MAX_TEXT: usize = 8_192;
const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
/// The longest delay a step may ask for: ten minutes of a mock clock is already a mistake.
const MAX_DELAY_MS: f64 = 600_000.0;

/// The scripts of the two recognizers a test run puts in the chain.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    /// The system's recognizer; `None`: hears nothing.
    pub native: Option<Vec<Step>>,
    /// The built-in recognizer; `None`: hears nothing.
    pub builtin: Option<Vec<Step>>,
    pub then: Then,
    /// Who is asked first (`automatic` unless the script says).
    pub preference: Preference,
}

/// The script a plain text makes (the old `AZ_SPEECH_HEARD`): that text is heard on every call.
#[must_use]
pub fn heard_every_time(text: &str) -> Vec<Step> {
    let words = words_of(text.trim(), 0, 300, 1.0);
    if words.is_empty() {
        return vec![Step::Silence];
    }
    vec![Step::Say {
        delay_ms: 0,
        words,
        language: String::new(),
    }]
}

/// Reads a script.
///
/// # Errors
///
/// A message naming what is wrong: a text that is not JSON, a member nobody knows, a step that
/// is none of the four, a number out of range.
pub fn parse(text: &str) -> Result<Script, String> {
    if text.len() > MAX_SCRIPT_BYTES {
        return Err(format!("the script is {} bytes long, more than the {MAX_SCRIPT_BYTES} read", text.len()));
    }
    let root = json::parse(text)?;
    let Value::Object(members) = &root else {
        return Err("the script is not a JSON object".to_string());
    };
    let mut script = Script {
        native: None,
        builtin: None,
        then: Then::Silence,
        preference: Preference::Automatic,
    };
    for (key, value) in members {
        match key.as_str() {
            "native" => script.native = Some(steps(value, "native")?),
            "builtin" => script.builtin = Some(steps(value, "builtin")?),
            "then" => {
                script.then = match value.as_text() {
                    Some("silence") => Then::Silence,
                    Some("repeat") => Then::RepeatLast,
                    _ => return Err("\"then\" is \"silence\" or \"repeat\"".to_string()),
                }
            }
            "preference" => {
                script.preference = match value.as_text() {
                    Some("automatic") => Preference::Automatic,
                    Some("native_first") => Preference::NativeFirst,
                    Some("builtin_first") => Preference::BuiltinFirst,
                    _ => return Err("\"preference\" is \"automatic\", \"native_first\" or \"builtin_first\"".to_string()),
                }
            }
            other => return Err(format!("the script has a member nobody knows: \"{other}\"")),
        }
    }
    Ok(script)
}

fn steps(value: &Value, whose: &str) -> Result<Vec<Step>, String> {
    let list = value.as_list().ok_or_else(|| format!("\"{whose}\" is a list of steps"))?;
    if list.len() > MAX_STEPS {
        return Err(format!("\"{whose}\" has {} steps, more than the {MAX_STEPS} read", list.len()));
    }
    list.iter()
        .enumerate()
        .map(|(index, step)| one_step(step).map_err(|why| format!("\"{whose}\" step {index}: {why}")))
        .collect()
}

fn text_of(value: &Value, what: &str) -> Result<String, String> {
    let text = value.as_text().ok_or_else(|| format!("\"{what}\" is a text"))?;
    if text.len() > MAX_TEXT {
        return Err(format!("\"{what}\" is {} bytes long, more than the {MAX_TEXT} read", text.len()));
    }
    Ok(text.to_string())
}

fn millis_of(value: &Value, what: &str) -> Result<u32, String> {
    match value.as_number() {
        Some(n) if n.is_finite() && (0.0..=MAX_DELAY_MS).contains(&n) => Ok(n.round() as u32),
        _ => Err(format!("\"{what}\" is a number of milliseconds between 0 and {MAX_DELAY_MS}")),
    }
}

fn one_step(step: &Value) -> Result<Step, String> {
    let Value::Object(members) = step else {
        return Err("a step is an object".to_string());
    };
    let mut kind: Option<&str> = None;
    let mut delay_ms = 200;
    let mut after_ms = 200;
    let mut language = String::new();
    let mut message = String::new();
    let mut outcome = None;
    let mut partial = String::new();
    let mut said: Option<&Value> = None;
    for (key, value) in members {
        let mut set_kind = |name: &'static str| -> Result<(), String> {
            if kind.is_some() {
                return Err("a step is one of say, silence, refuse and fail, not several".to_string());
            }
            kind = Some(name);
            Ok(())
        };
        match key.as_str() {
            "say" => {
                set_kind("say")?;
                said = Some(value);
            }
            "silence" => set_kind("silence")?,
            "refuse" => {
                set_kind("refuse")?;
                outcome = Some(match value.as_text() {
                    Some("denied") => Outcome::Denied,
                    Some("unavailable") => Outcome::Unavailable,
                    _ => return Err("\"refuse\" is \"denied\" or \"unavailable\"".to_string()),
                });
            }
            "fail" => {
                set_kind("fail")?;
                message = text_of(value, "fail")?;
            }
            "message" => message = text_of(value, "message")?,
            "delay_ms" => delay_ms = millis_of(value, "delay_ms")?,
            "after_ms" => after_ms = millis_of(value, "after_ms")?,
            "language" => language = text_of(value, "language")?,
            "partial" => partial = text_of(value, "partial")?,
            other => return Err(format!("a step has a member nobody knows: \"{other}\"")),
        }
    }
    match kind {
        Some("say") => Ok(Step::Say {
            delay_ms,
            words: words_said(said.ok_or("\"say\" is missing")?)?,
            language,
        }),
        Some("silence") => Ok(Step::Silence),
        Some("refuse") => Ok(Step::Refuse {
            outcome: outcome.unwrap_or(Outcome::Unavailable),
            message,
        }),
        Some("fail") => Ok(Step::Fail {
            after_ms,
            message,
            partial: words_of(partial.trim(), 0, 300, 0.9),
        }),
        _ => Err("a step is one of say, silence, refuse and fail".to_string()),
    }
}

/// What a `say` holds: a text, or a list of `[word, start_ms, end_ms, probability]`.
fn words_said(value: &Value) -> Result<Vec<Word>, String> {
    match value {
        Value::Text(text) => {
            if text.len() > MAX_TEXT {
                return Err(format!("\"say\" is {} bytes long, more than the {MAX_TEXT} read", text.len()));
            }
            Ok(words_of(text, 0, 300, 0.95))
        }
        Value::List(items) => {
            if items.len() > MAX_WORDS {
                return Err(format!("\"say\" has {} words, more than the {MAX_WORDS} read", items.len()));
            }
            items
                .iter()
                .map(|item| {
                    let parts = item.as_list().filter(|p| p.len() == 4).ok_or(
                        "a word is [\"text\", start_ms, end_ms, probability]".to_string(),
                    )?;
                    let text = text_of(&parts[0], "word")?;
                    let start = millis_of(&parts[1], "start_ms")?;
                    let end = millis_of(&parts[2], "end_ms")?.max(start);
                    let probability = parts[3].as_number().filter(|p| (0.0..=1.0).contains(p)).ok_or(
                        "a word's probability is a number between 0 and 1".to_string(),
                    )?;
                    Ok(Word::new(&text, start, end, probability as f32))
                })
                .collect()
        }
        _ => Err("\"say\" is a text or a list of words".to_string()),
    }
}

#[cfg(test)]
#[path = "script_test.rs"]
mod script_test;
