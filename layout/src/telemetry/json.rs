//! Minimal JSON value type — just enough to write OTLP payloads and read the
//! layered telemetry config files.
//!
//! Deliberately hand-rolled rather than reaching for `serde_json`:
//!
//! * the OTLP/HTTP JSON encoding needs exact control, because the spec's
//!   proto3 JSON mapping encodes every 64-bit integer as a *string*
//!   (`"timeUnixNano": "1723..."`, `"asInt": "42"`) — a `serde` round trip
//!   through `u64` would silently emit bare numbers and lose precision at the
//!   receiver;
//! * the config schema is six flat keys;
//! * and the `telemetry` feature must not drag the optional `json` feature
//!   (and with it `serde_json`) into every build that wants metrics.
//!
//! The parser is a plain recursive-descent reader with a depth limit. It is
//! not a general-purpose JSON implementation and does not try to be.

use std::fmt;

/// Maximum nesting depth accepted by [`parse`]. Config files are flat; this
/// only exists so a malformed file cannot blow the stack.
const MAX_DEPTH: u32 = 32;

/// A parsed or constructed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// `null`
    Null,
    /// `true` / `false`
    Bool(bool),
    /// Any JSON number, held as `f64`.
    Number(f64),
    /// A string.
    Str(String),
    /// An array.
    Array(Vec<JsonValue>),
    /// An object. Insertion-ordered so encoded output is deterministic.
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Looks a key up in an object. Returns `None` for non-objects.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The string contents, if this is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The value as `u64`, if this is a finite non-negative number. Also
    /// accepts a numeric *string*, since OTLP-style configs and hand-edited
    /// files routinely quote integers.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(n) if n.is_finite() && *n >= 0.0 => Some(*n as u64),
            Self::Str(s) => s.parse::<u64>().ok(),
            _ => None,
        }
    }

    /// The value as `bool`, if this is a boolean.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The elements, if this is an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// The fields, if this is an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(fields) => Some(fields.as_slice()),
            _ => None,
        }
    }

    /// Appends the compact JSON encoding of this value to `out`.
    pub fn write_to(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Number(n) => write_number(out, *n),
            Self::Str(s) => write_string(out, s),
            Self::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Self::Object(fields) => {
                out.push('{');
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write_to(out);
                }
                out.push('}');
            }
        }
    }

    /// The compact JSON encoding of this value.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_json_string())
    }
}

/// Appends a JSON string literal (including the surrounding quotes) to `out`.
///
/// Escapes the two mandatory characters, the shorthand control escapes, and
/// every remaining C0 control as `\u00XX`. Non-ASCII passes through as UTF-8,
/// which is what the OTLP JSON encoding expects.
pub fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                for shift in [12_u32, 8, 4, 0] {
                    let nibble = ((c as u32) >> shift) & 0xf;
                    out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Appends a JSON number to `out`.
///
/// JSON has no `NaN`/`Infinity`, so non-finite values are written as `0` —
/// a dropped data point beats an unparseable payload at the collector.
pub fn write_number(out: &mut String, n: f64) {
    use std::fmt::Write as _;

    if !n.is_finite() {
        out.push('0');
        return;
    }
    if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
        let _ = write!(out, "{}", n as i64);
    } else {
        let _ = write!(out, "{n}");
    }
}

/// A JSON parse failure, with the byte offset it was detected at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// Human-readable description.
    pub message: String,
    /// Byte offset into the input.
    pub offset: usize,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for JsonError {}

/// Parses a complete JSON document.
///
/// # Errors
///
/// Returns a [`JsonError`] if the input is not a single well-formed JSON
/// value, or if it nests deeper than 32 levels.
pub fn parse(input: &str) -> Result<JsonValue, JsonError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing input after JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> JsonError {
        JsonError {
            message: message.to_owned(),
            offset: self.pos,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error("unexpected character"))
        }
    }

    fn literal(&mut self, word: &str, value: JsonValue) -> Result<JsonValue, JsonError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn parse_value(&mut self, depth: u32) -> Result<JsonValue, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error("maximum nesting depth exceeded"));
        }
        match self.peek() {
            None => Err(self.error("unexpected end of input")),
            Some(b'n') => self.literal("null", JsonValue::Null),
            Some(b't') => self.literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.literal("false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_object(depth),
            Some(_) => self.parse_number(),
        }
    }

    fn parse_array(&mut self, depth: u32) -> Result<JsonValue, JsonError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.parse_value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn parse_object(&mut self, depth: u32) -> Result<JsonValue, JsonError> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.parse_value(depth + 1)?;
            fields.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(fields));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(esc) = self.peek() else {
                        return Err(self.error("unterminated escape"));
                    };
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.parse_unicode_escape()?),
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                _ => {
                    // Copy one whole UTF-8 scalar so multi-byte characters
                    // survive intact.
                    let rest = &self.bytes[self.pos..];
                    let s = std::str::from_utf8(rest)
                        .map_err(|_| self.error("invalid UTF-8 in string"))?;
                    let Some(c) = s.chars().next() else {
                        return Err(self.error("unterminated string"));
                    };
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    /// Reads the four hex digits after `\u`, joining a surrogate pair if one
    /// follows. Lone surrogates become U+FFFD rather than failing the parse.
    fn parse_unicode_escape(&mut self) -> Result<char, JsonError> {
        let high = self.parse_hex4()?;
        if (0xD800..0xDC00).contains(&high) {
            // Expect a trailing surrogate: \uDC00-\uDFFF
            if self.bytes[self.pos..].starts_with(b"\\u") {
                let save = self.pos;
                self.pos += 2;
                let low = self.parse_hex4()?;
                if (0xDC00..0xE000).contains(&low) {
                    let combined = 0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                    return Ok(char::from_u32(combined).unwrap_or('\u{fffd}'));
                }
                self.pos = save;
            }
            return Ok('\u{fffd}');
        }
        Ok(char::from_u32(high).unwrap_or('\u{fffd}'))
    }

    fn parse_hex4(&mut self) -> Result<u32, JsonError> {
        let end = self.pos + 4;
        if end > self.bytes.len() {
            return Err(self.error("truncated \\u escape"));
        }
        let mut value = 0_u32;
        for &byte in &self.bytes[self.pos..end] {
            let digit = char::from(byte)
                .to_digit(16)
                .ok_or_else(|| self.error("invalid hex digit"))?;
            value = value * 16 + digit;
        }
        self.pos = end;
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.error("expected a value"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid UTF-8 in number"))?;
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| JsonError {
                message: "invalid number".to_owned(),
                offset: start,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_flat_config_object() {
        let value = parse(
            r#"{"tier":"metrics","client_id":"abc","flush_interval_secs":60,"asked_versions":["1.4"]}"#,
        )
        .expect("valid JSON");
        assert_eq!(value.get("tier").and_then(JsonValue::as_str), Some("metrics"));
        assert_eq!(value.get("client_id").and_then(JsonValue::as_str), Some("abc"));
        assert_eq!(value.get("flush_interval_secs").and_then(JsonValue::as_u64), Some(60));
        assert_eq!(
            value.get("asked_versions").and_then(JsonValue::as_array).map(<[JsonValue]>::len),
            Some(1)
        );
    }

    #[test]
    fn round_trips_escapes_and_unicode() {
        let original = "quote:\" backslash:\\ newline:\n tab:\t bell:\u{7} snowman:☃";
        let mut encoded = String::new();
        write_string(&mut encoded, original);
        // The C0 control must have been escaped as , not emitted raw.
        assert!(encoded.contains("\\u0007"), "encoded = {encoded}");
        let decoded = parse(&encoded).expect("re-parses");
        assert_eq!(decoded.as_str(), Some(original));
    }

    #[test]
    fn parses_surrogate_pairs() {
        let value = parse(r#""😀""#).expect("valid JSON");
        assert_eq!(value.as_str(), Some("😀"));
    }

    #[test]
    fn writes_integers_without_a_decimal_point() {
        // OTLP `asDouble` accepts either, but bare integers keep payloads
        // small and diffable, and the consent preview is the payload.
        let mut out = String::new();
        write_number(&mut out, 42.0);
        assert_eq!(out, "42");
        out.clear();
        write_number(&mut out, 0.5);
        assert_eq!(out, "0.5");
        out.clear();
        write_number(&mut out, f64::NAN);
        assert_eq!(out, "0");
    }

    #[test]
    fn rejects_trailing_input_and_deep_nesting() {
        assert!(parse("{} {}").is_err());
        let deep = "[".repeat(64);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn object_encoding_is_insertion_ordered() {
        let value = JsonValue::Object(vec![
            ("b".to_owned(), JsonValue::Number(1.0)),
            ("a".to_owned(), JsonValue::Bool(true)),
        ]);
        assert_eq!(value.to_json_string(), r#"{"b":1,"a":true}"#);
    }
}
