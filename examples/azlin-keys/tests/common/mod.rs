//! Helpers the test files share: hex, a seeded generator (no `rand`: SplitMix64) and a reader and
//! writer of the vector files (a tiny JSON subset: an array of flat objects whose values are
//! strings or booleans; the crate has no serde and the tests none either).
//!
//! Vector files are written only under `UPDATE_VECTORS=1` and never when `CI` is set; otherwise a
//! test compares its computed rows with the file and fails with the first difference
//! (launch rule 10 of the W1 lanes).
#![allow(dead_code)]

use std::path::PathBuf;

/// SplitMix64: the seeded generator of every property test. A failing case prints its seed and
/// index, and `SplitMix64::new(seed)` replays it.
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

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u64().to_le_bytes()[0]).collect()
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length: {text}");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

pub fn from_hex_array<const N: usize>(text: &str) -> [u8; N] {
    let bytes = from_hex(text);
    <[u8; N]>::try_from(bytes.as_slice()).unwrap_or_else(|_| panic!("{text}: not {N} bytes"))
}

/// One row of a vector file. Every field is a string; a row that has no use for a field leaves it
/// empty. `valid` is always written (`true`: every row of these files is a known answer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub kind: String,
    pub context: String,
    pub hex: String,
    pub extra: String,
    pub expect: String,
}

impl Row {
    pub fn new(name: &str, kind: &str, context: &str, hex: &str, extra: &str, expect: &str) -> Row {
        Row {
            name: name.to_string(),
            kind: kind.to_string(),
            context: context.to_string(),
            hex: hex.to_string(),
            extra: extra.to_string(),
            expect: expect.to_string(),
        }
    }
}

pub fn vector_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors")
        .join(file)
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            other => panic!("vector files hold printable ASCII only, not {other:?}"),
        }
    }
    out.push('"');
    out
}

/// The vector file text of these rows: one object per line, so a diff shows one vector.
pub fn render_json(rows: &[Row]) -> String {
    let mut out = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "  {{\"name\": {}, \"valid\": true, \"kind\": {}, \"context\": {}, \"hex\": {}, \"extra\": {}, \"expect\": {}}}",
            json_str(&r.name),
            json_str(&r.kind),
            json_str(&r.context),
            json_str(&r.hex),
            json_str(&r.extra),
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

    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = String::new();
        loop {
            let b = *self.s.get(self.i).expect("unterminated string");
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).expect("unterminated escape");
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        other => panic!("vector file: bad escape {:?}", other as char),
                    }
                }
                _ => out.push(b as char),
            }
        }
        out
    }

    fn boolean(&mut self) -> bool {
        self.ws();
        for (word, value) in [("true", true), ("false", false)] {
            if self.s[self.i..].starts_with(word.as_bytes()) {
                self.i += word.len();
                return value;
            }
        }
        panic!("vector file: expected a boolean at byte {}", self.i)
    }

    fn object(&mut self) -> Row {
        self.eat(b'{');
        let mut row = Row::new("", "", "", "", "", "");
        let mut seen_valid = false;
        loop {
            let key = self.string();
            self.eat(b':');
            match key.as_str() {
                "name" => row.name = self.string(),
                "valid" => {
                    assert!(self.boolean(), "every row of these files is a valid vector");
                    seen_valid = true;
                }
                "kind" => row.kind = self.string(),
                "context" => row.context = self.string(),
                "hex" => row.hex = self.string(),
                "extra" => row.extra = self.string(),
                "expect" => row.expect = self.string(),
                other => panic!("vector file: unknown key {other:?}"),
            }
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b'}');
        assert!(seen_valid, "vector file: a row without `valid`");
        row
    }

    fn rows(&mut self) -> Vec<Row> {
        self.eat(b'[');
        let mut rows = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return rows;
        }
        loop {
            rows.push(self.object());
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b']');
        rows
    }
}

pub fn parse_json(text: &str) -> Vec<Row> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let rows = p.rows();
    p.ws();
    assert_eq!(p.i, text.len(), "vector file: trailing bytes");
    rows
}

/// Compares `computed` with the vector file `file`, or writes the file when `UPDATE_VECTORS=1`
/// (never when `CI` is set). A difference fails with the first differing row.
pub fn check_or_update(file: &str, computed: &[Row]) {
    let path = vector_path(file);
    if std::env::var_os("UPDATE_VECTORS").is_some_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 refuses to write {file} when CI is set"
        );
        std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("vectors folder");
        std::fs::write(&path, render_json(computed)).expect("write the vector file");
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{file} is missing or unreadable ({e}): UPDATE_VECTORS=1 writes it, never in CI")
    });
    let pinned = parse_json(&text);
    assert_eq!(
        text,
        render_json(&pinned),
        "{file} is not in the canonical form that UPDATE_VECTORS=1 writes"
    );
    for (i, (want, got)) in pinned.iter().zip(computed).enumerate() {
        assert_eq!(
            want, got,
            "{file}: row {i} ({}) differs from what the code computes now (pinned left, computed right)",
            want.name
        );
    }
    assert_eq!(
        pinned.len(),
        computed.len(),
        "{file}: {} pinned rows, the code computes {}",
        pinned.len(),
        computed.len()
    );
}
