//! The agents' JSON: `serde_json` behind a small facade (the CLIs' envelopes, the APIs' bodies,
//! the model's own reply). The input is bounded BEFORE it is parsed; the parser's own recursion
//! limit bounds the nesting. A model's reply is not a serializer's output, so [`parse_lenient`]
//! also reads what a model commonly gets wrong - a raw line break inside a string, a comma before a
//! closing bracket - and says that it did; [`find_object`] finds the reply's object between prose
//! and code fences.

use std::ops::Range;

pub use serde_json::{json, Value as Json};

/// The most bytes of JSON text any agent reader looks at (the CLIs' envelopes and the replies are
/// kilobytes; a megabyte is a runaway).
pub const MAX_JSON_BYTES: usize = 1 << 20;

/// Why a text is no JSON the agents can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonError {
    /// Nothing but white space.
    Empty,
    /// More bytes than the reader looks at.
    TooLarge { size: usize, limit: usize },
    /// Not JSON: where, and what the parser said.
    Syntax {
        line: usize,
        column: usize,
        what: String,
    },
}

/// An object found between other text.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub value: Json,
    /// Where it is in the text searched, in bytes.
    pub range: Range<usize>,
    /// It was read after [`repair`].
    pub repaired: bool,
}

/// The most plausible starts of an object [`find_object`] tries before it gives up.
const MAX_CANDIDATES: usize = 64;

fn syntax(e: &serde_json::Error) -> JsonError {
    JsonError::Syntax {
        line: e.line(),
        column: e.column(),
        what: e.to_string(),
    }
}

/// Parses `text` as one JSON value, strictly.
///
/// # Errors
/// [`JsonError`]: empty, bigger than `limit` bytes (checked first), or not JSON.
pub fn parse(text: &str, limit: usize) -> Result<Json, JsonError> {
    if text.len() > limit {
        return Err(JsonError::TooLarge {
            size: text.len(),
            limit,
        });
    }
    if text.trim().is_empty() {
        return Err(JsonError::Empty);
    }
    serde_json::from_str::<Json>(text).map_err(|e| syntax(&e))
}

/// Like [`parse`], and when the strict reading fails, again after [`repair`]; the flag says the
/// repair was needed.
///
/// # Errors
/// [`JsonError`] as [`parse`], with the position of the strict reading's complaint.
pub fn parse_lenient(text: &str, limit: usize) -> Result<(Json, bool), JsonError> {
    match parse(text, limit) {
        Ok(value) => Ok((value, false)),
        Err(strict @ JsonError::Syntax { .. }) => {
            let fixed = repair(text);
            if fixed == text {
                return Err(strict);
            }
            // `text` was bounded above; its repair is at most six times as long (an escape).
            match serde_json::from_str::<Json>(&fixed) {
                Ok(value) => Ok((value, true)),
                Err(_) => Err(strict),
            }
        }
        Err(other) => Err(other),
    }
}

/// `text` with what models commonly get wrong put right: a raw control character inside a string
/// becomes its escape, a comma before a closing bracket goes. Valid JSON comes back unchanged.
#[must_use]
pub fn repair(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                out.push(c);
            } else if c == '\\' {
                escaped = true;
                out.push(c);
            } else if c == '"' {
                in_string = false;
                out.push(c);
            } else if c < ' ' {
                match c {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    other => out.push_str(&format!("\\u{:04x}", u32::from(other))),
                }
            } else {
                out.push(c);
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' && matches!(text[at + 1..].trim_start().chars().next(), Some('}' | ']'))
        {
            // a comma before a closing bracket: dropped
        } else {
            out.push(c);
        }
    }
    out
}

/// Where the object that starts at `s[0]` (a `{`) ends, by matching its brackets with strings
/// skipped; `None` when it never closes.
fn object_end(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (at, c) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '{' | '[' => depth += 1,
                '}' | ']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(at + c.len_utf8());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// The first JSON object in `text` (prose and code fences around it are skipped; a brace that
/// starts no object is passed over: an object starts with a brace and a quote or a closing brace).
///
/// # Errors
/// [`JsonError::TooLarge`] when `text` is bigger than `limit` bytes.
pub fn find_object(text: &str, limit: usize) -> Result<Option<Found>, JsonError> {
    find_object_where(text, limit, &|_| true)
}

/// The first JSON object in `text` that `wanted` accepts: an example object in a prose answer is
/// passed over when it is not the one that was asked for.
///
/// # Errors
/// [`JsonError::TooLarge`] when `text` is bigger than `limit` bytes.
pub fn find_object_where(
    text: &str,
    limit: usize,
    wanted: &dyn Fn(&Json) -> bool,
) -> Result<Option<Found>, JsonError> {
    if text.len() > limit {
        return Err(JsonError::TooLarge {
            size: text.len(),
            limit,
        });
    }
    let mut from = 0;
    let mut tried = 0;
    while let Some(rel) = text[from..].find('{') {
        let start = from + rel;
        from = start + 1;
        if !matches!(text[from..].trim_start().chars().next(), Some('"' | '}')) {
            continue;
        }
        tried += 1;
        if tried > MAX_CANDIDATES {
            break;
        }
        let Some(len) = object_end(&text[start..]) else {
            continue;
        };
        let slice = &text[start..start + len];
        if let Ok(value @ Json::Object(_)) = serde_json::from_str::<Json>(slice) {
            if wanted(&value) {
                return Ok(Some(Found {
                    value,
                    range: start..start + len,
                    repaired: false,
                }));
            }
            // an object that is not the one asked for: what is inside it is not either
            from = start + len;
            continue;
        }
        if let Ok(value @ Json::Object(_)) = serde_json::from_str::<Json>(&repair(slice)) {
            if wanted(&value) {
                return Ok(Some(Found {
                    value,
                    range: start..start + len,
                    repaired: true,
                }));
            }
            from = start + len;
        }
    }
    Ok(None)
}

/// The value under `key` of an object.
#[must_use]
pub fn get<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.as_object()?.get(key)
}

/// The string under `key`.
#[must_use]
pub fn str_of<'a>(value: &'a Json, key: &str) -> Option<&'a str> {
    get(value, key)?.as_str()
}

/// The whole number under `key` (never negative).
#[must_use]
pub fn u64_of(value: &Json, key: &str) -> Option<u64> {
    let n = get(value, key)?;
    n.as_u64().or_else(|| {
        n.as_f64()
            .filter(|f| f.fract() == 0.0 && *f >= 0.0 && *f < 1.0e18)
            // checked: a whole number inside the range of u64
            .map(|f| f as u64)
    })
}

/// The number under `key` (finite).
#[must_use]
pub fn f64_of(value: &Json, key: &str) -> Option<f64> {
    get(value, key)?.as_f64().filter(|f| f.is_finite())
}

/// The truth under `key`.
#[must_use]
pub fn bool_of(value: &Json, key: &str) -> Option<bool> {
    get(value, key)?.as_bool()
}

/// The array under `key`.
#[must_use]
pub fn array_of<'a>(value: &'a Json, key: &str) -> Option<&'a [Json]> {
    get(value, key)?.as_array().map(Vec::as_slice)
}

/// `text` as a JSON string, quotes included.
#[must_use]
pub fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""))
}

/// The compact text of `value`.
#[must_use]
pub fn to_text(value: &Json) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
#[path = "agent_json_test.rs"]
mod agent_json_test;
