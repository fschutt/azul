//! Helpers the test files share: hex, a seeded generator, the diagnostic notation of RFC 8949
//! section 8 for the profile, and a reader and writer of the vector file (a tiny JSON subset, as
//! the crate has no dependency and the tests none either).
#![allow(dead_code)]

pub mod fuzz_entry;

use std::path::PathBuf;

use azlin_cbor::Value;

/// SplitMix64: the seeded generator of every property test (no `rand`). A failing case prints its
/// seed and index, and `SplitMix64::new(seed)` replays it.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`); the modulo bias is irrelevant for tests.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u64().to_le_bytes()[0]).collect()
    }
}

pub fn from_hex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "odd hex length: {s}");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The diagnostic notation of RFC 8949 section 8 for the profile: what the vector file's
/// `expect` of a valid vector holds, so the file states the value without the codec.
pub fn diag(v: &Value) -> String {
    match v {
        Value::Uint(n) => n.to_string(),
        Value::Nint(n) => (-1i128 - i128::from(*n)).to_string(),
        Value::Bytes(b) => format!("h'{}'", to_hex(b)),
        Value::Text(s) => diag_text(s),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(diag).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(m) => {
            let parts: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {}", diag(k), diag(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn diag_text(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// One row of a vector file: `{name, valid, hex, expect}` (rule 10 of the launch rules). `expect`
/// is the diagnostic notation of the value for a valid vector and the name of the `DecodeError`
/// variant for an invalid one; every vector is decoded under `Limits::OBJECT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vector {
    pub name: String,
    pub valid: bool,
    pub hex: String,
    pub expect: String,
}

pub fn vector_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/cbor_strict.json")
}

pub fn load_vectors() -> Vec<Vector> {
    let text = std::fs::read_to_string(vector_path())
        .expect("tests/vectors/cbor_strict.json exists (UPDATE_VECTORS=1 writes it, never in CI)");
    parse_vectors(&text)
}

pub fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// The vector file text of these rows: one object per line, so a diff shows one vector.
pub fn render_json(rows: &[Vector]) -> String {
    let mut out = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "  {{\"name\": {}, \"valid\": {}, \"hex\": {}, \"expect\": {}}}",
            json_str(&r.name),
            r.valid,
            json_str(&r.hex),
            json_str(&r.expect)
        ));
        out.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) {
        self.ws();
        assert_eq!(
            self.s.get(self.i),
            Some(&c),
            "vector file: expected {:?} at byte {}",
            c as char,
            self.i
        );
        self.i += 1;
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn word(&mut self, w: &str) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            true
        } else {
            false
        }
    }

    fn hex4(&mut self) -> u32 {
        let digits = std::str::from_utf8(&self.s[self.i..self.i + 4]).expect("ascii");
        self.i += 4;
        u32::from_str_radix(digits, 16).expect("hex digits after \\u")
    }

    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out: Vec<u16> = Vec::new();
        loop {
            let b = *self.s.get(self.i).expect("unterminated string");
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).expect("unterminated escape");
                    self.i += 1;
                    match e {
                        b'"' => out.push(u16::from(b'"')),
                        b'\\' => out.push(u16::from(b'\\')),
                        b'/' => out.push(u16::from(b'/')),
                        b'n' => out.push(10),
                        b't' => out.push(9),
                        b'r' => out.push(13),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => out.push(u16::try_from(self.hex4()).expect("four hex digits")),
                        other => panic!("vector file: bad escape {:?}", other as char),
                    }
                }
                _ => {
                    // Plain bytes are collected as UTF-8 and re-split into UTF-16 units.
                    let start = self.i - 1;
                    let mut end = self.i;
                    while self.s.get(end).is_some_and(|c| *c != b'"' && *c != b'\\') {
                        end += 1;
                    }
                    let text =
                        std::str::from_utf8(&self.s[start..end]).expect("utf-8 in the vector file");
                    out.extend(text.encode_utf16());
                    self.i = end;
                }
            }
        }
        String::from_utf16(&out).expect("valid UTF-16")
    }

    fn boolean(&mut self) -> bool {
        if self.word("true") {
            true
        } else if self.word("false") {
            false
        } else {
            panic!("vector file: expected a boolean at byte {}", self.i)
        }
    }

    fn object(&mut self) -> Vector {
        self.eat(b'{');
        let (mut name, mut valid, mut hex, mut expect) = (None, None, None, None);
        loop {
            let key = self.string();
            self.eat(b':');
            match key.as_str() {
                "name" => name = Some(self.string()),
                "valid" => valid = Some(self.boolean()),
                "hex" => hex = Some(self.string()),
                "expect" => expect = Some(self.string()),
                other => panic!("vector file: unknown field {other}"),
            }
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b'}');
        Vector {
            name: name.expect("name"),
            valid: valid.expect("valid"),
            hex: hex.expect("hex"),
            expect: expect.expect("expect"),
        }
    }
}

pub fn parse_vectors(text: &str) -> Vec<Vector> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    p.eat(b'[');
    let mut rows = Vec::new();
    if p.peek() != Some(b']') {
        loop {
            rows.push(p.object());
            if p.peek() == Some(b',') {
                p.i += 1;
            } else {
                break;
            }
        }
    }
    p.eat(b']');
    p.ws();
    assert_eq!(p.i, p.s.len(), "vector file: text after the array");
    rows
}
