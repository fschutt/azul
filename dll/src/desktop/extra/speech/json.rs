//! A small JSON reader (the speech module is built in every feature set, `serde_json` is not),
//! bounded in depth and size so a broken text is an error and never a stack overflow. It reads
//! the built-in engine's answer, `{"text":"...","language":"de","words":[{"word":"Bitte",
//! "start":0.08,"end":0.32,"probability":0.996},...],"ttft_ms":54.7,"decode_tps":250.4}`
//! ([`transcript`]), and the scripts of a scripted run ([`super::script`]). std only.

// The items are `pub` for the tests and for the parts of the module that are re-exported.
#![allow(unreachable_pub)]

use super::recognizer::Word;

/// The longest answer read: a minute of speech is a few hundred words, a few tens of kilobytes.
pub const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
/// The deepest nesting read.
const MAX_DEPTH: usize = 16;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The member `key` of an object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(*number),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// Reads one JSON value (and nothing after it but white space).
///
/// # Errors
///
/// A message naming what is wrong and where.
pub fn parse(text: &str) -> Result<Value, String> {
    if text.len() > MAX_JSON_BYTES {
        return Err(format!("the answer is {} bytes long, more than the {MAX_JSON_BYTES} read", text.len()));
    }
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("something follows the value"));
    }
    Ok(value)
}

impl Reader<'_> {
    fn error(&self, what: &str) -> String {
        format!("invalid JSON at byte {}: {what}", self.at)
    }

    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn expect(&mut self, byte: u8, what: &str) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(what))
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("not a value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.skip_space();
        match self.peek() {
            None => Err(self.error("the text ends where a value should be")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.list(depth),
            Some(b'"') => Ok(Value::Text(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("not a value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            self.expect(b':', "a colon after the name")?;
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(members));
                }
                _ => return Err(self.error("a comma or a closing brace")),
            }
        }
    }

    fn list(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::List(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::List(items));
                }
                _ => return Err(self.error("a comma or a closing bracket")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let begin = self.at;
        while self.at < self.bytes.len()
            && matches!(self.bytes[self.at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[begin..self.at]).map_err(|_| self.error("not a number"))?;
        match text.parse::<f64>() {
            Ok(number) if number.is_finite() => Ok(Value::Number(number)),
            _ => Err(self.error("not a number")),
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.bytes.get(self.at..self.at + 4).ok_or_else(|| self.error("a short \\u escape"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("a bad \\u escape"))?;
        let value = u32::from_str_radix(text, 16).map_err(|_| self.error("a bad \\u escape"))?;
        self.at += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"', "a string")?;
        let mut out = String::new();
        loop {
            let begin = self.at;
            while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b'"' | b'\\') {
                self.at += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.bytes[begin..self.at]).map_err(|_| self.error("not UTF-8"))?,
            );
            match self.peek() {
                None => return Err(self.error("a string that never ends")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(_) => {
                    self.at += 1;
                    let escape = self.peek().ok_or_else(|| self.error("an escape that never ends"))?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&first) {
                                // A high surrogate needs its low half.
                                if self.bytes[self.at..].starts_with(b"\\u") {
                                    self.at += 2;
                                    let second = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&second) {
                                        return Err(self.error("a lone surrogate"));
                                    }
                                    0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                                } else {
                                    return Err(self.error("a lone surrogate"));
                                }
                            } else {
                                first
                            };
                            out.push(char::from_u32(code).ok_or_else(|| self.error("not a character"))?);
                        }
                        _ => return Err(self.error("an unknown escape")),
                    }
                }
            }
        }
    }
}

/// What the engine understood.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    /// The language the engine recognised (`en`, `de`, ...); empty when it heard nothing.
    pub language: String,
    pub words: Vec<Word>,
}

/// Seconds as milliseconds, kept to what a `u32` holds.
fn millis(seconds: f64) -> u32 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let ms = (seconds * 1000.0).round();
    if ms >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        ms as u32
    }
}

/// Reads the engine's answer to a transcription.
///
/// # Errors
///
/// A message when the text is not JSON or is not an object. Members the engine leaves out are
/// empty; a word that is not an object with a text is skipped.
pub fn transcript(text: &str) -> Result<Transcript, String> {
    let value = parse(text)?;
    if !matches!(value, Value::Object(_)) {
        return Err("the engine's answer is not a JSON object".to_string());
    }
    let words = value
        .get("words")
        .and_then(Value::as_list)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let word = entry.get("word")?.as_text()?;
            let start = entry.get("start").and_then(Value::as_number).unwrap_or(0.0);
            let end = entry.get("end").and_then(Value::as_number).unwrap_or(start);
            let probability = entry.get("probability").and_then(Value::as_number).unwrap_or(0.0);
            let start_ms = millis(start);
            Some(Word {
                text: word.to_string(),
                start_ms,
                end_ms: millis(end).max(start_ms),
                probability: super::recognizer::clamp01(probability as f32),
                biased: false,
            })
        })
        .collect();
    Ok(Transcript {
        text: value.get("text").and_then(Value::as_text).unwrap_or_default().trim().to_string(),
        language: value.get("language").and_then(Value::as_text).unwrap_or_default().to_string(),
        words,
    })
}

#[cfg(test)]
#[path = "json_test.rs"]
mod json_test;
