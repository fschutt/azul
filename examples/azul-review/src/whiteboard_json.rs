//! A small JSON reader and writer for the whiteboard's file (`whiteboard_store`): std only, so the
//! whole of what a board file may hold is tested in seconds, and strict, so a file that is not one is
//! told so with the place it went wrong. Bounded: a document is read to at most [`MAX_DEPTH`] levels
//! and [`MAX_TEXT`] bytes, and the parser uses no recursion deeper than that.

/// How deep arrays and objects nest at most.
pub const MAX_DEPTH: usize = 16;
/// The longest text that is read (a board file's manifest is a few hundred kilobytes at most).
pub const MAX_TEXT: usize = 16 * 1024 * 1024;

/// A JSON value. An object keeps its keys in the order of the text; the first of two equal keys is the
/// one [`Json::get`] finds.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

/// Why a text is no JSON, and where (a byte offset).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonError {
    pub at: usize,
    pub why: &'static str,
}

impl Json {
    /// The value under `key` of an object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// A number as `f32` (a value too large to be one is infinite: the reader of a board takes care).
    #[must_use]
    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|n| n as f32)
    }

    /// A whole number from 0 up to 2^53 (what a double holds exactly).
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        let n = self.as_f64()?;
        (n >= 0.0 && n <= 9_007_199_254_740_992.0 && n.fract() == 0.0).then_some(n as u64)
    }

    #[must_use]
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(items) => Some(items),
            _ => None,
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

type Res<T> = Result<T, JsonError>;

impl Parser<'_> {
    fn fail<T>(&self, why: &'static str) -> Res<T> {
        Err(JsonError { at: self.at, why })
    }

    fn skip_space(&mut self) {
        while matches!(self.text.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Res<Json> {
        if self.text[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            self.fail("this is no JSON value")
        }
    }

    fn value(&mut self, depth: usize) -> Res<Json> {
        self.skip_space();
        match self.text.get(self.at) {
            None => self.fail("the text ends where a value should be"),
            Some(b'n') => self.word("null", Json::Null),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.fail("this is no JSON value"),
        }
    }

    fn array(&mut self, depth: usize) -> Res<Json> {
        if depth >= MAX_DEPTH {
            return self.fail("the text nests deeper than a board file does");
        }
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.eat(b']') {
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_space();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Json::Arr(items));
            }
            return self.fail("a comma or the end of the list was expected");
        }
    }

    fn object(&mut self, depth: usize) -> Res<Json> {
        if depth >= MAX_DEPTH {
            return self.fail("the text nests deeper than a board file does");
        }
        self.at += 1;
        let mut pairs = Vec::new();
        self.skip_space();
        if self.eat(b'}') {
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_space();
            if self.text.get(self.at) != Some(&b'"') {
                return self.fail("a key in quotes was expected");
            }
            let key = self.string()?;
            self.skip_space();
            if !self.eat(b':') {
                return self.fail("a colon was expected after the key");
            }
            let value = self.value(depth + 1)?;
            pairs.push((key, value));
            self.skip_space();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Json::Obj(pairs));
            }
            return self.fail("a comma or the end of the object was expected");
        }
    }

    fn number(&mut self) -> Res<Json> {
        let start = self.at;
        self.eat(b'-');
        match self.text.get(self.at) {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return self.fail("a digit was expected"),
        }
        if self.eat(b'.') {
            if !matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
                return self.fail("a digit was expected after the point");
            }
            while matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if matches!(self.text.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.text.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
                return self.fail("a digit was expected in the exponent");
            }
            while matches!(self.text.get(self.at), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        // The slice is ASCII digits and signs only.
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .map(Json::Num)
            .map_or_else(|| self.fail("this number is out of range"), Ok)
    }

    fn hex4(&mut self) -> Res<u32> {
        let mut v = 0_u32;
        for _ in 0..4 {
            let Some(d) = self.text.get(self.at).and_then(|b| (*b as char).to_digit(16)) else {
                return self.fail("four hexadecimal digits were expected after \\u");
            };
            v = v * 16 + d;
            self.at += 1;
        }
        Ok(v)
    }

    fn string(&mut self) -> Res<String> {
        self.at += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&b) = self.text.get(self.at) else {
                return self.fail("the string is not closed");
            };
            match b {
                b'"' => {
                    self.at += 1;
                    return String::from_utf8(out).or_else(|_| self.fail("the string is no valid UTF-8"));
                }
                0..=0x1f => return self.fail("a control character in a string must be escaped"),
                b'\\' => {
                    self.at += 1;
                    let Some(&e) = self.text.get(self.at) else {
                        return self.fail("the string is not closed");
                    };
                    self.at += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return self.fail("a high surrogate needs a low one after it");
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return self.fail("a high surrogate needs a low one after it");
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return self.fail("a low surrogate without a high one before it");
                            } else {
                                hi
                            };
                            match char::from_u32(code) {
                                Some(c) => c,
                                None => return self.fail("this is no character"),
                            }
                        }
                        _ => return self.fail("this escape is not JSON"),
                    };
                    let mut buf = [0_u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                _ => {
                    out.push(b);
                    self.at += 1;
                }
            }
        }
    }
}

/// Reads `text` as one JSON value.
///
/// # Errors
/// Why `text` is no JSON (or one a board file cannot be), and the byte it went wrong at.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    parse_with(text, MAX_TEXT)
}

/// Reads `text` as one JSON value, when it is no longer than `max_text` bytes (a document that holds its pictures in the
/// clear is far longer than a manifest is).
///
/// # Errors
/// Why `text` is no JSON (or one a board file cannot be), and the byte it went wrong at.
pub fn parse_with(text: &str, max_text: usize) -> Result<Json, JsonError> {
    if text.len() > max_text {
        return Err(JsonError { at: 0, why: "the text is longer than a board file may be" });
    }
    let mut p = Parser { text: text.as_bytes(), at: 0 };
    let v = p.value(0)?;
    p.skip_space();
    if p.at != p.text.len() {
        return p.fail("there is text after the value");
    }
    Ok(v)
}

/// Appends `text` as a JSON string literal.
pub fn push_string(out: &mut String, text: &str) {
    use std::fmt::Write;
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A number as short as it reads back (`0`, `3`, `-2.5`, `0.1`); what is no number is 0.
#[must_use]
pub fn num(v: f32) -> String {
    if !v.is_finite() || v == 0.0 {
        return "0".to_string();
    }
    format!("{v}")
}

/// The JSON number of an `f32` with the digits [`num`] gives (`0.1` stays `0.1` and does not become the `f64` that is
/// nearest to the `f32`), so that a document written from `f32`s reads back to the same `f32`s and is short.
#[must_use]
pub fn from_f32(v: f32) -> Json {
    Json::Num(num(v).parse::<f64>().unwrap_or(0.0))
}

/// A whole number (an id, a time in milliseconds): exact up to 2^53.
#[must_use]
pub fn from_u64(v: u64) -> Json {
    Json::Num(v.min(9_007_199_254_740_992) as f64)
}

/// A text.
#[must_use]
pub fn from_str(v: &str) -> Json {
    Json::Str(v.to_string())
}

/// An object from its fields, in the order given.
#[must_use]
pub fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// A number as ECMAScript prints it (`Number.prototype.toString`: RFC 8785, section 3.2.2.3): the shortest digits that read
/// back to the same double, without an exponent from 1e-6 up to 1e21, with one (`1e+21`, `1e-7`) beyond. `0` for zero (and
/// for minus zero) and for what is no number.
fn es_number(n: f64) -> String {
    if n == 0.0 || !n.is_finite() {
        return "0".to_string();
    }
    // Rust's `{:e}` gives the shortest digits and their exponent (`1.2345e-3`); how many of them is what is wanted: of the
    // numbers that many digits make, ECMAScript takes the one closest to the double, and the even one when two are as close
    // (Rust's shortest form takes the larger), which is the double correctly rounded to that many digits.
    let shortest = format!("{:e}", n.abs());
    let k = shortest.split_once('e').map_or(1, |(m, _)| m.chars().filter(char::is_ascii_digit).count());
    let sci = format!("{:.*e}", k.saturating_sub(1), n.abs());
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    // The decimal point comes after `point` digits (the spec's n).
    let point = exponent.parse::<i32>().unwrap_or(0) + 1;
    let body = if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat(usize::try_from(point - k).unwrap_or(0)))
    } else if 0 < point && point <= 21 {
        let cut = usize::try_from(point).unwrap_or(0);
        format!("{}.{}", &digits[..cut], &digits[cut..])
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat(usize::try_from(-point).unwrap_or(0)))
    } else {
        let e = point - 1;
        let sign = if e < 0 { '-' } else { '+' };
        if k == 1 {
            format!("{digits}e{sign}{}", e.abs())
        } else {
            format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
        }
    };
    if n < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// A string as RFC 8785 writes it: only the quote, the backslash and the control characters are escaped (`\b \t \n \f \r`
/// short, the others `\u00xx` in lower case); everything else, DEL and the characters past the plane 0 included, as it is.
fn jcs_string(out: &mut String, text: &str) {
    use std::fmt::Write;
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn jcs(v: &Json, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(&es_number(*n)),
        Json::Str(s) => jcs_string(out, s),
        Json::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                jcs(item, out);
            }
            out.push(']');
        }
        Json::Obj(pairs) => {
            // The members sorted by their names as sequences of UTF-16 code units (not of code points: a character past
            // the plane 0 sorts before U+E000).
            let mut sorted: Vec<&(String, Json)> = pairs.iter().collect();
            sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push('{');
            for (i, (k, item)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                jcs_string(out, k);
                out.push(':');
                jcs(item, out);
            }
            out.push('}');
        }
    }
}

/// `v` in the canonical form of RFC 8785 (JCS): no white space, the members of an object sorted by name, numbers and
/// strings as ECMAScript writes them. The same value is always the same text, which is what a hash is taken of; any
/// JCS library gives the same bytes.
#[must_use]
pub fn canonical(v: &Json) -> String {
    let mut out = String::new();
    jcs(v, &mut out);
    out
}

/// The longest a list or an object is that is written on one line, characters.
const INLINE: usize = 100;

fn scalar(v: &Json) -> bool {
    !matches!(v, Json::Arr(_) | Json::Obj(_))
}

fn push_scalar(out: &mut String, v: &Json) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => {
            if n.is_finite() {
                out.push_str(&format!("{n}"));
            } else {
                out.push('0');
            }
        }
        Json::Str(s) => push_string(out, s),
        Json::Arr(_) | Json::Obj(_) => {}
    }
}

/// `v` on one line, when it is a list or object of scalars that is short enough, or a scalar.
fn inline(v: &Json) -> Option<String> {
    let mut out = String::new();
    match v {
        Json::Arr(items) if items.iter().all(scalar) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_scalar(&mut out, item);
                if out.len() > INLINE {
                    return None;
                }
            }
            out.push(']');
        }
        Json::Obj(pairs) if pairs.iter().all(|(_, v)| scalar(v)) => {
            out.push('{');
            for (i, (k, item)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_string(&mut out, k);
                out.push_str(": ");
                push_scalar(&mut out, item);
                if out.len() > INLINE {
                    return None;
                }
            }
            out.push('}');
        }
        Json::Arr(_) | Json::Obj(_) => return None,
        other => push_scalar(&mut out, other),
    }
    (out.len() <= INLINE || scalar(v)).then_some(out)
}

fn render(v: &Json, depth: usize, out: &mut String) {
    if let Some(line) = inline(v) {
        out.push_str(&line);
        return;
    }
    let pad = "  ".repeat(depth + 1);
    match v {
        Json::Arr(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                render(item, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(depth));
            out.push(']');
        }
        Json::Obj(pairs) => {
            out.push_str("{\n");
            for (i, (k, item)) in pairs.iter().enumerate() {
                out.push_str(&pad);
                push_string(out, k);
                out.push_str(": ");
                render(item, depth + 1, out);
                out.push_str(if i + 1 < pairs.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
        scalar_value => push_scalar(out, scalar_value),
    }
}

/// `v` written as [`pretty`] writes it when it lies `depth` levels down in a document: the lines after the first are indented for
/// that depth, the first has no indentation and the text has no line feed at its end. A text made once can be put in a document
/// without being written again.
#[must_use]
pub fn pretty_at(v: &Json, depth: usize) -> String {
    let mut out = String::new();
    render(v, depth, &mut out);
    out
}

/// `v` as text a person can read and a diff can follow: two spaces for each level, a list or object of a few scalars
/// on one line, a line feed at the end. What [`parse`] reads back to `v`.
#[must_use]
pub fn pretty(v: &Json) -> String {
    let mut out = String::new();
    render(v, 0, &mut out);
    out.push('\n');
    out
}

#[cfg(test)]
#[path = "whiteboard_json_test.rs"]
mod whiteboard_json_test;
